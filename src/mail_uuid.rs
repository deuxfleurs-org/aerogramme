use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};

use lazy_static::lazy_static;
use rand::prelude::*;
use serde::{de::Error, Deserialize, Deserializer, Serialize, Serializer};

use crate::time::now_msec;

/// A Mail UUID is composed of two components:
/// - a process identifier, 128 bits, itself composed of:
///   - the timestamp of when the process started, 64 bits
///   - a 64-bit random number
/// - a sequence number, 64 bits
#[derive(Clone, Copy, PartialOrd, Ord, PartialEq, Eq, Debug)]
pub struct MailUuid(pub [u8; 24]);

struct UuidGenerator {
    pid: u128,
    sn: AtomicU64,
}

impl UuidGenerator {
    fn new() -> Self {
        let time = now_msec() as u128;
        let rand = thread_rng().gen::<u64>() as u128;
        Self {
            pid: (time << 64) | rand,
            sn: AtomicU64::new(0),
        }
    }

    fn gen(&self) -> MailUuid {
        let sn = self.sn.fetch_add(1, Ordering::Relaxed);
        let mut res = [0u8; 24];
        res[0..16].copy_from_slice(&u128::to_be_bytes(self.pid));
        res[16..24].copy_from_slice(&u64::to_be_bytes(sn));
        MailUuid(res)
    }
}

lazy_static! {
    static ref GENERATOR: UuidGenerator = UuidGenerator::new();
}

pub fn gen_uuid() -> MailUuid {
    GENERATOR.gen()
}

// -- serde --

impl<'de> Deserialize<'de> for MailUuid {
    fn deserialize<D>(d: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v = String::deserialize(d)?;
        MailUuid::from_str(&v).map_err(D::Error::custom)
    }
}

impl Serialize for MailUuid {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl ToString for MailUuid {
    fn to_string(&self) -> String {
        hex::encode(self.0)
    }
}

impl FromStr for MailUuid {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<MailUuid, &'static str> {
        let bytes = hex::decode(s).map_err(|_| "invalid hex")?;

        if bytes.len() != 24 {
            return Err("bad length");
        }

        let mut tmp = [0u8; 24];
        tmp[..].copy_from_slice(&bytes);
        Ok(MailUuid(tmp))
    }
}
