use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct CompanionConfig {
    pub pid: Option<PathBuf>,
    pub imap: ImapUnsecureConfig,
    // @FIXME Add DAV
    #[serde(flatten)]
    pub users: LoginStaticConfig,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ProviderConfig {
    pub pid: Option<PathBuf>,
    pub imap: Option<ImapConfig>,
    pub imap_unsecure: Option<ImapUnsecureConfig>,
    pub lmtp: Option<LmtpConfig>,
    pub auth: Option<AuthConfig>,
    pub dav: Option<DavConfig>,
    pub dav_unsecure: Option<DavUnsecureConfig>,
    pub metrics: Option<PrometheusEndpointConfig>,
    pub users: UserManagement,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "user_driver")]
pub enum UserManagement {
    Demo,
    Static(LoginStaticConfig),
    Ldap(LoginLdapConfig),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct AuthConfig {
    pub bind_addr: SocketAddr,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct LmtpConfig {
    pub bind_addr: SocketAddr,
    pub hostname: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ImapConfig {
    pub bind_addr: SocketAddr,
    pub certs: PathBuf,
    pub key: PathBuf,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct DavUnsecureConfig {
    pub bind_addr: SocketAddr,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct DavConfig {
    pub bind_addr: SocketAddr,
    pub certs: PathBuf,
    pub key: PathBuf,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ImapUnsecureConfig {
    pub bind_addr: SocketAddr,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct LoginStaticConfig {
    pub user_list: PathBuf,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct PrometheusEndpointConfig {
    pub bind_addr: SocketAddr,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "storage_driver")]
pub enum LdapStorage {
    Garage(LdapGarageConfig),
    InMemory,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct LdapGarageConfig {
    pub s3_endpoint: String,
    pub k2v_endpoint: String,
    pub aws_region: String,

    pub aws_access_key_id_attr: String,
    pub aws_secret_access_key_attr: String,
    pub bucket_attr: Option<String>,
    pub default_bucket: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
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
        .create(true)
        .truncate(true)
        .open(config_file.as_path())?;

    file.write_all(toml::to_string(config)?.as_bytes())?;

    Ok(())
}

fn default_mail_attr() -> String {
    "mail".into()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::config::{
        AnyConfig, AuthConfig, CompanionConfig, ImapConfig, ImapUnsecureConfig, LmtpConfig,
        LoginStaticConfig, ProviderConfig, UserManagement,
    };

    #[test]
    fn deserialize_provider_config() {
        const PROVIDER_CONFIG: &str = r#"role = "Provider"
        pid = "/var/run/aerogramme.pid"

        [auth]
        bind_addr = "[::1]:12345"

        [imap_unsecure]
        bind_addr="[::1]:143"

        [imap]
        bind_addr="[::]:993"
        certs = "my-certs.pem"
        key = "my-key.pem"

        [lmtp]
        bind_addr="[::1]:1025"
        hostname="example.tld"

        [users]
        user_driver = "Demo"
        "#;

        let config = toml::from_str::<AnyConfig>(PROVIDER_CONFIG)
            .expect("failed to deserialize `ProviderConfig` into `AnyConfig`");

        assert_eq!(
            config,
            AnyConfig::Provider(ProviderConfig {
                pid: Some(PathBuf::from("/var/run/aerogramme.pid")),
                imap: Some(ImapConfig {
                    bind_addr: "[::]:993".parse().expect("failed to parse SocketAddr"),
                    certs: PathBuf::from("my-certs.pem"),
                    key: PathBuf::from("my-key.pem"),
                }),
                imap_unsecure: Some(ImapUnsecureConfig {
                    bind_addr: "[::1]:143".parse().expect("failed to parse bind addr")
                }),
                lmtp: Some(LmtpConfig {
                    bind_addr: "[::1]:1025".parse().expect("failed to parse SocketAddr"),
                    hostname: "example.tld".into()
                }),
                auth: Some(AuthConfig {
                    bind_addr: "[::1]:12345".parse().expect("failed to parse SocketAddr")
                }),
                dav: None,
                dav_unsecure: None,
                metrics: None,
                users: UserManagement::Demo,
            })
        );
    }

    #[test]
    fn deserialize_companion_config() {
        const COMPANION_CONFIG: &str = r#"
        role = "Companion"
        pid = "/var/run/user/1000/aerogramme.pid"
        user_list = "/home/user/.config/aerogramme-users.toml"

        [imap]
        bind_addr = "[::1]:1143"
        "#;

        let config = toml::from_str::<AnyConfig>(COMPANION_CONFIG)
            .expect("failed to deserialize `CompanionConfig` into `AnyConfig`");
        assert_eq!(
            config,
            AnyConfig::Companion(CompanionConfig {
                pid: Some(PathBuf::from("/var/run/user/1000/aerogramme.pid")),
                imap: ImapUnsecureConfig {
                    bind_addr: "[::1]:1143".parse().expect("failed to parse SocketAddr")
                },
                users: LoginStaticConfig {
                    user_list: PathBuf::from("/home/user/.config/aerogramme-users.toml")
                },
            })
        );
    }
}
