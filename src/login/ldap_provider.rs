use anyhow::Result;
use async_trait::async_trait;
use ldap3::{LdapConnAsync, Scope, SearchEntry};
use log::debug;

use crate::config::*;
use crate::login::*;

pub struct LdapLoginProvider {
    k2v_region: Region,
    s3_region: Region,
    ldap_server: String,

    pre_bind_on_login: bool,
    bind_dn_and_pw: Option<(String, String)>,

    search_base: String,
    attrs_to_retrieve: Vec<String>,
    username_attr: String,
    mail_attr: String,

    aws_access_key_id_attr: String,
    aws_secret_access_key_attr: String,
    user_secret_attr: String,
    alternate_user_secrets_attr: Option<String>,

    bucket_source: BucketSource,
}

enum BucketSource {
    Constant(String),
    Attr(String),
}

impl LdapLoginProvider {
    pub fn new(config: LoginLdapConfig, k2v_region: Region, s3_region: Region) -> Result<Self> {
        let bind_dn_and_pw = match (config.bind_dn, config.bind_password) {
            (Some(dn), Some(pw)) => Some((dn, pw)),
            (None, None) => None,
            _ => bail!(
                "If either of `bind_dn` or `bind_password` is set, the other must be set as well."
            ),
        };

        let bucket_source = match (config.bucket, config.bucket_attr) {
            (Some(b), None) => BucketSource::Constant(b),
            (None, Some(a)) => BucketSource::Attr(a),
            _ => bail!("Must set `bucket` or `bucket_attr`, but not both"),
        };

        if config.pre_bind_on_login && bind_dn_and_pw.is_none() {
            bail!("Cannot use `pre_bind_on_login` without setting `bind_dn` and `bind_password`");
        }

        let mut attrs_to_retrieve = vec![
            config.username_attr.clone(),
            config.mail_attr.clone(),
            config.aws_access_key_id_attr.clone(),
            config.aws_secret_access_key_attr.clone(),
            config.user_secret_attr.clone(),
        ];
        if let Some(a) = &config.alternate_user_secrets_attr {
            attrs_to_retrieve.push(a.clone());
        }
        if let BucketSource::Attr(a) = &bucket_source {
            attrs_to_retrieve.push(a.clone());
        }

        Ok(Self {
            k2v_region,
            s3_region,
            ldap_server: config.ldap_server,
            pre_bind_on_login: config.pre_bind_on_login,
            bind_dn_and_pw,
            search_base: config.search_base,
            attrs_to_retrieve,
            username_attr: config.username_attr,
            mail_attr: config.mail_attr,
            aws_access_key_id_attr: config.aws_access_key_id_attr,
            aws_secret_access_key_attr: config.aws_secret_access_key_attr,
            user_secret_attr: config.user_secret_attr,
            alternate_user_secrets_attr: config.alternate_user_secrets_attr,
            bucket_source,
        })
    }

    fn storage_creds_from_ldap_user(&self, user: &SearchEntry) -> Result<StorageCredentials> {
        let aws_access_key_id = get_attr(user, &self.aws_access_key_id_attr)?;
        let aws_secret_access_key = get_attr(user, &self.aws_secret_access_key_attr)?;
        let bucket = match &self.bucket_source {
            BucketSource::Constant(b) => b.clone(),
            BucketSource::Attr(a) => get_attr(user, a)?,
        };

        Ok(StorageCredentials {
            k2v_region: self.k2v_region.clone(),
            s3_region: self.s3_region.clone(),
            aws_access_key_id,
            aws_secret_access_key,
            bucket,
        })
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

        let storage = self.storage_creds_from_ldap_user(&user)?;

        let user_secret = get_attr(&user, &self.user_secret_attr)?;
        let alternate_user_secrets = match &self.alternate_user_secrets_attr {
            None => vec![],
            Some(a) => user.attrs.get(a).cloned().unwrap_or_default(),
        };
        let user_secrets = UserSecrets {
            user_secret,
            alternate_user_secrets,
        };

        drop(ldap);

        let keys = CryptoKeys::open(&storage, &user_secrets, password).await?;

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

        let storage = self.storage_creds_from_ldap_user(&user)?;
        drop(ldap);

        let k2v_client = storage.k2v_client()?;
        let (_, public_key) = CryptoKeys::load_salt_and_public(&k2v_client).await?;

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
