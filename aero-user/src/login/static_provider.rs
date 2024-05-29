use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{anyhow, bail};
use async_trait::async_trait;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::watch;

use crate::config::*;
use crate::login::*;
use crate::storage;

pub struct ContextualUserEntry {
    pub username: String,
    pub config: UserEntry,
}

#[derive(Default)]
pub struct UserDatabase {
    users: HashMap<String, Arc<ContextualUserEntry>>,
    users_by_email: HashMap<String, Arc<ContextualUserEntry>>,
}

pub struct StaticLoginProvider {
    user_db: watch::Receiver<UserDatabase>,
    in_memory_store: storage::in_memory::MemDb,
    garage_store: storage::garage::GarageRoot,
}

pub async fn update_user_list(config: PathBuf, up: watch::Sender<UserDatabase>) -> Result<()> {
    let mut stream = signal(SignalKind::user_defined1())
        .expect("failed to install SIGUSR1 signal hander for reload");

    loop {
        let ulist: UserList = match read_config(config.clone()) {
            Ok(x) => x,
            Err(e) => {
                tracing::warn!(path=%config.as_path().to_string_lossy(), error=%e, "Unable to load config");
                stream.recv().await;
                continue;
            }
        };

        let users = ulist
            .into_iter()
            .map(|(username, config)| {
                (
                    username.clone(),
                    Arc::new(ContextualUserEntry { username, config }),
                )
            })
            .collect::<HashMap<_, _>>();

        let mut users_by_email = HashMap::new();
        for (_, u) in users.iter() {
            for m in u.config.email_addresses.iter() {
                if users_by_email.contains_key(m) {
                    tracing::warn!("Several users have the same email address: {}", m);
                    stream.recv().await;
                    continue;
                }
                users_by_email.insert(m.clone(), u.clone());
            }
        }

        tracing::info!("{} users loaded", users.len());
        up.send(UserDatabase {
            users,
            users_by_email,
        })
        .context("update user db config")?;
        stream.recv().await;
        tracing::info!("Received SIGUSR1, reloading");
    }
}

impl StaticLoginProvider {
    pub async fn new(config: LoginStaticConfig) -> Result<Self> {
        let (tx, mut rx) = watch::channel(UserDatabase::default());

        tokio::spawn(update_user_list(config.user_list, tx));
        rx.changed().await?;

        Ok(Self {
            user_db: rx,
            in_memory_store: storage::in_memory::MemDb::new(),
            garage_store: storage::garage::GarageRoot::new()?,
        })
    }
}

#[async_trait]
impl LoginProvider for StaticLoginProvider {
    async fn login(&self, username: &str, password: &str) -> Result<Credentials> {
        tracing::debug!(user=%username, "login");
        let user = {
            let user_db = self.user_db.borrow();
            match user_db.users.get(username) {
                None => bail!("User {} does not exist", username),
                Some(u) => u.clone(),
            }
        };

        tracing::debug!(user=%username, "verify password");
        if !verify_password(password, &user.config.password)? {
            bail!("Wrong password");
        }

        tracing::debug!(user=%username, "fetch keys");
        let storage: storage::Builder = match &user.config.storage {
            StaticStorage::InMemory => self.in_memory_store.builder(username).await,
            StaticStorage::Garage(grgconf) => {
                self.garage_store.user(storage::garage::GarageConf {
                    region: grgconf.aws_region.clone(),
                    k2v_endpoint: grgconf.k2v_endpoint.clone(),
                    s3_endpoint: grgconf.s3_endpoint.clone(),
                    aws_access_key_id: grgconf.aws_access_key_id.clone(),
                    aws_secret_access_key: grgconf.aws_secret_access_key.clone(),
                    bucket: grgconf.bucket.clone(),
                })?
            }
        };

        let cr = CryptoRoot(user.config.crypto_root.clone());
        let keys = cr.crypto_keys(password)?;

        tracing::debug!(user=%username, "logged");
        Ok(Credentials { storage, keys })
    }

    async fn public_login(&self, email: &str) -> Result<PublicCredentials> {
        let user = {
            let user_db = self.user_db.borrow();
            match user_db.users_by_email.get(email) {
                None => bail!("Email {} does not exist", email),
                Some(u) => u.clone(),
            }
        };
        tracing::debug!(user=%user.username, "public_login");

        let storage: storage::Builder = match &user.config.storage {
            StaticStorage::InMemory => self.in_memory_store.builder(&user.username).await,
            StaticStorage::Garage(grgconf) => {
                self.garage_store.user(storage::garage::GarageConf {
                    region: grgconf.aws_region.clone(),
                    k2v_endpoint: grgconf.k2v_endpoint.clone(),
                    s3_endpoint: grgconf.s3_endpoint.clone(),
                    aws_access_key_id: grgconf.aws_access_key_id.clone(),
                    aws_secret_access_key: grgconf.aws_secret_access_key.clone(),
                    bucket: grgconf.bucket.clone(),
                })?
            }
        };

        let cr = CryptoRoot(user.config.crypto_root.clone());
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
