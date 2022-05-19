use std::collections::HashMap;

use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use rusoto_signature::Region;

use crate::config::*;
use crate::cryptoblob::Key;
use crate::login::*;

pub struct StaticLoginProvider {
    default_bucket: Option<String>,
    users: HashMap<String, LoginStaticUser>,
    k2v_region: Region,
}

impl StaticLoginProvider {
    pub fn new(config: LoginStaticConfig, k2v_region: Region) -> Result<Self> {
        Ok(Self {
            default_bucket: config.default_bucket,
            users: config.users,
            k2v_region,
        })
    }
}

#[async_trait]
impl LoginProvider for StaticLoginProvider {
    async fn login(&self, username: &str, password: &str) -> Result<Credentials> {
        match self.users.get(username) {
            None => bail!("User {} does not exist", username),
            Some(u) => {
                if u.password != password {
                    // TODO cryptographic password compare
                    bail!("Wrong password");
                }
                let bucket = u
                    .bucket
                    .clone()
                    .or_else(|| self.default_bucket.clone())
                    .ok_or(anyhow!(
                        "No bucket configured and no default bucket specieid"
                    ))?;

                // TODO if master key is not specified, retrieve it from K2V key storage
                let master_key_str = u.master_key.as_ref().ok_or(anyhow!(
                    "Master key must be specified in config file for now, this will change"
                ))?;
                let master_key = Key::from_slice(&base64::decode(master_key_str)?)
                    .ok_or(anyhow!("Invalid master key"))?;

                Ok(Credentials {
                    aws_access_key_id: u.aws_access_key_id.clone(),
                    aws_secret_access_key: u.aws_secret_access_key.clone(),
                    bucket,
                    master_key,
                })
            }
        }
    }
}
