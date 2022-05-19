pub mod ldap_provider;
pub mod static_provider;

use anyhow::Result;
use async_trait::async_trait;
use k2v_client::K2vClient;
use rusoto_core::HttpClient;
use rusoto_credential::{AwsCredentials, StaticProvider};
use rusoto_s3::S3Client;
use rusoto_signature::Region;

use crate::cryptoblob::*;

#[async_trait]
pub trait LoginProvider {
    async fn login(&self, username: &str, password: &str) -> Result<Credentials>;
}

#[derive(Clone, Debug)]
pub struct Credentials {
    pub storage: StorageCredentials,
    pub keys: CryptoKeys,
}

#[derive(Clone, Debug)]
pub struct StorageCredentials {
    pub s3_region: Region,
    pub k2v_region: Region,

    pub aws_access_key_id: String,
    pub aws_secret_access_key: String,
    pub bucket: String,
}

#[derive(Clone, Debug)]
pub struct CryptoKeys {
    // Master key for symmetric encryption of mailbox data
    pub master: Key,
    // Public/private keypair for encryption of incomming emails
    pub secret: SecretKey,
    pub public: PublicKey,
}

// ----

impl Credentials {
    pub fn k2v_client(&self) -> Result<K2vClient> {
        self.storage.k2v_client()
    }
    pub fn s3_client(&self) -> Result<S3Client> {
        self.storage.s3_client()
    }
    pub fn bucket(&self) -> &str {
        self.storage.bucket.as_str()
    }
    pub fn dump_config(&self) {
        println!("aws_access_key_id = \"{}\"", self.storage.aws_access_key_id);
        println!("aws_secret_access_key = \"{}\"", self.storage.aws_secret_access_key);
        println!("master_key = \"{}\"", base64::encode(&self.keys.master));
        println!("secret_key = \"{}\"", base64::encode(&self.keys.secret));
    }
}

impl StorageCredentials {
    pub fn k2v_client(&self) -> Result<K2vClient> {
        let aws_creds = AwsCredentials::new(
            self.aws_access_key_id.clone(),
            self.aws_secret_access_key.clone(),
            None,
            None,
        );

        Ok(K2vClient::new(
            self.k2v_region.clone(),
            self.bucket.clone(),
            aws_creds,
            None,
        )?)
    }

    pub fn s3_client(&self) -> Result<S3Client> {
        let aws_creds_provider = StaticProvider::new_minimal(
            self.aws_access_key_id.clone(),
            self.aws_secret_access_key.clone(),
        );

        Ok(S3Client::new_with(
            HttpClient::new()?,
            aws_creds_provider,
            self.s3_region.clone(),
        ))
    }
}

impl CryptoKeys {
    pub fn init(storage: &StorageCredentials) -> Result<Self> {
        unimplemented!()
    }

    pub fn init_without_password(storage: &StorageCredentials, master_key: &Key, secret_key: &SecretKey) -> Result<Self> {
        unimplemented!()
    }

    pub fn open(storage: &StorageCredentials, password: &str) -> Result<Self> {
        unimplemented!()
    }

    pub fn open_without_password(storage: &StorageCredentials, master_key: &Key, secret_key: &SecretKey) -> Result<Self> {
        unimplemented!()
    }

    pub fn add_password(&self, storage: &StorageCredentials, password: &str) -> Result<()> {
        unimplemented!()
    }

    pub fn remove_password(&self, storage: &StorageCredentials, password: &str, allow_remove_all: bool) -> Result<()> {
        unimplemented!()
    }
}

