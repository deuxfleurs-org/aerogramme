use std::time::Duration;

use anyhow::Result;
use rand::prelude::*;
use serde::{Deserialize, Serialize};

use k2v_client::K2vClient;
use rusoto_core::HttpClient;
use rusoto_credential::{AwsCredentials, StaticProvider};
use rusoto_s3::S3Client;
use rusoto_signature::Region;

use crate::cryptoblob::Key;
use crate::time::now_msec;

pub trait BayouState:
    Default + Clone + Serialize + for<'de> Deserialize<'de> + Send + Sync + 'static
{
    type Op: Clone + Serialize + for<'de> Deserialize<'de> + Send + Sync + 'static;

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
        })
    }

    /// Re-reads the state from persistent storage backend
    pub async fn sync(&mut self) -> Result<()> {
        // 1. List checkpoints
        // 2. Load last checkpoint if different from currently used one
        // 3. List all operations starting from checkpoint
        // 4. Check that first operation has same timestamp as checkpoint (if not zero)
        // 5. Apply all operations in order
        unimplemented!()
    }

    /// Applies a new operation on the state. Once this function returns,
    /// the option has been safely persisted to storage backend
    pub async fn push(&mut self, op: S::Op) -> Result<()> {
        unimplemented!()
    }

    pub fn state(&self) -> &S {
        if let Some(last) = self.history.last() {
            last.2.as_ref().unwrap()
        } else {
            &self.checkpoint.1
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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
