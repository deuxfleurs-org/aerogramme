pub mod ldap_provider;
pub mod static_provider;

use std::sync::Arc;
use base64::Engine;

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use rand::prelude::*;

use crate::cryptoblob::*;
use crate::storage::*;

/// The trait LoginProvider defines the interface for a login provider that allows
/// to retrieve storage and cryptographic credentials for access to a user account
/// from their username and password.
#[async_trait]
pub trait LoginProvider {
    /// The login method takes an account's password as an input to decypher
    /// decryption keys and obtain full access to the user's account.
    async fn login(&self, username: &str, password: &str) -> Result<Credentials>;
    /// The public_login method takes an account's email address and returns
    /// public credentials for adding mails to the user's inbox.
    async fn public_login(&self, email: &str) -> Result<PublicCredentials>;
}

/// ArcLoginProvider is simply an alias on a structure that is used
/// in many places in the code
pub type ArcLoginProvider = Arc<dyn LoginProvider + Send + Sync>;

/// The struct Credentials represent all of the necessary information to interact
/// with a user account's data after they are logged in.
#[derive(Clone, Debug)]
pub struct Credentials {
    /// The storage credentials are used to authenticate access to the underlying storage (S3, K2V)
    pub storage: Builder,
    /// The cryptographic keys are used to encrypt and decrypt data stored in S3 and K2V
    pub keys: CryptoKeys,
}

#[derive(Clone, Debug)]
pub struct PublicCredentials {
    /// The storage credentials are used to authenticate access to the underlying storage (S3, K2V)
    pub storage: Builder,
    pub public_key: PublicKey,
}

use serde::{Serialize, Deserialize};
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CryptoRoot(pub String);

impl CryptoRoot {
    pub fn create_pass(password: &str, k: &CryptoKeys) -> Result<Self> {
        let bytes = k.password_seal(password)?;
        let b64 = base64::engine::general_purpose::STANDARD_NO_PAD.encode(bytes);
        let cr = format!("aero:cryptoroot:pass:{}", b64);
        Ok(Self(cr))
    }

    pub fn create_cleartext(k: &CryptoKeys) -> Self {
        let bytes = k.serialize();
        let b64 = base64::engine::general_purpose::STANDARD_NO_PAD.encode(bytes);
        let cr = format!("aero:cryptoroot:cleartext:{}", b64);
        Self(cr)
    }

    pub fn create_incoming(pk: &PublicKey) -> Self {
        let bytes: &[u8] = &pk[..];
        let b64 = base64::engine::general_purpose::STANDARD_NO_PAD.encode(bytes);
        let cr = format!("aero:cryptoroot:incoming:{}", b64);
        Self(cr)
    }

    pub fn public_key(&self) -> Result<PublicKey> {
        match self.0.splitn(4, ':').collect::<Vec<&str>>()[..] {
            [ "aero", "cryptoroot", "pass", b64blob ] => {
                let blob = base64::engine::general_purpose::STANDARD_NO_PAD.decode(b64blob)?;
                if blob.len() < 32 {
                    bail!("Decoded data is {} bytes long, expect at least 32 bytes", blob.len());
                }
                PublicKey::from_slice(&blob[..32]).context("must be a valid public key")
            },
            [ "aero", "cryptoroot", "cleartext", b64blob ] => {
                let blob = base64::engine::general_purpose::STANDARD_NO_PAD.decode(b64blob)?;
                Ok(CryptoKeys::deserialize(&blob)?.public)
            },
            [ "aero", "cryptoroot", "incoming", b64blob ] => {
                let blob = base64::engine::general_purpose::STANDARD_NO_PAD.decode(b64blob)?;
                if blob.len() < 32 {
                    bail!("Decoded data is {} bytes long, expect at least 32 bytes", blob.len());
                }
                PublicKey::from_slice(&blob[..32]).context("must be a valid public key")
            },
            [ "aero", "cryptoroot", "keyring", _ ] => {
                bail!("keyring is not yet implemented!")
            },
            _ => bail!(format!("passed string '{}' is not a valid cryptoroot", self.0)),
        }
    }
    pub fn crypto_keys(&self, password: &str) -> Result<CryptoKeys> {
        match self.0.splitn(4, ':').collect::<Vec<&str>>()[..] {
            [ "aero", "cryptoroot", "pass", b64blob ] => {
                let blob = base64::engine::general_purpose::STANDARD_NO_PAD.decode(b64blob)?;
                CryptoKeys::password_open(password, &blob)
            },
            [ "aero", "cryptoroot", "cleartext", b64blob ] => {
                let blob = base64::engine::general_purpose::STANDARD_NO_PAD.decode(b64blob)?;
                CryptoKeys::deserialize(&blob)
            },
            [ "aero", "cryptoroot", "incoming", _ ] => {
                bail!("incoming cryptoroot does not contain a crypto key!")
            },
            [ "aero", "cryptoroot", "keyring", _ ] =>{
                bail!("keyring is not yet implemented!")
            },
            _ => bail!(format!("passed string '{}' is not a valid cryptoroot", self.0)),
        }
    }
}

/// The struct CryptoKeys contains the cryptographic keys used to encrypt and decrypt
/// data in a user's mailbox.
#[derive(Clone, Debug)]
pub struct CryptoKeys {
    /// Master key for symmetric encryption of mailbox data
    pub master: Key,
    /// Public/private keypair for encryption of incomming emails (secret part)
    pub secret: SecretKey,
    /// Public/private keypair for encryption of incomming emails (public part)
    pub public: PublicKey,
}

// ----




impl CryptoKeys {
    /// Initialize a new cryptography root
    pub fn init() -> Self {
        let (public, secret) = gen_keypair();
        let master = gen_key();
        CryptoKeys {
            master,
            secret,
            public,
        }
    }

    // Clear text serialize/deserialize
    /// Serialize the root as bytes without encryption
    fn serialize(&self) -> [u8; 64] {
        let mut res = [0u8; 64];
        res[..32].copy_from_slice(self.master.as_ref());
        res[32..].copy_from_slice(self.secret.as_ref());
        res
    }

    /// Deserialize a clear text crypto root without encryption
    fn deserialize(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != 64 {
            bail!("Invalid length: {}, expected 64", bytes.len());
        }
        let master = Key::from_slice(&bytes[..32]).unwrap();
        let secret = SecretKey::from_slice(&bytes[32..]).unwrap();
        let public = secret.public_key();
        Ok(Self {
            master,
            secret,
            public,
        })
    }

    // Password sealed keys serialize/deserialize
    pub fn password_open(password: &str, blob: &[u8]) -> Result<Self> {
        let _pubkey = &blob[0..32];
        let kdf_salt = &blob[32..64];
        let password_openned = try_open_encrypted_keys(kdf_salt, password, &blob[64..])?;

        let keys = Self::deserialize(&password_openned)?;
        Ok(keys)
    }

    pub fn password_seal(&self, password: &str) -> Result<Vec<u8>> {
        let mut kdf_salt = [0u8; 32];
        thread_rng().fill(&mut kdf_salt);

        // Calculate key for password secret box
        let password_key = derive_password_key(&kdf_salt, password)?;

        // Seal a secret box that contains our crypto keys
        let password_sealed = seal(&self.serialize(), &password_key)?;

        // Create blob
        let password_blob = [&self.public[..], &kdf_salt[..], &password_sealed].concat();

        Ok(password_blob)
    }
}

fn derive_password_key(kdf_salt: &[u8], password: &str) -> Result<Key> {
    Ok(Key::from_slice(&argon2_kdf(kdf_salt, password.as_bytes(), 32)?).unwrap())
}

fn try_open_encrypted_keys(kdf_salt: &[u8], password: &str, encrypted_keys: &[u8]) -> Result<Vec<u8>> {
    let password_key = derive_password_key(kdf_salt, password)?;
    open(encrypted_keys, &password_key)
}

// ---- UTIL ----

pub fn argon2_kdf(salt: &[u8], password: &[u8], output_len: usize) -> Result<Vec<u8>> {
    use argon2::{Algorithm, Argon2, ParamsBuilder, PasswordHasher, Version};

    let mut params = ParamsBuilder::new();
    params
        .output_len(output_len)
        .map_err(|e| anyhow!("Invalid output length: {}", e))?;

    let params = params
        .params()
        .map_err(|e| anyhow!("Invalid argon2 params: {}", e))?;
    let argon2 = Argon2::new(Algorithm::default(), Version::default(), params);

    let salt = base64::engine::general_purpose::STANDARD_NO_PAD.encode(salt);
    let hash = argon2
        .hash_password(password, &salt)
        .map_err(|e| anyhow!("Unable to hash: {}", e))?;

    let hash = hash.hash.ok_or(anyhow!("Missing output"))?;
    assert!(hash.len() == output_len);
    Ok(hash.as_bytes().to_vec())
}
