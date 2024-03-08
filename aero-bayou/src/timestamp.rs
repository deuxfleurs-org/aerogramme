use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use rand::prelude::*;

/// Returns milliseconds since UNIX Epoch
pub fn now_msec() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Fix your clock :o")
        .as_millis() as u64
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Timestamp {
    pub msec: u64,
    pub rand: u64,
}

impl Timestamp {
    #[allow(dead_code)]
    // 2023-05-15 try to make clippy happy and not sure if this fn will be used in the future.
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
}

impl ToString for Timestamp {
    fn to_string(&self) -> String {
        let mut bytes = [0u8; 16];
        bytes[0..8].copy_from_slice(&u64::to_be_bytes(self.msec));
        bytes[8..16].copy_from_slice(&u64::to_be_bytes(self.rand));
        hex::encode(bytes)
    }
}

impl FromStr for Timestamp {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Timestamp, &'static str> {
        let bytes = hex::decode(s).map_err(|_| "invalid hex")?;
        if bytes.len() != 16 {
            return Err("bad length");
        }
        Ok(Self {
            msec: u64::from_be_bytes(bytes[0..8].try_into().unwrap()),
            rand: u64::from_be_bytes(bytes[8..16].try_into().unwrap()),
        })
    }
}
