pub mod ldap_provider;
pub mod static_provider;

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use k2v_client::{
    BatchInsertOp, BatchReadOp, CausalValue, CausalityToken, Filter, K2vClient, K2vValue,
};
use rand::prelude::*;
use rusoto_core::HttpClient;
use rusoto_credential::{AwsCredentials, StaticProvider};
use rusoto_s3::S3Client;

use crate::cryptoblob::*;

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
    pub storage: StorageCredentials,
    /// The cryptographic keys are used to encrypt and decrypt data stored in S3 and K2V
    pub keys: CryptoKeys,
}

#[derive(Clone, Debug)]
pub struct PublicCredentials {
    /// The storage credentials are used to authenticate access to the underlying storage (S3, K2V)
    pub storage: StorageCredentials,
    pub public_key: PublicKey,
}

/// The struct StorageCredentials contains access key to an S3 and K2V bucket
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct StorageCredentials {
    pub s3_region: Region,
    pub k2v_region: Region,

    pub aws_access_key_id: String,
    pub aws_secret_access_key: String,
    pub bucket: String,
}

/// The struct UserSecrets represents intermediary secrets that are mixed in with the user's
/// password when decrypting the cryptographic keys that are stored in their bucket.
/// These secrets should be stored somewhere else (e.g. in the LDAP server or in the
/// local config file), as an additionnal authentification factor so that the password
/// isn't enough just alone to decrypt the content of a user's bucket.
pub struct UserSecrets {
    /// The main user secret that will be used to encrypt keys when a new password is added
    pub user_secret: String,
    /// Alternative user secrets that will be tried when decrypting keys that were encrypted
    /// with old passwords
    pub alternate_user_secrets: Vec<String>,
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

/// A custom S3 region, composed of a region name and endpoint.
/// We use this instead of rusoto_signature::Region so that we can
/// derive Hash and Eq
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct Region {
    pub name: String,
    pub endpoint: String,
}

impl Region {
    pub fn as_rusoto_region(&self) -> rusoto_signature::Region {
        rusoto_signature::Region::Custom {
            name: self.name.clone(),
            endpoint: self.endpoint.clone(),
        }
    }
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
            self.k2v_region.as_rusoto_region(),
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
            self.s3_region.as_rusoto_region(),
        ))
    }
}

impl CryptoKeys {
    pub async fn init(
        storage: &StorageCredentials,
        user_secrets: &UserSecrets,
        password: &str,
    ) -> Result<Self> {
        // Check that salt and public don't exist already
        let k2v = storage.k2v_client()?;
        let (salt_ct, public_ct) = Self::check_uninitialized(&k2v).await?;

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
        let password_key = user_secrets.derive_password_key(&kdf_salt, password)?;

        // Seal a secret box that contains our crypto keys
        let password_sealed = seal(&keys.serialize(), &password_key)?;

        let password_sortkey = format!("password:{}", hex::encode(&ident));
        let password_blob = [&kdf_salt[..], &password_sealed].concat();

        // Write values to storage
        k2v.insert_batch(&[
            k2v_insert_single_key("keys", "salt", salt_ct, &ident_salt),
            k2v_insert_single_key("keys", "public", public_ct, &keys.public),
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
        let (salt_ct, public_ct) = Self::check_uninitialized(&k2v).await?;

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
            k2v_insert_single_key("keys", "salt", salt_ct, &ident_salt),
            k2v_insert_single_key("keys", "public", public_ct, &keys.public),
        ])
        .await
        .context("InsertBatch for salt and public")?;

        Ok(keys)
    }

    pub async fn open(
        storage: &StorageCredentials,
        user_secrets: &UserSecrets,
        password: &str,
    ) -> Result<Self> {
        let k2v = storage.k2v_client()?;
        let (ident_salt, expected_public) = Self::load_salt_and_public(&k2v).await?;

        // Generate short password digest (= password identity)
        let ident = argon2_kdf(&ident_salt, password.as_bytes(), 16)?;

        // Lookup password blob
        let password_sortkey = format!("password:{}", hex::encode(&ident));

        let password_blob = {
            let mut val = match k2v.read_item("keys", &password_sortkey).await {
                Err(k2v_client::Error::NotFound) => {
                    bail!("invalid password")
                }
                x => x?,
            };
            if val.value.len() != 1 {
                bail!("multiple values for password in storage");
            }
            match val.value.pop().unwrap() {
                K2vValue::Value(v) => v,
                K2vValue::Tombstone => bail!("invalid password"),
            }
        };

        // Try to open blob
        let kdf_salt = &password_blob[..32];
        let password_openned =
            user_secrets.try_open_encrypted_keys(&kdf_salt, password, &password_blob[32..])?;

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

    pub async fn add_password(
        &self,
        storage: &StorageCredentials,
        user_secrets: &UserSecrets,
        password: &str,
    ) -> Result<()> {
        let k2v = storage.k2v_client()?;
        let (ident_salt, _public) = Self::load_salt_and_public(&k2v).await?;

        // Generate short password digest (= password identity)
        let ident = argon2_kdf(&ident_salt, password.as_bytes(), 16)?;

        // Generate salt for KDF
        let mut kdf_salt = [0u8; 32];
        thread_rng().fill(&mut kdf_salt);

        // Calculate key for password secret box
        let password_key = user_secrets.derive_password_key(&kdf_salt, password)?;

        // Seal a secret box that contains our crypto keys
        let password_sealed = seal(&self.serialize(), &password_key)?;

        let password_sortkey = format!("password:{}", hex::encode(&ident));
        let password_blob = [&kdf_salt[..], &password_sealed].concat();

        // List existing passwords to overwrite existing entry if necessary
        let ct = match k2v.read_item("keys", &password_sortkey).await {
            Err(k2v_client::Error::NotFound) => None,
            v => {
                let entry = v?;
                if entry.value.iter().any(|x| matches!(x, K2vValue::Value(_))) {
                    bail!("password already exists");
                }
                Some(entry.causality.clone())
            }
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

    async fn check_uninitialized(
        k2v: &K2vClient,
    ) -> Result<(Option<CausalityToken>, Option<CausalityToken>)> {
        let params = k2v
            .read_batch(&[
                k2v_read_single_key("keys", "salt", true),
                k2v_read_single_key("keys", "public", true),
            ])
            .await
            .context("ReadBatch for salt and public in check_uninitialized")?;
        if params.len() != 2 {
            bail!(
                "Invalid response from k2v storage: {:?} (expected two items)",
                params
            );
        }
        if params[0].items.len() > 1 || params[1].items.len() > 1 {
            bail!(
                "invalid response from k2v storage: {:?} (several items in single_item read)",
                params
            );
        }

        let salt_ct = match params[0].items.iter().next() {
            None => None,
            Some((_, CausalValue { causality, value })) => {
                if value.iter().any(|x| matches!(x, K2vValue::Value(_))) {
                    bail!("key storage already initialized");
                }
                Some(causality.clone())
            }
        };

        let public_ct = match params[1].items.iter().next() {
            None => None,
            Some((_, CausalValue { causality, value })) => {
                if value.iter().any(|x| matches!(x, K2vValue::Value(_))) {
                    bail!("key storage already initialized");
                }
                Some(causality.clone())
            }
        };

        Ok((salt_ct, public_ct))
    }

    pub async fn load_salt_and_public(k2v: &K2vClient) -> Result<([u8; 32], PublicKey)> {
        let mut params = k2v
            .read_batch(&[
                k2v_read_single_key("keys", "salt", false),
                k2v_read_single_key("keys", "public", false),
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
            bail!("cryptographic keys not initialized for user");
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

impl UserSecrets {
    fn derive_password_key_with(user_secret: &str, kdf_salt: &[u8], password: &str) -> Result<Key> {
        let tmp = format!("{}\n\n{}", user_secret, password);
        Ok(Key::from_slice(&argon2_kdf(&kdf_salt, tmp.as_bytes(), 32)?).unwrap())
    }

    fn derive_password_key(&self, kdf_salt: &[u8], password: &str) -> Result<Key> {
        Self::derive_password_key_with(&self.user_secret, kdf_salt, password)
    }

    fn try_open_encrypted_keys(
        &self,
        kdf_salt: &[u8],
        password: &str,
        encrypted_keys: &[u8],
    ) -> Result<Vec<u8>> {
        let secrets_to_try =
            std::iter::once(&self.user_secret).chain(self.alternate_user_secrets.iter());
        for user_secret in secrets_to_try {
            let password_key = Self::derive_password_key_with(user_secret, kdf_salt, password)?;
            if let Ok(res) = open(encrypted_keys, &password_key) {
                return Ok(res);
            }
        }
        bail!("Unable to decrypt password blob.");
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

pub fn k2v_read_single_key<'a>(
    partition_key: &'a str,
    sort_key: &'a str,
    tombstones: bool,
) -> BatchReadOp<'a> {
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
        tombstones,
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
