use std::collections::{HashMap, BTreeMap};
use std::sync::{Arc, Weak};

use anyhow::{Result, bail};
use lazy_static::lazy_static;
use serde::{Serialize, Deserialize};
use k2v_client::{K2vClient, CausalityToken, K2vValue};

use crate::cryptoblob::{seal_serialize, open_deserialize};
use crate::login::{Credentials, StorageCredentials};
use crate::mail::mailbox::Mailbox;
use crate::mail::unique_ident::{UniqueIdent, gen_ident};
use crate::mail::uidindex::ImapUidvalidity;
use crate::time::now_msec;

const MAILBOX_HIERARCHY_DELIMITER: &str = "/";

/// INBOX is the only mailbox that must always exist.
/// It is created automatically when the account is created.
/// IMAP allows the user to rename INBOX to something else,
/// in this case all messages from INBOX are moved to a mailbox
/// with the new name and the INBOX mailbox still exists and is empty.
/// In our implementation, we indeed move the underlying mailbox
/// to the new name (i.e. the new name has the same id as the previous
/// INBOX), and we create a new empty mailbox for INBOX.
const INBOX: &str = "INBOX";

pub struct User {
    pub username: String,
    pub creds: Credentials,
    pub k2v: K2vClient,
}

impl User {
    pub fn new(username: String, creds: Credentials) -> Result<Self> {
        let k2v = creds.k2v_client()?;
        Ok(Self {
            username,
            creds,
            k2v,
        })
    }

    /// Lists user's available mailboxes
    pub async fn list_mailboxes(&self) -> Result<Vec<String>> {
        let (list, _ct) = self.load_mailbox_list().await?;
        Ok(list.into_iter().map(|(k, _)| k).collect())
    }

    /// Opens an existing mailbox given its IMAP name.
    pub async fn open_mailbox(&self, name: &str) -> Result<Option<Arc<Mailbox>>> {
        let (list, _ct) = self.load_mailbox_list().await?;
        match list.get(name) {
            Some(MailboxListEntry { id_lww: (_, Some(mbid)), uidvalidity }) =>
                self.open_mailbox_by_id(*mbid, *uidvalidity).await,
            _ =>
                bail!("Mailbox does not exist: {}", name),
        }
    }

    /// Creates a new mailbox in the user's IMAP namespace.
    pub fn create_mailbox(&self, _name: &str) -> Result<()> {
        unimplemented!()
    }

    /// Deletes a mailbox in the user's IMAP namespace.
    pub fn delete_mailbox(&self, _name: &str) -> Result<()> {
        unimplemented!()
    }

    /// Renames a mailbox in the user's IMAP namespace.
    pub fn rename_mailbox(&self, _old_name: &str, _new_name: &str) -> Result<()> {
        unimplemented!()
    }

    // ---- Internal mailbox management ----

    async fn open_mailbox_by_id(&self, id: UniqueIdent, min_uidvalidity: ImapUidvalidity) -> Result<Option<Arc<Mailbox>>> {
        let cache_key = (self.creds.storage.clone(), id);

        {
            let cache = MAILBOX_CACHE.cache.lock().unwrap();
            if let Some(mb) = cache.get(&cache_key).and_then(Weak::upgrade) {
                return Ok(Some(mb));
            }
        }

        let mb = Arc::new(Mailbox::open(&self.creds, id, min_uidvalidity).await?);

        let mut cache = MAILBOX_CACHE.cache.lock().unwrap();
        if let Some(concurrent_mb) = cache.get(&cache_key).and_then(Weak::upgrade) {
            drop(mb); // we worked for nothing but at least we didn't starve someone else
            Ok(Some(concurrent_mb))
        } else {
            cache.insert(cache_key, Arc::downgrade(&mb));
            Ok(Some(mb))
        }
    }

    // ---- Mailbox list management ----

    async fn load_mailbox_list(&self) -> Result<(MailboxList, Option<CausalityToken>)> {
        let cv = match self.k2v.read_item("mailboxes", "list").await {
            Err(k2v_client::Error::NotFound) => return Ok((BTreeMap::new(), None)),
            Err(e) => return Err(e.into()),
            Ok(cv) => cv,
        };

        let mut list = BTreeMap::new();
        for v in cv.value {
            if let K2vValue::Value(vbytes) = v {
                let list2 = open_deserialize::<MailboxList>(&vbytes, &self.creds.keys.master)?;
                list = merge_mailbox_lists(list, list2);
            }
        }

        // If INBOX doesn't exist, create a new mailbox with that name
        // and save new mailbox list.
        match list.get_mut(INBOX) {
            None => {
                list.insert(INBOX.into(), MailboxListEntry {
                    id_lww: (now_msec(), Some(gen_ident())),
                    uidvalidity: ImapUidvalidity::new(1).unwrap(),
                });
                self.save_mailbox_list(&list, Some(cv.causality.clone())).await?;
            }
            Some(MailboxListEntry { id_lww, uidvalidity }) if id_lww.1.is_none() => {
                id_lww.0 = std::cmp::max(id_lww.0 + 1, now_msec());
                id_lww.1 = Some(gen_ident());
                *uidvalidity = ImapUidvalidity::new(uidvalidity.get() + 1).unwrap();
                self.save_mailbox_list(&list, Some(cv.causality.clone())).await?;
            }
            _ => (),
        }

        Ok((list, Some(cv.causality)))
    }

    async fn save_mailbox_list(&self, list: &MailboxList, ct: Option<CausalityToken>) -> Result<()> {
        let list_blob = seal_serialize(list, &self.creds.keys.master)?;
        self.k2v.insert_item("mailboxes", "list", list_blob, ct).await?;
        Ok(())
    }
}

// ---- User's mailbox list (serialized in K2V) ----

type MailboxList = BTreeMap<String, MailboxListEntry>;

#[derive(Serialize, Deserialize, Clone, Copy)]
struct MailboxListEntry {
    id_lww: (u64, Option<UniqueIdent>),
    uidvalidity: ImapUidvalidity,
}

impl MailboxListEntry {
    fn merge(&mut self, other: &Self) {
        // Simple CRDT merge rule
        if other.id_lww.0 > self.id_lww.0
        || (other.id_lww.0 == self.id_lww.0 && other.id_lww.1 > self.id_lww.1) {
            self.id_lww = other.id_lww;
        }
        self.uidvalidity = std::cmp::max(self.uidvalidity, other.uidvalidity);
    }
}

fn merge_mailbox_lists(mut list1: MailboxList, list2: MailboxList) -> MailboxList {
    for (k, v) in list2.into_iter() {
        if let Some(e) = list1.get_mut(&k) {
            e.merge(&v);
        } else {
            list1.insert(k, v);
        }
    }
    list1
}

// ---- Mailbox cache ----

struct MailboxCache {
    cache: std::sync::Mutex<HashMap<(StorageCredentials, UniqueIdent), Weak<Mailbox>>>,
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
