use std::collections::HashMap;

use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use rusoto_signature::Region;

use crate::config::*;
use crate::cryptoblob::{Key, SecretKey};
use crate::login::*;

pub struct StaticLoginProvider {
    default_bucket: Option<String>,
    users: HashMap<String, LoginStaticUser>,
    k2v_region: Region,
    s3_region: Region,
}

impl StaticLoginProvider {
    pub fn new(config: LoginStaticConfig, k2v_region: Region, s3_region: Region) -> Result<Self> {
        Ok(Self {
            default_bucket: config.default_bucket,
            users: config.users,
            k2v_region,
            s3_region,
        })
    }
}

#[async_trait]
impl LoginProvider for StaticLoginProvider {
    async fn login(&self, username: &str, password: &str) -> Result<Credentials> {
        match self.users.get(username) {
            None => bail!("User {} does not exist", username),
            Some(u) => {
                if !verify_password(password, &u.password)? {
                    bail!("Wrong password");
                }
                let bucket = u
                    .bucket
                    .clone()
                    .or_else(|| self.default_bucket.clone())
                    .ok_or(anyhow!(
                        "No bucket configured and no default bucket specieid"
                    ))?;

                let storage = StorageCredentials {
                    k2v_region: self.k2v_region.clone(),
                    s3_region: self.s3_region.clone(),
                    aws_access_key_id: u.aws_access_key_id.clone(),
                    aws_secret_access_key: u.aws_secret_access_key.clone(),
                    bucket,
                };

                let keys = match (&u.master_key, &u.secret_key) {
                    (Some(m), Some(s)) => {
                        let master_key = Key::from_slice(&base64::decode(m)?)
                            .ok_or(anyhow!("Invalid master key"))?;
                        let secret_key = SecretKey::from_slice(&base64::decode(s)?)
                            .ok_or(anyhow!("Invalid secret key"))?;
                        CryptoKeys::open_without_password(&storage, &master_key, &secret_key).await?
                    }
                    (None, None) => {
                        CryptoKeys::open(&storage, password).await?
                    }
                    _ => bail!("Either both master and secret key or none of them must be specified for user"),
                };

                Ok(Credentials { storage, keys })
            }
        }
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
        password_hash::{rand_core::OsRng, PasswordHash, PasswordVerifier},
        Argon2,
    };
    let parsed_hash =
        PasswordHash::new(&hash).map_err(|e| anyhow!("Invalid hashed password: {}", e))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}
