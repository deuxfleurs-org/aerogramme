use std::collections::HashMap;
use std::io::Read;
use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    pub login_static: Option<LoginStaticConfig>,
    pub login_ldap: Option<LoginLdapConfig>,

    pub lmtp: Option<LmtpConfig>,
    pub imap: Option<ImapConfig>,
}

pub type LoginStaticConfig = HashMap<String, LoginStaticUser>;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "storage_driver")]
pub enum StaticStorage {
    Garage(StaticGarageConfig),
    InMemory,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StaticGarageConfig {
    pub s3_endpoint: String,
    pub k2v_endpoint: String,
    pub aws_region: String,

    pub aws_access_key_id: String,
    pub aws_secret_access_key: String,
    pub bucket: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LoginStaticUser {
    #[serde(default)]
    pub email_addresses: Vec<String>,
    pub password: String,

    pub user_secret: String,
    #[serde(default)]
    pub alternate_user_secrets: Vec<String>,

    pub master_key: Option<String>,
    pub secret_key: Option<String>,

    #[serde(flatten)]
    pub storage: StaticStorage,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "storage_driver")]
pub enum LdapStorage {
    Garage(LdapGarageConfig),
    InMemory,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LdapGarageConfig {
    pub s3_endpoint: String,
    pub k2v_endpoint: String,
    pub aws_region: String,

    pub aws_access_key_id_attr: String,
    pub aws_secret_access_key_attr: String,
    pub bucket_attr: Option<String>,
    pub default_bucket: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LoginLdapConfig {
    // LDAP connection info
    pub ldap_server: String,
    #[serde(default)]
    pub pre_bind_on_login: bool,
    pub bind_dn: Option<String>,
    pub bind_password: Option<String>,
    pub search_base: String,

    // Schema-like info required for Aerogramme's logic
    pub username_attr: String,
    #[serde(default = "default_mail_attr")]
    pub mail_attr: String,
    pub user_secret_attr: String,
    pub alternate_user_secrets_attr: Option<String>,

    // Storage related thing
    #[serde(flatten)]
    pub storage: LdapStorage,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LmtpConfig {
    pub bind_addr: SocketAddr,
    pub hostname: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ImapConfig {
    pub bind_addr: SocketAddr,
}

pub fn read_config(config_file: PathBuf) -> Result<Config> {
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .open(config_file.as_path())?;

    let mut config = String::new();
    file.read_to_string(&mut config)?;

    Ok(toml::from_str(&config)?)
}

fn default_mail_attr() -> String {
    "mail".into()
}
