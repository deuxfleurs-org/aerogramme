use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;

use crate::config::*;
use crate::cryptoblob::{Key, SecretKey};
use crate::login::*;

pub struct StaticLoginProvider {
    default_bucket: Option<String>,
    users: HashMap<String, Arc<LoginStaticUser>>,
    users_by_email: HashMap<String, Arc<LoginStaticUser>>,

    k2v_region: Region,
    s3_region: Region,
}

impl StaticLoginProvider {
    pub fn new(config: LoginStaticConfig, k2v_region: Region, s3_region: Region) -> Result<Self> {
        let users = config
            .users
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

        Ok(Self {
            default_bucket: config.default_bucket,
            users,
            users_by_email,
            k2v_region,
            s3_region,
        })
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

        tracing::debug!(user=%username, "fetch bucket");
        let bucket = user
            .bucket
            .clone()
            .or_else(|| self.default_bucket.clone())
            .ok_or(anyhow!(
                "No bucket configured and no default bucket specieid"
            ))?;

        tracing::debug!(user=%username, "fetch keys");
        let storage = StorageCredentials {
            k2v_region: self.k2v_region.clone(),
            s3_region: self.s3_region.clone(),
            aws_access_key_id: user.aws_access_key_id.clone(),
            aws_secret_access_key: user.aws_secret_access_key.clone(),
            bucket,
        };

        let keys = match (&user.master_key, &user.secret_key) {
            (Some(m), Some(s)) => {
                let master_key =
                    Key::from_slice(&base64::decode(m)?).ok_or(anyhow!("Invalid master key"))?;
                let secret_key = SecretKey::from_slice(&base64::decode(s)?)
                    .ok_or(anyhow!("Invalid secret key"))?;
                CryptoKeys::open_without_password(&storage, &master_key, &secret_key).await?
            }
            (None, None) => {
                let user_secrets = UserSecrets {
                    user_secret: user.user_secret.clone(),
                    alternate_user_secrets: user.alternate_user_secrets.clone(),
                };
                CryptoKeys::open(&storage, &user_secrets, password).await?
            }
            _ => bail!(
                "Either both master and secret key or none of them must be specified for user"
            ),
        };

        tracing::debug!(user=%username, "logged");
        Ok(Credentials { storage, keys })
    }

    async fn public_login(&self, email: &str) -> Result<PublicCredentials> {
        let user = match self.users_by_email.get(email) {
            None => bail!("No user for email address {}", email),
            Some(u) => u,
        };

        let bucket = user
            .bucket
            .clone()
            .or_else(|| self.default_bucket.clone())
            .ok_or(anyhow!(
                "No bucket configured and no default bucket specieid"
            ))?;

        let storage = StorageCredentials {
            k2v_region: self.k2v_region.clone(),
            s3_region: self.s3_region.clone(),
            aws_access_key_id: user.aws_access_key_id.clone(),
            aws_secret_access_key: user.aws_secret_access_key.clone(),
            bucket,
        };

        let k2v_client = storage.k2v_client()?;
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
