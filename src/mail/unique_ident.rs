use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};

use lazy_static::lazy_static;
use rand::prelude::*;
use serde::{de::Error, Deserialize, Deserializer, Serialize, Serializer};

use crate::time::now_msec;

/// An internal Mail Identifier is composed of two components:
/// - a process identifier, 128 bits, itself composed of:
///   - the timestamp of when the process started, 64 bits
///   - a 64-bit random number
/// - a sequence number, 64 bits
/// They are not part of the protocol but an internal representation
/// required by Aerogramme.
/// Their main property is to be unique without having to rely
/// on synchronization between IMAP processes.
#[derive(Clone, Copy, PartialOrd, Ord, PartialEq, Eq, Hash, Debug)]
pub struct UniqueIdent(pub [u8; 24]);

struct IdentGenerator {
    pid: u128,
    sn: AtomicU64,
}

impl IdentGenerator {
    fn new() -> Self {
        let time = now_msec() as u128;
        let rand = thread_rng().gen::<u64>() as u128;
        Self {
            pid: (time << 64) | rand,
            sn: AtomicU64::new(0),
        }
    }

    fn gen(&self) -> UniqueIdent {
        let sn = self.sn.fetch_add(1, Ordering::Relaxed);
        let mut res = [0u8; 24];
        res[0..16].copy_from_slice(&u128::to_be_bytes(self.pid));
        res[16..24].copy_from_slice(&u64::to_be_bytes(sn));
        UniqueIdent(res)
    }
}

lazy_static! {
    static ref GENERATOR: IdentGenerator = IdentGenerator::new();
}

pub fn gen_ident() -> UniqueIdent {
    GENERATOR.gen()
}

// -- serde --

impl<'de> Deserialize<'de> for UniqueIdent {
    fn deserialize<D>(d: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v = String::deserialize(d)?;
        UniqueIdent::from_str(&v).map_err(D::Error::custom)
    }
}

impl Serialize for UniqueIdent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl std::fmt::Display for UniqueIdent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

impl FromStr for UniqueIdent {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<UniqueIdent, &'static str> {
        let bytes = hex::decode(s).map_err(|_| "invalid hex")?;

        if bytes.len() != 24 {
            return Err("bad length");
        }

        let mut tmp = [0u8; 24];
        tmp[..].copy_from_slice(&bytes);
        Ok(UniqueIdent(tmp))
    }
}
