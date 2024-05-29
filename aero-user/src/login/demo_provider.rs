use crate::login::*;
use crate::storage::*;

pub struct DemoLoginProvider {
    keys: CryptoKeys,
    in_memory_store: in_memory::MemDb,
}

impl DemoLoginProvider {
    pub fn new() -> Self {
        Self {
            keys: CryptoKeys::init(),
            in_memory_store: in_memory::MemDb::new(),
        }
    }
}

#[async_trait]
impl LoginProvider for DemoLoginProvider {
    async fn login(&self, username: &str, password: &str) -> Result<Credentials> {
        tracing::debug!(user=%username, "login");

        if username != "alice" {
            bail!("user does not exist");
        }

        if password != "hunter2" {
            bail!("wrong password");
        }

        let storage = self.in_memory_store.builder("alice").await;
        let keys = self.keys.clone();

        Ok(Credentials { storage, keys })
    }

    async fn public_login(&self, email: &str) -> Result<PublicCredentials> {
        tracing::debug!(user=%email, "public_login");
        if email != "alice@example.tld" {
            bail!("invalid email address");
        }

        let storage = self.in_memory_store.builder("alice").await;
        let public_key = self.keys.public.clone();

        Ok(PublicCredentials {
            storage,
            public_key,
        })
    }
}
