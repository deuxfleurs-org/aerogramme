use std::collections::HashMap;
use std::sync::{Arc, Weak};

use anyhow::Result;
use lazy_static::lazy_static;

use k2v_client::K2vClient;
use rusoto_s3::S3Client;

use crate::login::{Credentials, StorageCredentials};
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
    pub async fn open_mailbox(&self, name: &str) -> Result<Option<Arc<Mailbox>>> {
        {
            let cache = MAILBOX_CACHE.cache.lock().unwrap();
            if let Some(mb) = cache.get(&self.creds.storage).and_then(Weak::upgrade) {
                return Ok(Some(mb));
            }
        }

        let mb = Arc::new(Mailbox::open(&self.creds, name).await?);

        let mut cache = MAILBOX_CACHE.cache.lock().unwrap();
        if let Some(concurrent_mb) = cache.get(&self.creds.storage).and_then(Weak::upgrade) {
            drop(mb);   // we worked for nothing but at least we didn't starve someone else
            Ok(Some(concurrent_mb))
        } else {
            cache.insert(self.creds.storage.clone(), Arc::downgrade(&mb));
            Ok(Some(mb))
        }
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

// ---- Mailbox cache ----

struct MailboxCache {
    cache: std::sync::Mutex<HashMap<StorageCredentials, Weak<Mailbox>>>,
}

impl MailboxCache {
    fn new() -> Self {
        Self {
            cache: std::sync::Mutex::new(HashMap::new()),
        }
    }
}

lazy_static! {
    static ref MAILBOX_CACHE: MailboxCache = MailboxCache::new();
}
