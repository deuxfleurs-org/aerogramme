//! Helper functions for secret-key encrypted blobs
//! that contain Zstd encrypted data

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use zstd::stream::{decode_all as zstd_decode, encode_all as zstd_encode};

use sodiumoxide::crypto::secretbox::xsalsa20poly1305 as secretbox;
use sodiumoxide::crypto::box_ as publicbox;

pub use sodiumoxide::crypto::secretbox::xsalsa20poly1305::{gen_key, Key, KEYBYTES};
pub use sodiumoxide::crypto::box_::{gen_keypair, PublicKey, SecretKey, PUBLICKEYBYTES, SECRETKEYBYTES};

pub fn open(cryptoblob: &[u8], key: &Key) -> Result<Vec<u8>> {
    use secretbox::{NONCEBYTES, Nonce};

    if cryptoblob.len() < NONCEBYTES {
        return Err(anyhow!("Cyphertext too short"));
    }

    // Decrypt -> get Zstd data
    let nonce = Nonce::from_slice(&cryptoblob[..NONCEBYTES]).unwrap();
    let zstdblob = secretbox::open(&cryptoblob[NONCEBYTES..], &nonce, key)
        .map_err(|_| anyhow!("Could not decrypt blob"))?;

    // Decompress zstd data
    let mut reader = &zstdblob[..];
    let data = zstd_decode(&mut reader)?;

    Ok(data)
}

pub fn seal(plainblob: &[u8], key: &Key) -> Result<Vec<u8>> {
    use secretbox::{NONCEBYTES, gen_nonce};

    // Compress data using zstd
    let mut reader = &plainblob[..];
    let zstdblob = zstd_encode(&mut reader, 0)?;

    // Encrypt
    let nonce = gen_nonce();
    let cryptoblob = secretbox::seal(&zstdblob, &nonce, key);

    let mut res = Vec::with_capacity(NONCEBYTES + cryptoblob.len());
    res.extend(nonce.as_ref());
    res.extend(cryptoblob);

    Ok(res)
}

pub fn open_deserialize<T: for<'de> Deserialize<'de>>(cryptoblob: &[u8], key: &Key) -> Result<T> {
    let blob = open(cryptoblob, key)?;

    Ok(rmp_serde::decode::from_read_ref::<_, T>(&blob)?)
}

pub fn seal_serialize<T: Serialize>(obj: T, key: &Key) -> Result<Vec<u8>> {
    let mut wr = Vec::with_capacity(128);
    let mut se = rmp_serde::Serializer::new(&mut wr)
        .with_struct_map()
        .with_string_variants();
    obj.serialize(&mut se)?;

    Ok(seal(&wr, key)?)
}
