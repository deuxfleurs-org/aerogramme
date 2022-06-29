use anyhow::Result;

use k2v_client::K2vClient;
use rusoto_s3::S3Client;

use crate::login::Credentials;
use crate::mail::mailbox::Mailbox;

pub struct User {
    pub username: String,
    pub creds: Credentials,
    pub s3_client: S3Client,
    pub k2v_client: K2vClient,
}

impl User {
    pub fn new(username: String, creds: Credentials) -> Result<Self> {
        let s3_client = creds.s3_client()?;
        let k2v_client = creds.k2v_client()?;
        Ok(Self {
            username,
            creds,
            s3_client,
            k2v_client,
        })
    }

    /// Lists user's available mailboxes
    pub fn list_mailboxes(&self) -> Result<Vec<String>> {
        unimplemented!()
    }

    /// Opens an existing mailbox given its IMAP name.
    pub fn open_mailbox(&self, name: &str) -> Result<Option<Mailbox>> {
        Mailbox::new(&self.creds, name).map(Some)
    }

    /// Creates a new mailbox in the user's IMAP namespace.
    pub fn create_mailbox(&self, name: &str) -> Result<()> {
        unimplemented!()
    }

    /// Deletes a mailbox in the user's IMAP namespace.
    pub fn delete_mailbox(&self, name: &str) -> Result<()> {
        unimplemented!()
    }

    /// Renames a mailbox in the user's IMAP namespace.
    pub fn rename_mailbox(&self, old_name: &str, new_name: &str) -> Result<()> {
        unimplemented!()
    }
}
