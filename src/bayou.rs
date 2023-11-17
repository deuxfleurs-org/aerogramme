use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use log::{debug, error, info};
use rand::prelude::*;
use serde::{Deserialize, Serialize};
use tokio::sync::{watch, Notify};

use crate::cryptoblob::*;
use crate::login::Credentials;
use crate::timestamp::*;
use crate::storage;


const KEEP_STATE_EVERY: usize = 64;

// Checkpointing interval constants: a checkpoint is not made earlier
// than CHECKPOINT_INTERVAL time after the last one, and is not made
// if there are less than CHECKPOINT_MIN_OPS new operations since last one.
const CHECKPOINT_INTERVAL: Duration = Duration::from_secs(6 * 3600);
const CHECKPOINT_MIN_OPS: usize = 16;
// HYPOTHESIS: processes are able to communicate in a synchronous
// fashion in times that are small compared to CHECKPOINT_INTERVAL.
// More precisely, if a process tried to save an operation within the last
// CHECKPOINT_INTERVAL, we are sure to read it from storage if it was
// successfully saved (and if we don't read it, it means it has been
// definitely discarded due to an error).

// Keep at least two checkpoints, here three, to avoid race conditions
// between processes doing .checkpoint() and those doing .sync()
const CHECKPOINTS_TO_KEEP: usize = 3;

const WATCH_SK: &str = "watch";

pub trait BayouState:
    Default + Clone + Serialize + for<'de> Deserialize<'de> + Send + Sync + 'static
{
    type Op: Clone + Serialize + for<'de> Deserialize<'de> + std::fmt::Debug + Send + Sync + 'static;

    fn apply(&self, op: &Self::Op) -> Self;
}

pub struct Bayou<S: BayouState> {
    path: String,
    key: Key,

    k2v: storage::RowStore,
    s3: storage::BlobStore,

    checkpoint: (Timestamp, S),
    history: Vec<(Timestamp, S::Op, Option<S>)>,

    last_sync: Option<Instant>,
    last_try_checkpoint: Option<Instant>,

    watch: Arc<K2vWatch>,
    last_sync_watch_ct: storage::RowRef,
}

impl<S: BayouState> Bayou<S> {
    pub fn new(creds: &Credentials, path: String) -> Result<Self> {
        let k2v_client = creds.row_client()?;
        let s3_client = creds.blob_client()?;

        let target = k2v_client.row(&path, WATCH_SK);
        let watch = K2vWatch::new(creds, path.clone(), WATCH_SK.to_string())?;

        Ok(Self {
            path,
            key: creds.keys.master.clone(),
            k2v: k2v_client,
            s3: s3_client,
            checkpoint: (Timestamp::zero(), S::default()),
            history: vec![],
            last_sync: None,
            last_try_checkpoint: None,
            watch,
            last_sync_watch_ct: target,
        })
    }

    /// Re-reads the state from persistent storage backend
    pub async fn sync(&mut self) -> Result<()> {
        let new_last_sync = Some(Instant::now());
        let new_last_sync_watch_ct = self.watch.rx.borrow().clone();

        // 1. List checkpoints
        let checkpoints = self.list_checkpoints().await?;
        debug!("(sync) listed checkpoints: {:?}", checkpoints);

        // 2. Load last checkpoint if different from currently used one
        let checkpoint = if let Some((ts, key)) = checkpoints.last() {
            if *ts == self.checkpoint.0 {
                (*ts, None)
            } else {
                debug!("(sync) loading checkpoint: {}", key);

                let obj_res = self.s3.blob(key).fetch().await?;
                let buf = obj_res.content().ok_or(anyhow!("object can't be empty"))?;

                debug!("(sync) checkpoint body length: {}", buf.len());

                let ck = open_deserialize::<S>(&buf, &self.key)?;
                (*ts, Some(ck))
            }
        } else {
            (Timestamp::zero(), None)
        };

        if self.checkpoint.0 > checkpoint.0 {
            bail!("Loaded checkpoint is more recent than stored one");
        }

        if let Some(ck) = checkpoint.1 {
            debug!(
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
        let ts_ser = self.checkpoint.0.to_string();
        debug!("(sync) looking up operations starting at {}", ts_ser);
        let ops_map = self.k2v.select(storage::Selector::Range { shard_key: &self.path, begin: &ts_ser, end: WATCH_SK }).await?;
        /*let ops_map = self
            .k2v
            .read_batch(&[BatchReadOp {
                partition_key: &self.path,
                filter: Filter {
                    start: Some(&ts_ser),
                    end: Some(WATCH_SK),
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
            .items;*/

        let mut ops = vec![];
        for row_value in ops_map {
            let row = row_value.to_ref();
            let sort_key = row.key().1;
            let ts = sort_key.parse::<Timestamp>().map_err(|_| anyhow!("Invalid operation timestamp: {}", sort_key))?;

            let val = row_value.content();
            if val.len() != 1 {
                bail!("Invalid operation, has {} values", row_value.content().len());
            }
            match &val[0] {
                storage::Alternative::Value(v) => {
                    let op = open_deserialize::<S::Op>(v, &self.key)?;
                    debug!("(sync) operation {}: {} {:?}", sort_key, base64::encode(v), op);
                    ops.push((ts, op));
                }
                storage::Alternative::Tombstone => {
                    continue;
                }
            }
        }
        ops.sort_by_key(|(ts, _)| *ts);
        debug!("(sync) {} operations", ops.len());

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
            .zip(ops.iter())
            .take_while(|((ts1, _, _), (ts2, _))| ts1 == ts2)
            .count();

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
                if (self.history.len() + 1) % KEEP_STATE_EVERY == 0 {
                    self.history.push((ts, op, Some(state.clone())));
                } else {
                    self.history.push((ts, op, None));
                }
            }

            // Always save final state as result of last operation
            self.history.last_mut().unwrap().2 = Some(state);
        }

        // Save info that sync has been done
        self.last_sync = new_last_sync;
        self.last_sync_watch_ct = self.k2v.from_orphan(new_last_sync_watch_ct).expect("Source & target storage must be compatible");
        Ok(())
    }

    /// Does a sync() if either of the two conditions is met:
    /// - last sync was more than CHECKPOINT_INTERVAL/5 ago
    /// - a change was detected
    pub async fn opportunistic_sync(&mut self) -> Result<()> {
        let too_old = match self.last_sync {
            Some(t) => Instant::now() > t + (CHECKPOINT_INTERVAL / 5),
            _ => true,
        };
        let changed = self.last_sync_watch_ct.to_orphan() != *self.watch.rx.borrow();
        if too_old || changed {
            self.sync().await?;
        }
        Ok(())
    }

    /// Applies a new operation on the state. Once this function returns,
    /// the operation has been safely persisted to storage backend.
    /// Make sure to call `.opportunistic_sync()` before doing this,
    /// and even before calculating the `op` argument given here.
    pub async fn push(&mut self, op: S::Op) -> Result<()> {
        debug!("(push) add operation: {:?}", op);

        let ts = Timestamp::after(
            self.history
                .last()
                .map(|(ts, _, _)| ts)
                .unwrap_or(&self.checkpoint.0),
        );
        self.k2v
            .row(&self.path, &ts.to_string())
            .set_value(seal_serialize(&op, &self.key)?)
            .push()
            .await?;

        self.watch.notify.notify_one();

        let new_state = self.state().apply(&op);
        self.history.push((ts, op, Some(new_state)));

        // Clear previously saved state in history if not required
        let hlen = self.history.len();
        if hlen >= 2 && (hlen - 1) % KEEP_STATE_EVERY != 0 {
            self.history[hlen - 2].2 = None;
        }

        self.checkpoint().await?;

        Ok(())
    }

    /// Save a new checkpoint if previous checkpoint is too old
    pub async fn checkpoint(&mut self) -> Result<()> {
        match self.last_try_checkpoint {
            Some(ts) if Instant::now() - ts < CHECKPOINT_INTERVAL / 5 => Ok(()),
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
        self.sync().await?;

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
                debug!("(cp) Oldest operation is too recent to trigger checkpoint");
                return Ok(());
            }
        };

        if i_cp < CHECKPOINT_MIN_OPS {
            debug!("(cp) Not enough old operations to trigger checkpoint");
            return Ok(());
        }

        let ts_cp = self.history[i_cp].0;
        debug!(
            "(cp) we could checkpoint at time {} (index {} in history)",
            ts_cp.to_string(),
            i_cp
        );

        // Check existing checkpoints: if last one is too recent, don't checkpoint again.
        let existing_checkpoints = self.list_checkpoints().await?;
        debug!("(cp) listed checkpoints: {:?}", existing_checkpoints);

        if let Some(last_cp) = existing_checkpoints.last() {
            if (ts_cp.msec as i128 - last_cp.0.msec as i128)
                < CHECKPOINT_INTERVAL.as_millis() as i128
            {
                debug!(
                    "(cp) last checkpoint is too recent: {}, not checkpointing",
                    last_cp.0.to_string()
                );
                return Ok(());
            }
        }

        debug!("(cp) saving checkpoint at {}", ts_cp.to_string());

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
        debug!("(cp) checkpoint body length: {}", cryptoblob.len());

        self.s3
            .blob(format!("{}/checkpoint/{}", self.path, ts_cp.to_string()).as_str())
            .set_value(cryptoblob.into())
            .push()
            .await?;


        // Drop old checkpoints (but keep at least CHECKPOINTS_TO_KEEP of them)
        let ecp_len = existing_checkpoints.len();
        if ecp_len + 1 > CHECKPOINTS_TO_KEEP {
            let last_to_keep = ecp_len + 1 - CHECKPOINTS_TO_KEEP;

            // Delete blobs
            for (_ts, key) in existing_checkpoints[..last_to_keep].iter() {
                debug!("(cp) drop old checkpoint {}", key);
                self.s3
                    .blob(key)
                    .rm()
                    .await?;
            }

            // Delete corresponding range of operations
            let ts_ser = existing_checkpoints[last_to_keep].0.to_string();
            self.k2v
                .rm(storage::Selector::Range{
                    shard_key: &self.path, 
                    begin: "", 
                    end: &ts_ser
                })
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

        let checkpoints_res = self.s3.list(&prefix).await?;

        let mut checkpoints = vec![];
        for object in checkpoints_res {
            let key = object.key();
            if let Some(ckid) = key.strip_prefix(&prefix) {
                if let Ok(ts) = ckid.parse::<Timestamp>() {
                    checkpoints.push((ts, key.into()));
                }
            }
        }
        checkpoints.sort_by_key(|(ts, _)| *ts);
        Ok(checkpoints)
    }
}

// ---- Bayou watch in K2V ----

struct K2vWatch {
    pk: String,
    sk: String,
    rx: watch::Receiver<storage::OrphanRowRef>,
    notify: Notify,
}

impl K2vWatch {
    /// Creates a new watch and launches subordinate threads.
    /// These threads hold Weak pointers to the struct;
    /// they exit when the Arc is dropped.
    fn new(creds: &Credentials, pk: String, sk: String) -> Result<Arc<Self>> {
        let row_client = creds.row_client()?;

        let (tx, rx) = watch::channel::<storage::OrphanRowRef>(row_client.row(&pk, &sk).to_orphan());
        let notify = Notify::new();

        let watch = Arc::new(K2vWatch { pk, sk, rx, notify });

        tokio::spawn(Self::background_task(
            Arc::downgrade(&watch),
            row_client,
            tx,
        ));

        Ok(watch)
    }

    async fn background_task(
        self_weak: Weak<Self>,
        k2v: storage::RowStore,
        tx: watch::Sender<storage::OrphanRowRef>,
    ) {
        let mut row = match Weak::upgrade(&self_weak) {
            Some(this) => k2v.row(&this.pk, &this.sk),
            None => {
                error!("can't start loop");
                return
            },
        };

        while let Some(this) = Weak::upgrade(&self_weak) {
            debug!(
                "bayou k2v watch bg loop iter ({}, {})",
                this.pk, this.sk
            );
            tokio::select!(
                _ = tokio::time::sleep(Duration::from_secs(60)) => continue,
                update = row.poll() => {
                //update = k2v_wait_value_changed(&k2v, &this.pk, &this.sk, &ct) => {
                    match update {
                        Err(e) => {
                            error!("Error in bayou k2v wait value changed: {}", e);
                            tokio::time::sleep(Duration::from_secs(30)).await;
                        }
                        Ok(new_value) => {
                            row = new_value.to_ref();
                            if tx.send(row.to_orphan()).is_err() {
                                break;
                            }
                        }
                    }
                }
                _ = this.notify.notified() => {
                    let rand = u128::to_be_bytes(thread_rng().gen()).to_vec();
                    if let Err(e) = row.set_value(rand).push().await
                    {
                        error!("Error in bayou k2v watch updater loop: {}", e);
                        tokio::time::sleep(Duration::from_secs(30)).await;
                    }
                }
            );
        }
        info!("bayou k2v watch bg loop exiting");
    }
}
