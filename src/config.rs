use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CompanionConfig {
    pub pid: Option<String>,
    pub imap: ImapConfig,

    #[serde(flatten)]
    pub users: LoginStaticConfig,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProviderConfig {
    pub pid: Option<String>,
    pub imap: ImapConfig,
    pub lmtp: LmtpConfig,
    pub users: UserManagement,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "user_driver")]
pub enum UserManagement {
    Static(LoginStaticConfig),
    Ldap(LoginLdapConfig),
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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LoginStaticConfig {
    pub user_list: PathBuf,
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

    // The field that will contain the crypto root thingy
    pub crypto_root_attr: String,

    // Storage related thing
    #[serde(flatten)]
    pub storage: LdapStorage,
}

// ----

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

pub type UserList = HashMap<String, UserEntry>;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UserEntry {
    #[serde(default)]
    pub email_addresses: Vec<String>,
    pub password: String,
    pub crypto_root: String,

    #[serde(flatten)]
    pub storage: StaticStorage,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SetupEntry {
    #[serde(default)]
    pub email_addresses: Vec<String>,

    #[serde(default)]
    pub clear_password: Option<String>,

    #[serde(flatten)]
    pub storage: StaticStorage,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "role")]
pub enum AnyConfig {
    Companion(CompanionConfig),
    Provider(ProviderConfig),
}

// ---
pub fn read_config<T: serde::de::DeserializeOwned>(config_file: PathBuf) -> Result<T> {
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .open(config_file.as_path())?;

    let mut config = String::new();
    file.read_to_string(&mut config)?;

    Ok(toml::from_str(&config)?)
}

pub fn write_config<T: Serialize>(config_file: PathBuf, config: &T) -> Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .open(config_file.as_path())?;

    file.write_all(toml::to_string(config)?.as_bytes())?;

    Ok(())
}

fn default_mail_attr() -> String {
    "mail".into()
}
