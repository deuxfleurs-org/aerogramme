use std::collections::HashMap;
use std::sync::Arc;
use std::path::PathBuf;

use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;

use crate::config::*;
use crate::cryptoblob::{Key, SecretKey};
use crate::login::*;
use crate::storage;

pub struct StaticLoginProvider {
    user_list: PathBuf,
    users: HashMap<String, Arc<UserEntry>>,
    users_by_email: HashMap<String, Arc<UserEntry>>,
}

impl StaticLoginProvider {
    pub fn new(config: LoginStaticConfig) -> Result<Self> {
        let mut lp = Self {
            user_list: config.user_list,
            users: HashMap::new(),
            users_by_email: HashMap::new(),
        };

        lp.update_user_list();

        Ok(lp)
    }

    pub fn update_user_list(&mut self) -> Result<()> {
        let ulist: UserList = read_config(self.user_list.clone())?;

        let users = ulist
            .into_iter()
            .map(|(k, v)| (k, Arc::new(v)))
            .collect::<HashMap<_, _>>();
        let mut users_by_email = HashMap::new();
        for (_, u) in users.iter() {
            for m in u.email_addresses.iter() {
                if users_by_email.contains_key(m) {
                    bail!("Several users have same email address: {}", m);
                }
                users_by_email.insert(m.clone(), u.clone());
            }
        }
        Ok(())
    }
}

#[async_trait]
impl LoginProvider for StaticLoginProvider {
    async fn login(&self, username: &str, password: &str) -> Result<Credentials> {
        tracing::debug!(user=%username, "login");
        let user = match self.users.get(username) {
            None => bail!("User {} does not exist", username),
            Some(u) => u,
        };

        tracing::debug!(user=%username, "verify password");
        if !verify_password(password, &user.password)? {
            bail!("Wrong password");
        }

        tracing::debug!(user=%username, "fetch keys");
        let storage: storage::Builders = match &user.storage {
            StaticStorage::InMemory => Box::new(storage::in_memory::FullMem {}),
            StaticStorage::Garage(grgconf) => Box::new(storage::garage::GrgCreds {
                region: grgconf.aws_region.clone(),
                k2v_endpoint: grgconf.k2v_endpoint.clone(),
                s3_endpoint: grgconf.s3_endpoint.clone(),
                aws_access_key_id: grgconf.aws_access_key_id.clone(),
                aws_secret_access_key: grgconf.aws_secret_access_key.clone(),
                bucket: grgconf.bucket.clone(),
            }),
        };

        let keys = match &user.crypto_root { /*(&user.master_key, &user.secret_key) {*/
            CryptographyRoot::InPlace { master_key: m, secret_key: s } => {
                let master_key =
                    Key::from_slice(&base64::decode(m)?).ok_or(anyhow!("Invalid master key"))?;
                let secret_key = SecretKey::from_slice(&base64::decode(s)?)
                    .ok_or(anyhow!("Invalid secret key"))?;
                CryptoKeys::open_without_password(&storage, &master_key, &secret_key).await?
            }
            CryptographyRoot::PasswordProtected => {
                CryptoKeys::open(&storage, password).await?
            }
            CryptographyRoot::Keyring => unimplemented!(),
        };

        tracing::debug!(user=%username, "logged");
        Ok(Credentials { storage, keys })
    }

    async fn public_login(&self, email: &str) -> Result<PublicCredentials> {
        let user = match self.users_by_email.get(email) {
            None => bail!("No user for email address {}", email),
            Some(u) => u,
        };

        let storage: storage::Builders = match &user.storage {
            StaticStorage::InMemory => Box::new(storage::in_memory::FullMem {}),
            StaticStorage::Garage(grgconf) => Box::new(storage::garage::GrgCreds {
                region: grgconf.aws_region.clone(),
                k2v_endpoint: grgconf.k2v_endpoint.clone(),
                s3_endpoint: grgconf.s3_endpoint.clone(),
                aws_access_key_id: grgconf.aws_access_key_id.clone(),
                aws_secret_access_key: grgconf.aws_secret_access_key.clone(),
                bucket: grgconf.bucket.clone(),
            }),
        };

        let k2v_client = storage.row_store()?;
        let (_, public_key) = CryptoKeys::load_salt_and_public(&k2v_client).await?;

        Ok(PublicCredentials {
            storage,
            public_key,
        })
    }
}

pub fn hash_password(password: &str) -> Result<String> {
    use argon2::{
        password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
        Argon2,
    };
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    Ok(argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow!("Argon2 error: {}", e))?
        .to_string())
}

pub fn verify_password(password: &str, hash: &str) -> Result<bool> {
    use argon2::{
        password_hash::{PasswordHash, PasswordVerifier},
        Argon2,
    };
    let parsed_hash =
        PasswordHash::new(hash).map_err(|e| anyhow!("Invalid hashed password: {}", e))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}
