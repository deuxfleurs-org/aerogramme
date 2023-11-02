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
use crate::storage;
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
        Ok(list.existing_mailbox_names())
    }

    /// Opens an existing mailbox given its IMAP name.
    pub async fn open_mailbox(&self, name: &str) -> Result<Option<Arc<Mailbox>>> {
        let (mut list, ct) = self.load_mailbox_list().await?;
        eprintln!("List of mailboxes:");
        for ent in list.0.iter() {
            eprintln!(" - {:?}", ent);
        }

        if let Some((uidvalidity, Some(mbid))) = list.get_mailbox(name) {
            let mb = self.open_mailbox_by_id(mbid, uidvalidity).await?;
            let mb_uidvalidity = mb.current_uid_index().await.uidvalidity;
            if mb_uidvalidity > uidvalidity {
                list.update_uidvalidity(name, mb_uidvalidity);
                self.save_mailbox_list(&list, ct).await?;
            }
            Ok(Some(mb))
        } else {
            Ok(None)
        }
    }

    /// Check whether mailbox exists
    pub async fn has_mailbox(&self, name: &str) -> Result<bool> {
        let (list, _ct) = self.load_mailbox_list().await?;
        Ok(list.has_mailbox(name))
    }

    /// Creates a new mailbox in the user's IMAP namespace.
    pub async fn create_mailbox(&self, name: &str) -> Result<()> {
        if name.ends_with(MAILBOX_HIERARCHY_DELIMITER) {
            bail!("Invalid mailbox name: {}", name);
        }

        let (mut list, ct) = self.load_mailbox_list().await?;
        match list.create_mailbox(name) {
            CreatedMailbox::Created(_, _) => {
                self.save_mailbox_list(&list, ct).await?;
                Ok(())
            }
            CreatedMailbox::Existed(_, _) => Err(anyhow!("Mailbox {} already exists", name)),
        }
    }

    /// Deletes a mailbox in the user's IMAP namespace.
    pub async fn delete_mailbox(&self, name: &str) -> Result<()> {
        if name == INBOX {
            bail!("Cannot delete INBOX");
        }

        let (mut list, ct) = self.load_mailbox_list().await?;
        if list.has_mailbox(name) {
            // TODO: actually delete mailbox contents
            list.set_mailbox(name, None);
            self.save_mailbox_list(&list, ct).await?;
            Ok(())
        } else {
            bail!("Mailbox {} does not exist", name);
        }
    }

    /// Renames a mailbox in the user's IMAP namespace.
    pub async fn rename_mailbox(&self, old_name: &str, new_name: &str) -> Result<()> {
        let (mut list, ct) = self.load_mailbox_list().await?;

        if old_name.ends_with(MAILBOX_HIERARCHY_DELIMITER) {
            bail!("Invalid mailbox name: {}", old_name);
        }
        if new_name.ends_with(MAILBOX_HIERARCHY_DELIMITER) {
            bail!("Invalid mailbox name: {}", new_name);
        }

        if old_name == INBOX {
            list.rename_mailbox(old_name, new_name)?;
            if !self.ensure_inbox_exists(&mut list, &ct).await? {
                self.save_mailbox_list(&list, ct).await?;
            }
        } else {
            let names = list.existing_mailbox_names();

            let old_name_w_delim = format!("{}{}", old_name, MAILBOX_HIERARCHY_DELIMITER);
            let new_name_w_delim = format!("{}{}", new_name, MAILBOX_HIERARCHY_DELIMITER);

            if names
                .iter()
                .any(|x| x == new_name || x.starts_with(&new_name_w_delim))
            {
                bail!("Mailbox {} already exists", new_name);
            }

            for name in names.iter() {
                if name == old_name {
                    list.rename_mailbox(name, new_name)?;
                } else if let Some(tail) = name.strip_prefix(&old_name_w_delim) {
                    let nnew = format!("{}{}", new_name_w_delim, tail);
                    list.rename_mailbox(name, &nnew)?;
                }
            }
            self.save_mailbox_list(&list, ct).await?;
        }
        Ok(())
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
    ) -> Result<Arc<Mailbox>> {
        {
            let cache = self.mailboxes.lock().unwrap();
            if let Some(mb) = cache.get(&id).and_then(Weak::upgrade) {
                return Ok(mb);
            }
        }

        let mb = Arc::new(Mailbox::open(&self.creds, id, min_uidvalidity).await?);

        let mut cache = self.mailboxes.lock().unwrap();
        if let Some(concurrent_mb) = cache.get(&id).and_then(Weak::upgrade) {
            drop(mb); // we worked for nothing but at least we didn't starve someone else
            Ok(concurrent_mb)
        } else {
            cache.insert(id, Arc::downgrade(&mb));
            Ok(mb)
        }
    }

    // ---- Mailbox list management ----

    async fn load_mailbox_list(&self) -> Result<(MailboxList, Option<CausalityToken>)> {
        let (mut list, ct) = match self.k2v.read_item(MAILBOX_LIST_PK, MAILBOX_LIST_SK).await {
            Err(k2v_client::Error::NotFound) => (MailboxList::new(), None),
            Err(e) => return Err(e.into()),
            Ok(cv) => {
                let mut list = MailboxList::new();
                for v in cv.value {
                    if let K2vValue::Value(vbytes) = v {
                        let list2 =
                            open_deserialize::<MailboxList>(&vbytes, &self.creds.keys.master)?;
                        list.merge(list2);
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
    ) -> Result<bool> {
        // If INBOX doesn't exist, create a new mailbox with that name
        // and save new mailbox list.
        // Also, ensure that the mpsc::watch that keeps track of the
        // inbox id is up-to-date.
        let saved;
        let (inbox_id, inbox_uidvalidity) = match list.create_mailbox(INBOX) {
            CreatedMailbox::Created(i, v) => {
                self.save_mailbox_list(list, ct.clone()).await?;
                saved = true;
                (i, v)
            }
            CreatedMailbox::Existed(i, v) => {
                saved = false;
                (i, v)
            }
        };
        let inbox_id = Some((inbox_id, inbox_uidvalidity));
        if *self.tx_inbox_id.borrow() != inbox_id {
            self.tx_inbox_id.send(inbox_id).unwrap();
        }

        Ok(saved)
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
}

// ---- User's mailbox list (serialized in K2V) ----

#[derive(Serialize, Deserialize)]
struct MailboxList(BTreeMap<String, MailboxListEntry>);

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

impl MailboxList {
    fn new() -> Self {
        Self(BTreeMap::new())
    }

    fn merge(&mut self, list2: Self) {
        for (k, v) in list2.0.into_iter() {
            if let Some(e) = self.0.get_mut(&k) {
                e.merge(&v);
            } else {
                self.0.insert(k, v);
            }
        }
    }

    fn existing_mailbox_names(&self) -> Vec<String> {
        self.0
            .iter()
            .filter(|(_, v)| v.id_lww.1.is_some())
            .map(|(k, _)| k.to_string())
            .collect()
    }

    fn has_mailbox(&self, name: &str) -> bool {
        matches!(
            self.0.get(name),
            Some(MailboxListEntry {
                id_lww: (_, Some(_)),
                ..
            })
        )
    }

    fn get_mailbox(&self, name: &str) -> Option<(ImapUidvalidity, Option<UniqueIdent>)> {
        self.0.get(name).map(
            |MailboxListEntry {
                 id_lww: (_, mailbox_id),
                 uidvalidity,
             }| (*uidvalidity, *mailbox_id),
        )
    }

    /// Ensures mailbox `name` maps to id `id`.
    /// If it already mapped to that, returns None.
    /// If a change had to be done, returns Some(new uidvalidity in mailbox).
    fn set_mailbox(&mut self, name: &str, id: Option<UniqueIdent>) -> Option<ImapUidvalidity> {
        let (ts, id, uidvalidity) = match self.0.get_mut(name) {
            None => {
                if id.is_none() {
                    return None;
                } else {
                    (now_msec(), id, ImapUidvalidity::new(1).unwrap())
                }
            }
            Some(MailboxListEntry {
                id_lww,
                uidvalidity,
            }) => {
                if id_lww.1 == id {
                    return None;
                } else {
                    (
                        std::cmp::max(id_lww.0 + 1, now_msec()),
                        id,
                        ImapUidvalidity::new(uidvalidity.get() + 1).unwrap(),
                    )
                }
            }
        };

        self.0.insert(
            name.into(),
            MailboxListEntry {
                id_lww: (ts, id),
                uidvalidity,
            },
        );
        Some(uidvalidity)
    }

    fn update_uidvalidity(&mut self, name: &str, new_uidvalidity: ImapUidvalidity) {
        match self.0.get_mut(name) {
            None => {
                self.0.insert(
                    name.into(),
                    MailboxListEntry {
                        id_lww: (now_msec(), None),
                        uidvalidity: new_uidvalidity,
                    },
                );
            }
            Some(MailboxListEntry { uidvalidity, .. }) => {
                *uidvalidity = std::cmp::max(*uidvalidity, new_uidvalidity);
            }
        }
    }

    fn create_mailbox(&mut self, name: &str) -> CreatedMailbox {
        if let Some(MailboxListEntry {
            id_lww: (_, Some(id)),
            uidvalidity,
        }) = self.0.get(name)
        {
            return CreatedMailbox::Existed(*id, *uidvalidity);
        }

        let id = gen_ident();
        let uidvalidity = self.set_mailbox(name, Some(id)).unwrap();
        CreatedMailbox::Created(id, uidvalidity)
    }

    fn rename_mailbox(&mut self, old_name: &str, new_name: &str) -> Result<()> {
        if let Some((uidvalidity, Some(mbid))) = self.get_mailbox(old_name) {
            if self.has_mailbox(new_name) {
                bail!(
                    "Cannot rename {} into {}: {} already exists",
                    old_name,
                    new_name,
                    new_name
                );
            }

            self.set_mailbox(old_name, None);
            self.set_mailbox(new_name, Some(mbid));
            self.update_uidvalidity(new_name, uidvalidity);
            Ok(())
        } else {
            bail!(
                "Cannot rename {} into {}: {} doesn't exist",
                old_name,
                new_name,
                old_name
            );
        }
    }
}

enum CreatedMailbox {
    Created(UniqueIdent, ImapUidvalidity),
    Existed(UniqueIdent, ImapUidvalidity),
}

// ---- User cache ----

lazy_static! {
    static ref USER_CACHE: std::sync::Mutex<HashMap<(String, storage::Builders), Weak<User>>> =
        std::sync::Mutex::new(HashMap::new());
}
