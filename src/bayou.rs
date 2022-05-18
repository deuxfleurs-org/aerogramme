use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use rand::prelude::*;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;

use k2v_client::{BatchDeleteOp, BatchReadOp, Filter, K2vClient, K2vValue};
use rusoto_core::HttpClient;
use rusoto_credential::{AwsCredentials, StaticProvider};
use rusoto_s3::{
    DeleteObjectRequest, GetObjectRequest, ListObjectsV2Request, PutObjectRequest, S3Client, S3,
};
use rusoto_signature::Region;

use crate::cryptoblob::*;
use crate::time::now_msec;

const SAVE_STATE_EVERY: usize = 64;

// Checkpointing interval constants: a checkpoint is not made earlier
// than CHECKPOINT_INTERVAL time after the last one, and is not made
// if there are less than CHECKPOINT_MIN_OPS new operations since last one.
const CHECKPOINT_INTERVAL: Duration = Duration::from_secs(60);
const CHECKPOINT_MIN_OPS: usize = 4;
// HYPOTHESIS: processes are able to communicate in a synchronous
// fashion in times that are small compared to CHECKPOINT_INTERVAL.
// More precisely, if a process tried to save an operation within the last
// CHECKPOINT_INTERVAL, we are sure to read it from storage if it was
// successfully saved (and if we don't read it, it means it has been
// definitely discarded due to an error).

// Keep at least two checkpoints, here three, to avoid race conditions
// between processes doing .checkpoint() and those doing .sync()
const CHECKPOINTS_TO_KEEP: usize = 3;

pub trait BayouState:
    Default + Clone + Serialize + for<'de> Deserialize<'de> + Send + Sync + 'static
{
    type Op: Clone + Serialize + for<'de> Deserialize<'de> + std::fmt::Debug + Send + Sync + 'static;

    fn apply(&self, op: &Self::Op) -> Self;
}

pub struct Bayou<S: BayouState> {
    bucket: String,
    path: String,
    key: Key,

    k2v: K2vClient,
    s3: S3Client,

    checkpoint: (Timestamp, S),
    history: Vec<(Timestamp, S::Op, Option<S>)>,
    last_sync: Option<Instant>,
    last_try_checkpoint: Option<Instant>,
}

impl<S: BayouState> Bayou<S> {
    pub fn new(
        creds: AwsCredentials,
        k2v_region: Region,
        s3_region: Region,
        bucket: String,
        path: String,
        key: Key,
    ) -> Result<Self> {
        let k2v_client = K2vClient::new(k2v_region, bucket.clone(), creds.clone(), None)?;
        let static_creds = StaticProvider::new(
            creds.aws_access_key_id().to_string(),
            creds.aws_secret_access_key().to_string(),
            creds.token().clone(),
            None,
        );
        let s3_client = S3Client::new_with(HttpClient::new()?, static_creds, s3_region);

        Ok(Self {
            bucket,
            path,
            key,
            k2v: k2v_client,
            s3: s3_client,
            checkpoint: (Timestamp::zero(), S::default()),
            history: vec![],
            last_sync: None,
            last_try_checkpoint: None,
        })
    }

    /// Re-reads the state from persistent storage backend
    pub async fn sync(&mut self) -> Result<()> {
        // 1. List checkpoints
        let checkpoints = self.list_checkpoints().await?;
        eprintln!("(sync) listed checkpoints: {:?}", checkpoints);

        // 2. Load last checkpoint if different from currently used one
        let checkpoint = if let Some((ts, key)) = checkpoints.last() {
            if *ts == self.checkpoint.0 {
                (*ts, None)
            } else {
                eprintln!("(sync) loading checkpoint: {}", key);

                let mut gor = GetObjectRequest::default();
                gor.bucket = self.bucket.clone();
                gor.key = key.to_string();
                let obj_res = self.s3.get_object(gor).await?;

                let obj_body = obj_res.body.ok_or(anyhow!("Missing object body"))?;
                let mut buf = Vec::with_capacity(obj_res.content_length.unwrap_or(128) as usize);
                obj_body.into_async_read().read_to_end(&mut buf).await?;

                eprintln!("(sync) checkpoint body length: {}", buf.len());

                let ck = open_deserialize::<S>(&buf, &self.key)?;
                (*ts, Some(ck))
            }
        } else {
            (Timestamp::zero(), None)
        };

        if self.checkpoint.0 > checkpoint.0 {
            bail!("Existing checkpoint is more recent than stored one");
        }

        if let Some(ck) = checkpoint.1 {
            eprintln!(
                "(sync) updating checkpoint to loaded state at {:?}",
                checkpoint.0
            );
            self.checkpoint = (checkpoint.0, ck);
        };

        // remove from history events before checkpoint
        self.history = std::mem::take(&mut self.history)
            .into_iter()
            .skip_while(|(ts, _, _)| *ts < self.checkpoint.0)
            .collect();

        // 3. List all operations starting from checkpoint
        let ts_ser = self.checkpoint.0.serialize();
        eprintln!("(sync) looking up operations starting at {}", ts_ser);
        let ops_map = self
            .k2v
            .read_batch(&[BatchReadOp {
                partition_key: &self.path,
                filter: Filter {
                    start: Some(&ts_ser),
                    end: None,
                    prefix: None,
                    limit: None,
                    reverse: false,
                },
                single_item: false,
                conflicts_only: false,
                tombstones: false,
            }])
            .await?
            .into_iter()
            .next()
            .ok_or(anyhow!("Missing K2V result"))?
            .items;

        let mut ops = vec![];
        for (tsstr, val) in ops_map {
            let ts = Timestamp::parse(&tsstr)
                .ok_or(anyhow!("Invalid operation timestamp: {}", tsstr))?;
            if val.value.len() != 1 {
                bail!("Invalid operation, has {} values", val.value.len());
            }
            match &val.value[0] {
                K2vValue::Value(v) => {
                    let op = open_deserialize::<S::Op>(&v, &self.key)?;
                    eprintln!("(sync) operation {}: {} {:?}", tsstr, base64::encode(v), op);
                    ops.push((ts, op));
                }
                K2vValue::Tombstone => {
                    unreachable!();
                }
            }
        }
        ops.sort_by_key(|(ts, _)| *ts);
        eprintln!("(sync) {} operations", ops.len());

        if ops.len() < self.history.len() {
            bail!("Some operations have disappeared from storage!");
        }

        // 4. Check that first operation has same timestamp as checkpoint (if not zero)
        if self.checkpoint.0 != Timestamp::zero() && ops[0].0 != self.checkpoint.0 {
            bail!(
                "First operation in listing doesn't have timestamp that corresponds to checkpoint"
            );
        }

        // 5. Apply all operations in order
        // Hypothesis: before the loaded checkpoint, operations haven't changed
        // between what's on storage and what we used to calculate the state in RAM here.
        let i0 = self
            .history
            .iter()
            .enumerate()
            .zip(ops.iter())
            .skip_while(|((_, (ts1, _, _)), (ts2, _))| ts1 == ts2)
            .map(|((i, _), _)| i)
            .next()
            .unwrap_or(self.history.len());

        if ops.len() > i0 {
            // Remove operations from first position where histories differ
            self.history.truncate(i0);

            // Look up last calculated state which we have saved and start from there.
            let mut last_state = (0, &self.checkpoint.1);
            for (i, (_, _, state_opt)) in self.history.iter().enumerate().rev() {
                if let Some(state) = state_opt {
                    last_state = (i + 1, state);
                    break;
                }
            }

            // Calculate state at the end of this common part of the history
            let mut state = last_state.1.clone();
            for (_, op, _) in self.history[last_state.0..].iter() {
                state = state.apply(op);
            }

            // Now, apply all operations retrieved from storage after the common part
            for (ts, op) in ops.drain(i0..) {
                state = state.apply(&op);
                if (self.history.len() + 1) % SAVE_STATE_EVERY == 0 {
                    self.history.push((ts, op, Some(state.clone())));
                } else {
                    self.history.push((ts, op, None));
                }
            }

            // Always save final state as result of last operation
            self.history.last_mut().unwrap().2 = Some(state);
        }

        self.last_sync = Some(Instant::now());
        Ok(())
    }

    async fn check_recent_sync(&mut self) -> Result<()> {
        match self.last_sync {
            Some(t) if (Instant::now() - t) < CHECKPOINT_INTERVAL / 10 => Ok(()),
            _ => self.sync().await,
        }
    }

    /// Applies a new operation on the state. Once this function returns,
    /// the option has been safely persisted to storage backend
    pub async fn push(&mut self, op: S::Op) -> Result<()> {
        self.check_recent_sync().await?;

        eprintln!("(push) add operation: {:?}", op);

        let ts = Timestamp::after(
            self.history
                .last()
                .map(|(ts, _, _)| ts)
                .unwrap_or(&self.checkpoint.0),
        );
        self.k2v
            .insert_item(
                &self.path,
                &ts.serialize(),
                seal_serialize(&op, &self.key)?,
                None,
            )
            .await?;

        let new_state = self.state().apply(&op);
        self.history.push((ts, op, Some(new_state)));

        // Clear previously saved state in history if not required
        let hlen = self.history.len();
        if hlen >= 2 && (hlen - 1) % SAVE_STATE_EVERY != 0 {
            self.history[hlen - 2].2 = None;
        }

        self.checkpoint().await?;

        Ok(())
    }

    /// Save a new checkpoint if previous checkpoint is too old
    pub async fn checkpoint(&mut self) -> Result<()> {
        match self.last_try_checkpoint {
            Some(ts) if Instant::now() - ts < CHECKPOINT_INTERVAL / 10 => Ok(()),
            _ => {
                let res = self.checkpoint_internal().await;
                if res.is_ok() {
                    self.last_try_checkpoint = Some(Instant::now());
                }
                res
            }
        }
    }

    async fn checkpoint_internal(&mut self) -> Result<()> {
        self.check_recent_sync().await?;

        // Check what would be the possible time for a checkpoint in the history we have
        let now = now_msec() as i128;
        let i_cp = match self
            .history
            .iter()
            .enumerate()
            .rev()
            .skip_while(|(_, (ts, _, _))| {
                (now - ts.msec as i128) < CHECKPOINT_INTERVAL.as_millis() as i128
            })
            .map(|(i, _)| i)
            .next()
        {
            Some(i) => i,
            None => {
                eprintln!("(cp) Oldest operation is too recent to trigger checkpoint");
                return Ok(());
            }
        };

        if i_cp < CHECKPOINT_MIN_OPS {
            eprintln!("(cp) Not enough old operations to trigger checkpoint");
            return Ok(());
        }

        let ts_cp = self.history[i_cp].0;
        eprintln!(
            "(cp) we could checkpoint at time {} (index {} in history)",
            ts_cp.serialize(),
            i_cp
        );

        // Check existing checkpoints: if last one is too recent, don't checkpoint again.
        let existing_checkpoints = self.list_checkpoints().await?;
        eprintln!("(cp) listed checkpoints: {:?}", existing_checkpoints);

        if let Some(last_cp) = existing_checkpoints.last() {
            if (ts_cp.msec as i128 - last_cp.0.msec as i128)
                < CHECKPOINT_INTERVAL.as_millis() as i128
            {
                eprintln!(
                    "(cp) last checkpoint is too recent: {}, not checkpointing",
                    last_cp.0.serialize()
                );
                return Ok(());
            }
        }

        eprintln!("(cp) saving checkpoint at {}", ts_cp.serialize());

        // Calculate state at time of checkpoint
        let mut last_known_state = (0, &self.checkpoint.1);
        for (i, (_, _, st)) in self.history[..i_cp].iter().enumerate() {
            if let Some(s) = st {
                last_known_state = (i + 1, s);
            }
        }
        let mut state_cp = last_known_state.1.clone();
        for (_, op, _) in self.history[last_known_state.0..i_cp].iter() {
            state_cp = state_cp.apply(op);
        }

        // Serialize and save checkpoint
        let cryptoblob = seal_serialize(&state_cp, &self.key)?;
        eprintln!("(cp) checkpoint body length: {}", cryptoblob.len());

        let mut por = PutObjectRequest::default();
        por.bucket = self.bucket.clone();
        por.key = format!("{}/checkpoint/{}", self.path, ts_cp.serialize());
        por.body = Some(cryptoblob.into());
        self.s3.put_object(por).await?;

        // Drop old checkpoints (but keep at least CHECKPOINTS_TO_KEEP of them)
        let ecp_len = existing_checkpoints.len();
        if ecp_len + 1 > CHECKPOINTS_TO_KEEP {
            let last_to_keep = ecp_len + 1 - CHECKPOINTS_TO_KEEP;

            // Delete blobs
            for (_ts, key) in existing_checkpoints[..last_to_keep].iter() {
                eprintln!("(cp) drop old checkpoint {}", key);
                let mut dor = DeleteObjectRequest::default();
                dor.bucket = self.bucket.clone();
                dor.key = key.to_string();
                self.s3.delete_object(dor).await?;
            }

            // Delete corresponding range of operations
            let ts_ser = existing_checkpoints[last_to_keep].0.serialize();
            self.k2v
                .delete_batch(&[BatchDeleteOp {
                    partition_key: &self.path,
                    prefix: None,
                    start: None,
                    end: Some(&ts_ser),
                    single_item: false,
                }])
                .await?;
        }

        Ok(())
    }

    pub fn state(&self) -> &S {
        if let Some(last) = self.history.last() {
            last.2.as_ref().unwrap()
        } else {
            &self.checkpoint.1
        }
    }

    // ---- INTERNAL ----

    async fn list_checkpoints(&self) -> Result<Vec<(Timestamp, String)>> {
        let prefix = format!("{}/checkpoint/", self.path);

        let mut lor = ListObjectsV2Request::default();
        lor.bucket = self.bucket.clone();
        lor.max_keys = Some(1000);
        lor.prefix = Some(prefix.clone());

        let checkpoints_res = self.s3.list_objects_v2(lor).await?;

        let mut checkpoints = vec![];
        for object in checkpoints_res.contents.unwrap_or_default() {
            if let Some(key) = object.key {
                if let Some(ckid) = key.strip_prefix(&prefix) {
                    if let Some(ts) = Timestamp::parse(ckid) {
                        checkpoints.push((ts, key));
                    }
                }
            }
        }
        checkpoints.sort_by_key(|(ts, _)| *ts);
        Ok(checkpoints)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Timestamp {
    pub msec: u64,
    pub rand: u64,
}

impl Timestamp {
    pub fn now() -> Self {
        let mut rng = thread_rng();
        Self {
            msec: now_msec(),
            rand: rng.gen::<u64>(),
        }
    }

    pub fn after(other: &Self) -> Self {
        let mut rng = thread_rng();
        Self {
            msec: std::cmp::max(now_msec(), other.msec + 1),
            rand: rng.gen::<u64>(),
        }
    }

    pub fn zero() -> Self {
        Self { msec: 0, rand: 0 }
    }

    pub fn serialize(&self) -> String {
        let mut bytes = [0u8; 16];
        bytes[0..8].copy_from_slice(&u64::to_be_bytes(self.msec));
        bytes[8..16].copy_from_slice(&u64::to_be_bytes(self.rand));
        hex::encode(&bytes)
    }

    pub fn parse(v: &str) -> Option<Self> {
        let bytes = hex::decode(v).ok()?;
        if bytes.len() != 16 {
            return None;
        }
        Some(Self {
            msec: u64::from_be_bytes(bytes[0..8].try_into().unwrap()),
            rand: u64::from_be_bytes(bytes[8..16].try_into().unwrap()),
        })
    }
}
