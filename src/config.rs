use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;

use anyhow::Result;
use serde::Deserialize;

#[derive(Deserialize, Debug, Clone)]
pub struct Config {
    pub s3_endpoint: String,
    pub k2v_endpoint: String,
    pub aws_region: String,

    pub login_static: Option<LoginStaticConfig>,
    pub login_ldap: Option<LoginLdapConfig>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct LoginStaticConfig {
    pub default_bucket: Option<String>,
    pub users: HashMap<String, LoginStaticUser>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct LoginStaticUser {
    pub password: String,

    pub aws_access_key_id: String,
    pub aws_secret_access_key: String,
    pub bucket: Option<String>,

    pub master_key: Option<String>,
    pub secret_key: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct LoginLdapConfig {
    pub ldap_server: String,

    pub search_dn: String,
    pub username_attr: String,
    pub aws_access_key_id_attr: String,
    pub aws_secret_access_key_attr: String,

    pub bucket: Option<String>,
    pub bucket_attr: Option<String>,
}

pub fn read_config(config_file: PathBuf) -> Result<Config> {
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .open(config_file.as_path())?;

    let mut config = String::new();
    file.read_to_string(&mut config)?;

    Ok(toml::from_str(&config)?)
}
