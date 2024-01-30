use anyhow::Result;
use async_trait::async_trait;
use ldap3::{LdapConnAsync, Scope, SearchEntry};
use log::debug;

use crate::config::*;
use crate::login::*;
use crate::storage;

pub struct LdapLoginProvider {
    ldap_server: String,

    pre_bind_on_login: bool,
    bind_dn_and_pw: Option<(String, String)>,

    search_base: String,
    attrs_to_retrieve: Vec<String>,
    username_attr: String,
    mail_attr: String,
    crypto_root_attr: String,

    storage_specific: StorageSpecific,
    in_memory_store: storage::in_memory::MemDb,
}

enum BucketSource {
    Constant(String),
    Attr(String),
}

enum StorageSpecific {
    InMemory,
    Garage {
        from_config: LdapGarageConfig,
        bucket_source: BucketSource,
    },
}

impl LdapLoginProvider {
    pub fn new(config: LoginLdapConfig) -> Result<Self> {
        let bind_dn_and_pw = match (config.bind_dn, config.bind_password) {
            (Some(dn), Some(pw)) => Some((dn, pw)),
            (None, None) => None,
            _ => bail!(
                "If either of `bind_dn` or `bind_password` is set, the other must be set as well."
            ),
        };

        if config.pre_bind_on_login && bind_dn_and_pw.is_none() {
            bail!("Cannot use `pre_bind_on_login` without setting `bind_dn` and `bind_password`");
        }

        let mut attrs_to_retrieve = vec![
            config.username_attr.clone(),
            config.mail_attr.clone(),
            config.crypto_root_attr.clone(),
        ];

        // storage specific
        let specific = match config.storage {
            LdapStorage::InMemory => StorageSpecific::InMemory,
            LdapStorage::Garage(grgconf) => {
                attrs_to_retrieve.push(grgconf.aws_access_key_id_attr.clone());
                attrs_to_retrieve.push(grgconf.aws_secret_access_key_attr.clone());

                let bucket_source =
                    match (grgconf.default_bucket.clone(), grgconf.bucket_attr.clone()) {
                        (Some(b), None) => BucketSource::Constant(b),
                        (None, Some(a)) => BucketSource::Attr(a),
                        _ => bail!("Must set `bucket` or `bucket_attr`, but not both"),
                    };

                if let BucketSource::Attr(a) = &bucket_source {
                    attrs_to_retrieve.push(a.clone());
                }

                StorageSpecific::Garage {
                    from_config: grgconf,
                    bucket_source,
                }
            }
        };

        Ok(Self {
            ldap_server: config.ldap_server,
            pre_bind_on_login: config.pre_bind_on_login,
            bind_dn_and_pw,
            search_base: config.search_base,
            attrs_to_retrieve,
            username_attr: config.username_attr,
            mail_attr: config.mail_attr,
            crypto_root_attr: config.crypto_root_attr,
            storage_specific: specific,
            in_memory_store: storage::in_memory::MemDb::new(),
        })
    }

    async fn storage_creds_from_ldap_user(&self, user: &SearchEntry) -> Result<Builder> {
        let storage: Builder = match &self.storage_specific {
            StorageSpecific::InMemory => {
                self.in_memory_store
                    .builder(&get_attr(user, &self.username_attr)?)
                    .await
            }
            StorageSpecific::Garage {
                from_config,
                bucket_source,
            } => {
                let aws_access_key_id = get_attr(user, &from_config.aws_access_key_id_attr)?;
                let aws_secret_access_key =
                    get_attr(user, &from_config.aws_secret_access_key_attr)?;
                let bucket = match bucket_source {
                    BucketSource::Constant(b) => b.clone(),
                    BucketSource::Attr(a) => get_attr(user, &a)?,
                };

                storage::garage::GarageBuilder::new(storage::garage::GarageConf {
                    region: from_config.aws_region.clone(),
                    s3_endpoint: from_config.s3_endpoint.clone(),
                    k2v_endpoint: from_config.k2v_endpoint.clone(),
                    aws_access_key_id,
                    aws_secret_access_key,
                    bucket,
                })?
            }
        };

        Ok(storage)
    }
}

#[async_trait]
impl LoginProvider for LdapLoginProvider {
    async fn login(&self, username: &str, password: &str) -> Result<Credentials> {
        check_identifier(username)?;

        let (conn, mut ldap) = LdapConnAsync::new(&self.ldap_server).await?;
        ldap3::drive!(conn);

        if self.pre_bind_on_login {
            let (dn, pw) = self.bind_dn_and_pw.as_ref().unwrap();
            ldap.simple_bind(dn, pw).await?.success()?;
        }

        let (matches, _res) = ldap
            .search(
                &self.search_base,
                Scope::Subtree,
                &format!(
                    "(&(objectClass=inetOrgPerson)({}={}))",
                    self.username_attr, username
                ),
                &self.attrs_to_retrieve,
            )
            .await?
            .success()?;

        if matches.is_empty() {
            bail!("Invalid username");
        }
        if matches.len() > 1 {
            bail!("Invalid username (multiple matching accounts)");
        }
        let user = SearchEntry::construct(matches.into_iter().next().unwrap());
        debug!(
            "Found matching LDAP user for username {}: {}",
            username, user.dn
        );

        // Try to login against LDAP server with provided password
        // to check user's password
        ldap.simple_bind(&user.dn, password)
            .await?
            .success()
            .context("Invalid password")?;
        debug!("Ldap login with user name {} successfull", username);

        // cryptography
        let crstr = get_attr(&user, &self.crypto_root_attr)?;
        let cr = CryptoRoot(crstr);
        let keys = cr.crypto_keys(password)?;

        // storage
        let storage = self.storage_creds_from_ldap_user(&user).await?;

        drop(ldap);

        Ok(Credentials { storage, keys })
    }

    async fn public_login(&self, email: &str) -> Result<PublicCredentials> {
        check_identifier(email)?;

        let (dn, pw) = match self.bind_dn_and_pw.as_ref() {
            Some(x) => x,
            None => bail!("Missing bind_dn and bind_password in LDAP login provider config"),
        };

        let (conn, mut ldap) = LdapConnAsync::new(&self.ldap_server).await?;
        ldap3::drive!(conn);
        ldap.simple_bind(dn, pw).await?.success()?;

        let (matches, _res) = ldap
            .search(
                &self.search_base,
                Scope::Subtree,
                &format!(
                    "(&(objectClass=inetOrgPerson)({}={}))",
                    self.mail_attr, email
                ),
                &self.attrs_to_retrieve,
            )
            .await?
            .success()?;

        if matches.is_empty() {
            bail!("No such user account");
        }
        if matches.len() > 1 {
            bail!("Multiple matching user accounts");
        }
        let user = SearchEntry::construct(matches.into_iter().next().unwrap());
        debug!("Found matching LDAP user for email {}: {}", email, user.dn);

        // cryptography
        let crstr = get_attr(&user, &self.crypto_root_attr)?;
        let cr = CryptoRoot(crstr);
        let public_key = cr.public_key()?;

        // storage
        let storage = self.storage_creds_from_ldap_user(&user).await?;
        drop(ldap);

        Ok(PublicCredentials {
            storage,
            public_key,
        })
    }
}

fn get_attr(user: &SearchEntry, attr: &str) -> Result<String> {
    Ok(user
        .attrs
        .get(attr)
        .ok_or(anyhow!("Missing attr: {}", attr))?
        .iter()
        .next()
        .ok_or(anyhow!("No value for attr: {}", attr))?
        .clone())
}

fn check_identifier(id: &str) -> Result<()> {
    let is_ok = id
        .chars()
        .all(|c| c.is_alphanumeric() || "-+_.@".contains(c));
    if !is_ok {
        bail!("Invalid username/email address, must contain only a-z A-Z 0-9 - + _ . @");
    }
    Ok(())
}
