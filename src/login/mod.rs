pub mod ldap_provider;
pub mod static_provider;

use anyhow::Result;
use async_trait::async_trait;
use k2v_client::K2vClient;
use rusoto_core::HttpClient;
use rusoto_credential::{AwsCredentials, StaticProvider};
use rusoto_s3::S3Client;
use rusoto_signature::Region;

use crate::cryptoblob::Key as SymmetricKey;

#[async_trait]
pub trait LoginProvider {
    async fn login(&self, username: &str, password: &str) -> Result<Credentials>;
}

#[derive(Clone, Debug)]
pub struct Credentials {
    pub aws_access_key_id: String,
    pub aws_secret_access_key: String,
    pub bucket: String,
    pub master_key: SymmetricKey,
}

impl Credentials {
    pub fn k2v_client(&self, k2v_region: &Region) -> Result<K2vClient> {
        let aws_creds = AwsCredentials::new(
            self.aws_access_key_id.clone(),
            self.aws_secret_access_key.clone(),
            None,
            None,
        );

        Ok(K2vClient::new(
            k2v_region.clone(),
            self.bucket.clone(),
            aws_creds,
            None,
        )?)
    }

    pub fn s3_client(&self, s3_region: &Region) -> Result<S3Client> {
        let aws_creds_provider = StaticProvider::new_minimal(
            self.aws_access_key_id.clone(),
            self.aws_secret_access_key.clone(),
        );

        Ok(S3Client::new_with(
            HttpClient::new()?,
            aws_creds_provider,
            s3_region.clone(),
        ))
    }
}
