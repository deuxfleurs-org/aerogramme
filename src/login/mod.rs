pub mod ldap_provider;
pub mod static_provider;

use std::collections::BTreeMap;

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use k2v_client::{
    BatchInsertOp, BatchReadOp, CausalValue, CausalityToken, Filter, K2vClient, K2vValue,
};
use rand::prelude::*;
use rusoto_core::HttpClient;
use rusoto_credential::{AwsCredentials, StaticProvider};
use rusoto_s3::S3Client;
use rusoto_signature::Region;

use crate::cryptoblob::*;

#[async_trait]
pub trait LoginProvider {
    async fn login(&self, username: &str, password: &str) -> Result<Credentials>;
}

#[derive(Clone, Debug)]
pub struct Credentials {
    pub storage: StorageCredentials,
    pub keys: CryptoKeys,
}

#[derive(Clone, Debug)]
pub struct StorageCredentials {
    pub s3_region: Region,
    pub k2v_region: Region,

    pub aws_access_key_id: String,
    pub aws_secret_access_key: String,
    pub bucket: String,
}

#[derive(Clone, Debug)]
pub struct CryptoKeys {
    // Master key for symmetric encryption of mailbox data
    pub master: Key,
    // Public/private keypair for encryption of incomming emails
    pub secret: SecretKey,
    pub public: PublicKey,
}

// ----

impl Credentials {
    pub fn k2v_client(&self) -> Result<K2vClient> {
        self.storage.k2v_client()
    }
    pub fn s3_client(&self) -> Result<S3Client> {
        self.storage.s3_client()
    }
    pub fn bucket(&self) -> &str {
        self.storage.bucket.as_str()
    }
}

impl StorageCredentials {
    pub fn k2v_client(&self) -> Result<K2vClient> {
        let aws_creds = AwsCredentials::new(
            self.aws_access_key_id.clone(),
            self.aws_secret_access_key.clone(),
            None,
            None,
        );

        Ok(K2vClient::new(
            self.k2v_region.clone(),
            self.bucket.clone(),
            aws_creds,
            None,
        )?)
    }

    pub fn s3_client(&self) -> Result<S3Client> {
        let aws_creds_provider = StaticProvider::new_minimal(
            self.aws_access_key_id.clone(),
            self.aws_secret_access_key.clone(),
        );

        Ok(S3Client::new_with(
            HttpClient::new()?,
            aws_creds_provider,
            self.s3_region.clone(),
        ))
    }
}

impl CryptoKeys {
    pub async fn init(storage: &StorageCredentials, password: &str) -> Result<Self> {
        // Check that salt and public don't exist already
        let k2v = storage.k2v_client()?;
        Self::check_uninitialized(&k2v).await?;

        // Generate salt for password identifiers
        let mut ident_salt = [0u8; 32];
        thread_rng().fill(&mut ident_salt);

        // Generate (public, private) key pair and master key
        let (public, secret) = gen_keypair();
        let master = gen_key();
        let keys = CryptoKeys {
            master,
            secret,
            public,
        };

        // Generate short password digest (= password identity)
        let ident = argon2_kdf(&ident_salt, password.as_bytes(), 16)?;

        // Generate salt for KDF
        let mut kdf_salt = [0u8; 32];
        thread_rng().fill(&mut kdf_salt);

        // Calculate key for password secret box
        let password_key =
            Key::from_slice(&argon2_kdf(&kdf_salt, password.as_bytes(), 32)?).unwrap();

        // Seal a secret box that contains our crypto keys
        let password_sealed = seal(&keys.serialize(), &password_key)?;

        let password_sortkey = format!("password:{}", hex::encode(&ident));
        let password_blob = [&kdf_salt[..], &password_sealed].concat();

        // Write values to storage
        k2v.insert_batch(&[
            k2v_insert_single_key("keys", "salt", None, &ident_salt),
            k2v_insert_single_key("keys", "public", None, &keys.public),
            k2v_insert_single_key("keys", &password_sortkey, None, &password_blob),
        ])
        .await
        .context("InsertBatch for salt, public, and password")?;

        Ok(keys)
    }

    pub async fn init_without_password(
        storage: &StorageCredentials,
        master: &Key,
        secret: &SecretKey,
    ) -> Result<Self> {
        // Check that salt and public don't exist already
        let k2v = storage.k2v_client()?;
        Self::check_uninitialized(&k2v).await?;

        // Generate salt for password identifiers
        let mut ident_salt = [0u8; 32];
        thread_rng().fill(&mut ident_salt);

        // Create CryptoKeys struct from given keys
        let public = secret.public_key();
        let keys = CryptoKeys {
            master: master.clone(),
            secret: secret.clone(),
            public,
        };

        // Write values to storage
        k2v.insert_batch(&[
            k2v_insert_single_key("keys", "salt", None, &ident_salt),
            k2v_insert_single_key("keys", "public", None, &keys.public),
        ])
        .await
        .context("InsertBatch for salt and public")?;

        Ok(keys)
    }

    pub async fn open(storage: &StorageCredentials, password: &str) -> Result<Self> {
        let k2v = storage.k2v_client()?;
        let (ident_salt, expected_public) = Self::load_salt_and_public(&k2v).await?;

        // Generate short password digest (= password identity)
        let ident = argon2_kdf(&ident_salt, password.as_bytes(), 16)?;

        // Lookup password blob
        let password_sortkey = format!("password:{}", hex::encode(&ident));

        let password_blob = {
            let mut val = match k2v.read_item("keys", &password_sortkey).await {
                Err(k2v_client::Error::NotFound) => {
                    bail!("given password does not exist in storage")
                }
                x => x?,
            };
            if val.value.len() != 1 {
                bail!("multiple values for password in storage");
            }
            match val.value.pop().unwrap() {
                K2vValue::Value(v) => v,
                K2vValue::Tombstone => bail!("password is a tombstone"),
            }
        };

        // Try to open blob
        let kdf_salt = &password_blob[..32];
        let password_key =
            Key::from_slice(&argon2_kdf(kdf_salt, password.as_bytes(), 32)?).unwrap();
        let password_openned = open(&password_blob[32..], &password_key)?;

        let keys = Self::deserialize(&password_openned)?;
        if keys.public != expected_public {
            bail!("Password public key doesn't match stored public key");
        }

        Ok(keys)
    }

    pub async fn open_without_password(
        storage: &StorageCredentials,
        master: &Key,
        secret: &SecretKey,
    ) -> Result<Self> {
        let k2v = storage.k2v_client()?;
        let (_ident_salt, expected_public) = Self::load_salt_and_public(&k2v).await?;

        // Create CryptoKeys struct from given keys
        let public = secret.public_key();
        let keys = CryptoKeys {
            master: master.clone(),
            secret: secret.clone(),
            public,
        };

        // Check public key matches
        if keys.public != expected_public {
            bail!("Given public key doesn't match stored public key");
        }

        Ok(keys)
    }

    pub async fn add_password(&self, storage: &StorageCredentials, password: &str) -> Result<()> {
        let k2v = storage.k2v_client()?;
        let (ident_salt, _public) = Self::load_salt_and_public(&k2v).await?;

        // Generate short password digest (= password identity)
        let ident = argon2_kdf(&ident_salt, password.as_bytes(), 16)?;

        // Generate salt for KDF
        let mut kdf_salt = [0u8; 32];
        thread_rng().fill(&mut kdf_salt);

        // Calculate key for password secret box
        let password_key =
            Key::from_slice(&argon2_kdf(&kdf_salt, password.as_bytes(), 32)?).unwrap();

        // Seal a secret box that contains our crypto keys
        let password_sealed = seal(&self.serialize(), &password_key)?;

        let password_sortkey = format!("password:{}", hex::encode(&ident));
        let password_blob = [&kdf_salt[..], &password_sealed].concat();

        // List existing passwords to overwrite existing entry if necessary
        let existing_passwords = Self::list_existing_passwords(&k2v).await?;
        let ct = match existing_passwords.get(&password_sortkey) {
            Some(p) => {
                if p.value.iter().any(|x| matches!(x, K2vValue::Value(_))) {
                    bail!("Password already exists");
                }
                Some(p.causality.clone())
            }
            None => None,
        };

        // Write values to storage
        k2v.insert_batch(&[k2v_insert_single_key(
            "keys",
            &password_sortkey,
            ct,
            &password_blob,
        )])
        .await
        .context("InsertBatch for new password")?;

        Ok(())
    }

    pub async fn delete_password(
        &self,
        storage: &StorageCredentials,
        password: &str,
        allow_delete_all: bool,
    ) -> Result<()> {
        let k2v = storage.k2v_client()?;
        let (ident_salt, _public) = Self::load_salt_and_public(&k2v).await?;

        // Generate short password digest (= password identity)
        let ident = argon2_kdf(&ident_salt, password.as_bytes(), 16)?;
        let password_sortkey = format!("password:{}", hex::encode(&ident));

        // List existing passwords
        let existing_passwords = Self::list_existing_passwords(&k2v).await?;

        // Check password is there
        let pw = existing_passwords
            .get(&password_sortkey)
            .ok_or(anyhow!("password does not exist"))?;

        if !allow_delete_all && existing_passwords.len() < 2 {
            bail!("No other password exists, not deleting last password.");
        }

        k2v.delete_item("keys", &password_sortkey, pw.causality.clone())
            .await
            .context("DeleteItem for password")?;

        Ok(())
    }

    // ---- STORAGE UTIL ----

    async fn check_uninitialized(k2v: &K2vClient) -> Result<()> {
        let params = k2v
            .read_batch(&[
                k2v_read_single_key("keys", "salt"),
                k2v_read_single_key("keys", "public"),
            ])
            .await
            .context("ReadBatch for salt and public in check_uninitialized")?;
        if params.len() != 2 {
            bail!(
                "Invalid response from k2v storage: {:?} (expected two items)",
                params
            );
        }
        if !params[0].items.is_empty() || !params[1].items.is_empty() {
            bail!("`salt` or `public` already exists in keys storage.");
        }

        Ok(())
    }

    async fn load_salt_and_public(k2v: &K2vClient) -> Result<([u8; 32], PublicKey)> {
        let mut params = k2v
            .read_batch(&[
                k2v_read_single_key("keys", "salt"),
                k2v_read_single_key("keys", "public"),
            ])
            .await
            .context("ReadBatch for salt and public in load_salt_and_public")?;
        if params.len() != 2 {
            bail!(
                "Invalid response from k2v storage: {:?} (expected two items)",
                params
            );
        }
        if params[0].items.len() != 1 || params[1].items.len() != 1 {
            bail!("`salt` or `public` do not exist in storage.");
        }

        // Retrieve salt from given response
        let salt_vals = &mut params[0].items.iter_mut().next().unwrap().1.value;
        if salt_vals.len() != 1 {
            bail!("Multiple values for `salt`");
        }
        let salt: Vec<u8> = match &mut salt_vals[0] {
            K2vValue::Value(v) => std::mem::take(v),
            K2vValue::Tombstone => bail!("salt is a tombstone"),
        };
        if salt.len() != 32 {
            bail!("`salt` is not 32 bytes long");
        }
        let mut salt_constlen = [0u8; 32];
        salt_constlen.copy_from_slice(&salt);

        // Retrieve public from given response
        let public_vals = &mut params[1].items.iter_mut().next().unwrap().1.value;
        if public_vals.len() != 1 {
            bail!("Multiple values for `public`");
        }
        let public: Vec<u8> = match &mut public_vals[0] {
            K2vValue::Value(v) => std::mem::take(v),
            K2vValue::Tombstone => bail!("public is a tombstone"),
        };
        let public = PublicKey::from_slice(&public).ok_or(anyhow!("Invalid public key length"))?;

        Ok((salt_constlen, public))
    }

    async fn list_existing_passwords(k2v: &K2vClient) -> Result<BTreeMap<String, CausalValue>> {
        let mut res = k2v
            .read_batch(&[BatchReadOp {
                partition_key: "keys",
                filter: Filter {
                    start: None,
                    end: None,
                    prefix: Some("password:"),
                    limit: None,
                    reverse: false,
                },
                conflicts_only: false,
                tombstones: false,
                single_item: false,
            }])
            .await
            .context("ReadBatch for prefix password: in list_existing_passwords")?;
        if res.len() != 1 {
            bail!("unexpected k2v result: {:?}, expected one item", res);
        }
        Ok(res.pop().unwrap().items)
    }

    fn serialize(&self) -> [u8; 64] {
        let mut res = [0u8; 64];
        res[..32].copy_from_slice(self.master.as_ref());
        res[32..].copy_from_slice(self.secret.as_ref());
        res
    }

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

    let salt = base64::encode_config(salt, base64::STANDARD_NO_PAD);
    let hash = argon2
        .hash_password(password, &salt)
        .map_err(|e| anyhow!("Unable to hash: {}", e))?;

    let hash = hash.hash.ok_or(anyhow!("Missing output"))?;
    assert!(hash.len() == output_len);
    Ok(hash.as_bytes().to_vec())
}

pub fn k2v_read_single_key<'a>(partition_key: &'a str, sort_key: &'a str) -> BatchReadOp<'a> {
    BatchReadOp {
        partition_key: partition_key,
        filter: Filter {
            start: Some(sort_key),
            end: None,
            prefix: None,
            limit: None,
            reverse: false,
        },
        conflicts_only: false,
        tombstones: false,
        single_item: true,
    }
}

pub fn k2v_insert_single_key<'a>(
    partition_key: &'a str,
    sort_key: &'a str,
    causality: Option<CausalityToken>,
    value: impl AsRef<[u8]>,
) -> BatchInsertOp<'a> {
    BatchInsertOp {
        partition_key,
        sort_key,
        causality,
        value: K2vValue::Value(value.as_ref().to_vec()),
    }
}
