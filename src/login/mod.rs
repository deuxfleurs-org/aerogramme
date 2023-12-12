pub mod ldap_provider;
pub mod static_provider;

use std::sync::Arc;
use futures::try_join;

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
    pub storage: Builders,
    /// The cryptographic keys are used to encrypt and decrypt data stored in S3 and K2V
    pub keys: CryptoKeys,
}

#[derive(Clone, Debug)]
pub struct PublicCredentials {
    /// The storage credentials are used to authenticate access to the underlying storage (S3, K2V)
    pub storage: Builders,
    pub public_key: PublicKey,
}

/*
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
*/

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


impl Credentials {
    pub fn row_client(&self) -> Result<RowStore> {
        Ok(self.storage.row_store()?)
    }
    pub fn blob_client(&self) -> Result<BlobStore> {
        Ok(self.storage.blob_store()?)
    }
}

impl CryptoKeys {
    pub async fn init(
        storage: &Builders,
        password: &str,
    ) -> Result<Self> {
        // Check that salt and public don't exist already
        let k2v = storage.row_store()?;
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
        let password_key = derive_password_key(&kdf_salt, password)?;

        // Seal a secret box that contains our crypto keys
        let password_sealed = seal(&keys.serialize(), &password_key)?;

        let password_sortkey = format!("password:{}", hex::encode(&ident));
        let password_blob = [&kdf_salt[..], &password_sealed].concat();

        // Write values to storage
        // @FIXME Implement insert batch in the storage API
        let (salt, public, passwd) = (
            salt_ct.set_value(&ident_salt),
            public_ct.set_value(keys.public.as_ref()),
            k2v.row("keys", &password_sortkey).set_value(&password_blob)
        );
        try_join!(salt.push(), public.push(), passwd.push())
            .context("InsertBatch for salt, public, and password")?;

        Ok(keys)
    }

    pub async fn init_without_password(
        storage: &Builders,
        master: &Key,
        secret: &SecretKey,
    ) -> Result<Self> {
        // Check that salt and public don't exist already
        let k2v = storage.row_store()?;
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
        // @FIXME implement insert batch in the storage API
        let (salt, public) = (
            salt_ct.set_value(&ident_salt),
            public_ct.set_value(keys.public.as_ref()),
        );

        try_join!(salt.push(), public.push()).context("InsertBatch for salt and public")?;

        Ok(keys)
    }

    pub async fn open(
        password: &str,
        root_blob: &str,
    ) -> Result<Self> {
        let kdf_salt = &password_blob[..32];
        let password_openned = try_open_encrypted_keys(kdf_salt, password, &password_blob[32..])?;

        let keys = Self::deserialize(&password_openned)?;
        if keys.public != expected_public {
            bail!("Password public key doesn't match stored public key");
        }

        Ok(keys)
       
        /*
        let k2v = storage.row_store()?;
        let (ident_salt, expected_public) = Self::load_salt_and_public(&k2v).await?;

        // Generate short password digest (= password identity)
        let ident = argon2_kdf(&ident_salt, password.as_bytes(), 16)?;

        // Lookup password blob
        let password_sortkey = format!("password:{}", hex::encode(&ident));
        let password_ref = k2v.row("keys", &password_sortkey);

        let password_blob = {
            let val = match password_ref.fetch().await {
                Err(StorageError::NotFound) => {
                    bail!("invalid password")
                }
                x => x?,
            };
            if val.content().len() != 1 {
                bail!("multiple values for password in storage");
            }
            match val.content().pop().unwrap() {
                Alternative::Value(v) => v,
                Alternative::Tombstone => bail!("invalid password"),
            }
        };

        // Try to open blob
        let kdf_salt = &password_blob[..32];
        let password_openned = try_open_encrypted_keys(kdf_salt, password, &password_blob[32..])?;

        let keys = Self::deserialize(&password_openned)?;
        if keys.public != expected_public {
            bail!("Password public key doesn't match stored public key");
        }

        Ok(keys)
        */
    }

    pub async fn open_without_password(
        storage: &Builders,
        master: &Key,
        secret: &SecretKey,
    ) -> Result<Self> {
        let k2v = storage.row_store()?;
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
        storage: &Builders,
        password: &str,
    ) -> Result<()> {
        let k2v = storage.row_store()?;
        let (ident_salt, _public) = Self::load_salt_and_public(&k2v).await?;

        // Generate short password digest (= password identity)
        let ident = argon2_kdf(&ident_salt, password.as_bytes(), 16)?;

        // Generate salt for KDF
        let mut kdf_salt = [0u8; 32];
        thread_rng().fill(&mut kdf_salt);

        // Calculate key for password secret box
        let password_key = derive_password_key(&kdf_salt, password)?;

        // Seal a secret box that contains our crypto keys
        let password_sealed = seal(&self.serialize(), &password_key)?;

        let password_sortkey = format!("password:{}", hex::encode(&ident));
        let password_blob = [&kdf_salt[..], &password_sealed].concat();

        // List existing passwords to overwrite existing entry if necessary
        let pass_key = k2v.row("keys", &password_sortkey);
        let passwd = match pass_key.fetch().await {
            Err(StorageError::NotFound) => pass_key,
            v => {
                let entry = v?;
                if entry.content().iter().any(|x| matches!(x, Alternative::Value(_))) {
                    bail!("password already exists");
                }
                entry.to_ref()
            }
        };

        // Write values to storage
        passwd
            .set_value(&password_blob)
            .push()
            .await
            .context("InsertBatch for new password")?;

        Ok(())
    }

    pub async fn delete_password(
        storage: &Builders,
        password: &str,
        allow_delete_all: bool,
    ) -> Result<()> {
        let k2v = storage.row_store()?;
        let (ident_salt, _public) = Self::load_salt_and_public(&k2v).await?;

        // Generate short password digest (= password identity)
        let ident = argon2_kdf(&ident_salt, password.as_bytes(), 16)?;
        let password_sortkey = format!("password:{}", hex::encode(&ident));

        // List existing passwords
        let existing_passwords = Self::list_existing_passwords(&k2v).await?;

        // Check password is there
        let pw = existing_passwords
            .iter()
            .map(|x| x.to_ref())
            .find(|x| x.key().1 == &password_sortkey)
            //.get(&password_sortkey)
            .ok_or(anyhow!("password does not exist"))?;

        if !allow_delete_all && existing_passwords.len() < 2 {
            bail!("No other password exists, not deleting last password.");
        }

        pw.rm().await.context("DeleteItem for password")?;

        Ok(())
    }

    // ---- STORAGE UTIL ----
    //
    async fn check_uninitialized(
        k2v: &RowStore,
    ) -> Result<(RowRef, RowRef)> {
        let params = k2v
            .select(Selector::List(vec![
                ("keys", "salt"),
                ("keys", "public"),
            ]))
            .await
            .context("ReadBatch for salt and public in check_uninitialized")?;

        if params.len() != 2 {
            bail!(
                "Invalid response from k2v storage: {:?} (expected two items)",
                params
            );
        }

        let salt_ct = params[0].to_ref();
        if params[0].content().iter().any(|x| matches!(x, Alternative::Value(_))) {
            bail!("key storage already initialized");
        }

        let public_ct = params[1].to_ref();
        if params[1].content().iter().any(|x| matches!(x, Alternative::Value(_))) {
            bail!("key storage already initialized");
        }

        Ok((salt_ct, public_ct))
    }

    pub async fn load_salt_and_public(k2v: &RowStore) -> Result<([u8; 32], PublicKey)> {
        let params = k2v
            .select(Selector::List(vec![
                ("keys", "salt"),
                ("keys", "public"),
            ]))
            .await
            .context("ReadBatch for salt and public in load_salt_and_public")?;

        if params.len() != 2 {
            bail!(
                "Invalid response from k2v storage: {:?} (expected two items)",
                params
            );
        }
        if params[0].content().len() != 1 || params[1].content().len() != 1 {
            bail!("cryptographic keys not initialized for user");
        }

        // Retrieve salt from given response
        let salt: Vec<u8> = match &mut params[0].content().iter_mut().next().unwrap() {
            Alternative::Value(v) => std::mem::take(v),
            Alternative::Tombstone => bail!("salt is a tombstone"),
        };
        if salt.len() != 32 {
            bail!("`salt` is not 32 bytes long");
        }
        let mut salt_constlen = [0u8; 32];
        salt_constlen.copy_from_slice(&salt);

        // Retrieve public from given response
        let public: Vec<u8> = match &mut params[1].content().iter_mut().next().unwrap() {
            Alternative::Value(v) => std::mem::take(v),
            Alternative::Tombstone => bail!("public is a tombstone"),
        };
        let public = PublicKey::from_slice(&public).ok_or(anyhow!("Invalid public key length"))?;

        Ok((salt_constlen, public))
    }

    async fn list_existing_passwords(k2v: &RowStore) -> Result<Vec<RowValue>> {
        let res = k2v.select(Selector::Prefix { shard_key: "keys", prefix: "password:" })
            .await
            .context("ReadBatch for prefix password: in list_existing_passwords")?;

        Ok(res)
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

    let salt = base64::encode_config(salt, base64::STANDARD_NO_PAD);
    let hash = argon2
        .hash_password(password, &salt)
        .map_err(|e| anyhow!("Unable to hash: {}", e))?;

    let hash = hash.hash.ok_or(anyhow!("Missing output"))?;
    assert!(hash.len() == output_len);
    Ok(hash.as_bytes().to_vec())
}
