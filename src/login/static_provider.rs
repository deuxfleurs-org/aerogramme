use std::collections::HashMap;
use std::sync::Arc;
use std::path::PathBuf;

use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;

use crate::config::*;
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
            user_list: config.user_list.clone(),
            users: HashMap::new(),
            users_by_email: HashMap::new(),
        };

        lp
            .update_user_list()
            .context(
                format!(
                    "failed to read {:?}, make sure it exists and it's correctly formatted", 
                    config.user_list))?;

        Ok(lp)
    }

    pub fn update_user_list(&mut self) -> Result<()> {
        let ulist: UserList = read_config(self.user_list.clone())?;

        self.users = ulist
            .into_iter()
            .map(|(k, v)| (k, Arc::new(v)))
            .collect::<HashMap<_, _>>();

        self.users_by_email.clear();
        for (_, u) in self.users.iter() {
            for m in u.email_addresses.iter() {
                if self.users_by_email.contains_key(m) {
                    bail!("Several users have same email address: {}", m);
                }
                self.users_by_email.insert(m.clone(), u.clone());
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

        let cr = CryptoRoot(user.crypto_root);
        let keys = cr.crypto_keys(password)?;

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

        let cr = CryptoRoot(user.crypto_root);
        let public_key = cr.public_key()?;

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
