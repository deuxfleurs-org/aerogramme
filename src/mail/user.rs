use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Weak};

use anyhow::{anyhow, bail, Result};
use k2v_client::{CausalityToken, K2vClient, K2vValue};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use crate::cryptoblob::{open_deserialize, seal_serialize};
use crate::login::{Credentials, StorageCredentials};
use crate::mail::incoming::incoming_mail_watch_process;
use crate::mail::mailbox::Mailbox;
use crate::mail::uidindex::ImapUidvalidity;
use crate::mail::unique_ident::{gen_ident, UniqueIdent};
use crate::time::now_msec;

pub const MAILBOX_HIERARCHY_DELIMITER: char = '.';

/// INBOX is the only mailbox that must always exist.
/// It is created automatically when the account is created.
/// IMAP allows the user to rename INBOX to something else,
/// in this case all messages from INBOX are moved to a mailbox
/// with the new name and the INBOX mailbox still exists and is empty.
/// In our implementation, we indeed move the underlying mailbox
/// to the new name (i.e. the new name has the same id as the previous
/// INBOX), and we create a new empty mailbox for INBOX.
pub const INBOX: &str = "INBOX";

const MAILBOX_LIST_PK: &str = "mailboxes";
const MAILBOX_LIST_SK: &str = "list";

pub struct User {
    pub username: String,
    pub creds: Credentials,
    pub k2v: K2vClient,
    pub mailboxes: std::sync::Mutex<HashMap<UniqueIdent, Weak<Mailbox>>>,

    tx_inbox_id: watch::Sender<Option<(UniqueIdent, ImapUidvalidity)>>,
}

impl User {
    pub async fn new(username: String, creds: Credentials) -> Result<Arc<Self>> {
        let cache_key = (username.clone(), creds.storage.clone());

        {
            let cache = USER_CACHE.lock().unwrap();
            if let Some(u) = cache.get(&cache_key).and_then(Weak::upgrade) {
                return Ok(u);
            }
        }

        let user = Self::open(username, creds).await?;

        let mut cache = USER_CACHE.lock().unwrap();
        if let Some(concurrent_user) = cache.get(&cache_key).and_then(Weak::upgrade) {
            drop(user);
            Ok(concurrent_user)
        } else {
            cache.insert(cache_key, Arc::downgrade(&user));
            Ok(user)
        }
    }

    /// Lists user's available mailboxes
    pub async fn list_mailboxes(&self) -> Result<Vec<String>> {
        let (list, _ct) = self.load_mailbox_list().await?;
        Ok(list.into_iter().map(|(k, _)| k).collect())
    }

    /// Opens an existing mailbox given its IMAP name.
    pub async fn open_mailbox(&self, name: &str) -> Result<Option<Arc<Mailbox>>> {
        let (mut list, ct) = self.load_mailbox_list().await?;
        eprintln!("List of mailboxes: {:?}", list);
        match list.get_mut(name) {
            Some(MailboxListEntry {
                id_lww: (_, Some(mbid)),
                uidvalidity,
            }) => {
                let mb_opt = self.open_mailbox_by_id(*mbid, *uidvalidity).await?;
                if let Some(mb) = &mb_opt {
                    let mb_uidvalidity = mb.current_uid_index().await.uidvalidity;
                    if mb_uidvalidity > *uidvalidity {
                        *uidvalidity = mb_uidvalidity;
                        self.save_mailbox_list(&list, ct).await?;
                    }
                }
                Ok(mb_opt)
            }
            _ => bail!("Mailbox does not exist: {}", name),
        }
    }

    /// Creates a new mailbox in the user's IMAP namespace.
    pub async fn create_mailbox(&self, name: &str) -> Result<()> {
        let (mut list, ct) = self.load_mailbox_list().await?;
        match self.mblist_create_mailbox(&mut list, ct, name).await? {
            CreatedMailbox::Created(_, _) => Ok(()),
            CreatedMailbox::Existed(_, _) => Err(anyhow!("Mailbox {} already exists", name)),
        }
    }

    /// Deletes a mailbox in the user's IMAP namespace.
    pub async fn delete_mailbox(&self, _name: &str) -> Result<()> {
        bail!("Deleting mailboxes not implemented yet")
    }

    /// Renames a mailbox in the user's IMAP namespace.
    pub async fn rename_mailbox(&self, old_name: &str, new_name: &str) -> Result<()> {
        if old_name == INBOX {
            bail!("Renaming INBOX not implemented yet")
        } else {
            bail!("Renaming not implemented yet")
        }
    }

    // ---- Internal user & mailbox management ----

    async fn open(username: String, creds: Credentials) -> Result<Arc<Self>> {
        let k2v = creds.k2v_client()?;

        let (tx_inbox_id, rx_inbox_id) = watch::channel(None);

        let user = Arc::new(Self {
            username,
            creds: creds.clone(),
            k2v,
            tx_inbox_id,
            mailboxes: std::sync::Mutex::new(HashMap::new()),
        });

        // Ensure INBOX exists (done inside load_mailbox_list)
        user.load_mailbox_list().await?;

        tokio::spawn(incoming_mail_watch_process(
            Arc::downgrade(&user),
            user.creds.clone(),
            rx_inbox_id,
        ));

        Ok(user)
    }

    pub(super) async fn open_mailbox_by_id(
        &self,
        id: UniqueIdent,
        min_uidvalidity: ImapUidvalidity,
    ) -> Result<Option<Arc<Mailbox>>> {
        {
            let cache = self.mailboxes.lock().unwrap();
            if let Some(mb) = cache.get(&id).and_then(Weak::upgrade) {
                return Ok(Some(mb));
            }
        }

        let mb = Arc::new(Mailbox::open(&self.creds, id, min_uidvalidity).await?);

        let mut cache = self.mailboxes.lock().unwrap();
        if let Some(concurrent_mb) = cache.get(&id).and_then(Weak::upgrade) {
            drop(mb); // we worked for nothing but at least we didn't starve someone else
            Ok(Some(concurrent_mb))
        } else {
            cache.insert(id, Arc::downgrade(&mb));
            Ok(Some(mb))
        }
    }

    // ---- Mailbox list management ----

    async fn load_mailbox_list(&self) -> Result<(MailboxList, Option<CausalityToken>)> {
        let (mut list, ct) = match self.k2v.read_item(MAILBOX_LIST_PK, MAILBOX_LIST_SK).await {
            Err(k2v_client::Error::NotFound) => (BTreeMap::new(), None),
            Err(e) => return Err(e.into()),
            Ok(cv) => {
                let mut list = BTreeMap::new();
                for v in cv.value {
                    if let K2vValue::Value(vbytes) = v {
                        let list2 =
                            open_deserialize::<MailboxList>(&vbytes, &self.creds.keys.master)?;
                        list = merge_mailbox_lists(list, list2);
                    }
                }
                (list, Some(cv.causality))
            }
        };

        self.ensure_inbox_exists(&mut list, &ct).await?;

        Ok((list, ct))
    }

    async fn ensure_inbox_exists(
        &self,
        list: &mut MailboxList,
        ct: &Option<CausalityToken>,
    ) -> Result<()> {
        // If INBOX doesn't exist, create a new mailbox with that name
        // and save new mailbox list.
        // Also, ensure that the mpsc::watch that keeps track of the
        // inbox id is up-to-date.
        let (inbox_id, inbox_uidvalidity) =
            match self.mblist_create_mailbox(list, ct.clone(), INBOX).await? {
                CreatedMailbox::Created(i, v) => (i, v),
                CreatedMailbox::Existed(i, v) => (i, v),
            };
        let inbox_id = Some((inbox_id, inbox_uidvalidity));
        if *self.tx_inbox_id.borrow() != inbox_id {
            self.tx_inbox_id.send(inbox_id).unwrap();
        }

        Ok(())
    }

    async fn save_mailbox_list(
        &self,
        list: &MailboxList,
        ct: Option<CausalityToken>,
    ) -> Result<()> {
        let list_blob = seal_serialize(list, &self.creds.keys.master)?;
        self.k2v
            .insert_item(MAILBOX_LIST_PK, MAILBOX_LIST_SK, list_blob, ct)
            .await?;
        Ok(())
    }

    async fn mblist_create_mailbox(
        &self,
        list: &mut MailboxList,
        ct: Option<CausalityToken>,
        name: &str,
    ) -> Result<CreatedMailbox> {
        match list.get_mut(name) {
            None => {
                let (id, uidvalidity) = (gen_ident(), ImapUidvalidity::new(1).unwrap());
                list.insert(
                    name.into(),
                    MailboxListEntry {
                        id_lww: (now_msec(), Some(id)),
                        uidvalidity,
                    },
                );
                self.save_mailbox_list(&list, ct).await?;
                Ok(CreatedMailbox::Created(id, uidvalidity))
            }
            Some(MailboxListEntry {
                id_lww: id_lww @ (_, None),
                uidvalidity,
            }) => {
                let id = gen_ident();
                id_lww.0 = std::cmp::max(id_lww.0 + 1, now_msec());
                id_lww.1 = Some(id);
                *uidvalidity = ImapUidvalidity::new(uidvalidity.get() + 1).unwrap();
                let uidvalidity = *uidvalidity;
                self.save_mailbox_list(list, ct).await?;
                Ok(CreatedMailbox::Created(id, uidvalidity))
            }
            Some(MailboxListEntry {
                id_lww: (_, Some(id)),
                uidvalidity,
            }) => Ok(CreatedMailbox::Existed(*id, *uidvalidity)),
        }
    }
}

// ---- User's mailbox list (serialized in K2V) ----

type MailboxList = BTreeMap<String, MailboxListEntry>;

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
struct MailboxListEntry {
    id_lww: (u64, Option<UniqueIdent>),
    uidvalidity: ImapUidvalidity,
}

impl MailboxListEntry {
    fn merge(&mut self, other: &Self) {
        // Simple CRDT merge rule
        if other.id_lww.0 > self.id_lww.0
            || (other.id_lww.0 == self.id_lww.0 && other.id_lww.1 > self.id_lww.1)
        {
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

enum CreatedMailbox {
    Created(UniqueIdent, ImapUidvalidity),
    Existed(UniqueIdent, ImapUidvalidity),
}

// ---- User cache ----

lazy_static! {
    static ref USER_CACHE: std::sync::Mutex<HashMap<(String, StorageCredentials), Weak<User>>> =
        std::sync::Mutex::new(HashMap::new());
}
