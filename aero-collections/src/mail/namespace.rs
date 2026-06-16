use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use tokio::sync::watch;

use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};

use aero_bayou::timestamp::now_msec;
use aero_user::cryptoblob::{open_deserialize, seal_serialize};
use aero_user::login::Credentials;
use aero_user::storage;

use crate::mail::incoming::incoming_mail_watch_process;
use crate::mail::mailbox::{Mailbox, MailboxWeak};
use crate::unique_ident::{gen_ident, UniqueIdent};

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

/// For convenience purpose, we also create some special mailbox
/// that are described in RFC6154 SPECIAL-USE
/// @FIXME maybe it should be a configuration parameter
/// @FIXME maybe we should have a per-mailbox flag mechanism, either an enum or a string, so we
/// track which mailbox is used for what.
/// @FIXME Junk could be useful but we don't have any antispam solution yet so...
/// @FIXME IMAP supports virtual mailbox. \All or \Flagged are intended to be virtual mailboxes.
/// \Trash might be one, or not one. I don't know what we should do there.
pub const DRAFTS: &str = "Drafts";
pub const ARCHIVE: &str = "Archive";
pub const SENT: &str = "Sent";
pub const TRASH: &str = "Trash";

pub(crate) const MAILBOX_LIST_PK: &str = "mailboxes";
pub(crate) const MAILBOX_LIST_SK: &str = "list";

#[derive(Clone)]
pub struct MailboxNs {
    // `inner` is shared between clones of this struct, and with the worker that
    // processes incoming emails, (moving emails from the mailqueue to the
    // user's INBOX). See incoming.rs for the implementation of the worker.
    inner: Arc<MailboxNsInner>,
    // Channel to communicate with the worker and send it the INBOX id.
    tx_inbox_id: watch::Sender<Option<UniqueIdent>>,
}

pub(crate) struct MailboxNsInner {
    creds: Credentials,
    // A cache of already opened mailboxes. Opening a mailbox from scratch is
    // expensive, so it is better to clone it from the cache if possible.
    mailboxes: std::sync::Mutex<HashMap<UniqueIdent, MailboxWeak>>,
}

impl MailboxNs {
    pub async fn new(creds: Credentials) -> Result<Self> {
        let (tx_inbox_id, rx_inbox_id) = watch::channel(None);
        let inner = Arc::new(MailboxNsInner {
            mailboxes: std::sync::Mutex::new(HashMap::new()),
            creds: creds.clone(),
        });

        tokio::spawn(incoming_mail_watch_process(
            Arc::downgrade(&inner),
            creds,
            rx_inbox_id,
        ));

        let ns = Self { inner, tx_inbox_id };

        // Ensure INBOX exists (done inside load_mailbox_list)
        ns.load_mailbox_list().await?;

        Ok(ns)
    }

    /// Opens an existing mailbox given its IMAP name.
    pub async fn open(&self, name: &str) -> Result<Option<Mailbox>> {
        let (list, _ct) = self.load_mailbox_list().await?;

        //@FIXME it could be a trace or an opentelemtry trace thing.
        // Be careful to not leak sensible data
        /*
        eprintln!("List of mailboxes:");
        for ent in list.0.iter() {
            eprintln!(" - {:?}", ent);
        }
        */

        if let Some(mbid) = list.get_mailbox(name) {
            let mb = self.inner.open_by_id(mbid).await?;
            Ok(Some(mb))
        } else {
            Ok(None)
        }
    }

    /// Lists user's available mailboxes
    pub async fn list(&self) -> Result<Vec<String>> {
        let (list, _ct) = self.load_mailbox_list().await?;
        Ok(list.existing_mailbox_names())
    }

    /// Check whether mailbox exists
    pub async fn has(&self, name: &str) -> Result<bool> {
        let (list, _ct) = self.load_mailbox_list().await?;
        Ok(list.has_mailbox(name))
    }

    /// Creates a new mailbox in the user's IMAP namespace.
    pub async fn create(&self, name: &str) -> Result<()> {
        if name.ends_with(MAILBOX_HIERARCHY_DELIMITER) {
            bail!("Invalid mailbox name: {}", name);
        }

        let (mut list, ct) = self.load_mailbox_list().await?;
        match list.create_mailbox(name) {
            CreatedMailbox::Created(_) => {
                self.save_mailbox_list(&list, ct).await?;
                Ok(())
            }
            CreatedMailbox::Existed(_) => Err(anyhow!("Mailbox {} already exists", name)),
        }
    }

    /// Deletes a mailbox in the user's IMAP namespace.
    pub async fn delete(&self, name: &str) -> Result<()> {
        if name == INBOX {
            bail!("Cannot delete INBOX");
        }

        let (mut list, ct) = self.load_mailbox_list().await?;
        if list.has_mailbox(name) {
            //@TODO: actually delete mailbox contents
            list.set_mailbox(name, None);
            self.save_mailbox_list(&list, ct).await?;
            Ok(())
        } else {
            bail!("Mailbox {} does not exist", name);
        }
    }

    /// Renames a mailbox in the user's IMAP namespace.
    pub async fn rename(&self, old_name: &str, new_name: &str) -> Result<()> {
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

    // ---- internal mailbox list management ----

    async fn load_mailbox_list(&self) -> Result<(MailboxList, Option<storage::RowRef>)> {
        let (mut list, row) = match self
            .inner
            .creds
            .storage
            .row_fetch(MAILBOX_LIST_PK, MAILBOX_LIST_SK)
            .await
        {
            Err(storage::StorageError::NotFound) => (MailboxList::new(), None),
            Err(e) => return Err(e.into()),
            Ok(rv) => {
                let mut list = MailboxList::new();
                let (row_ref, row_vals) = (rv.row_ref, rv.value);

                for v in row_vals {
                    if let storage::Alternative::Value(vbytes) = v {
                        let list2 = open_deserialize::<MailboxList>(
                            &vbytes,
                            &self.inner.creds.keys.master,
                        )?;
                        list.merge(list2);
                    }
                }
                (list, Some(row_ref))
            }
        };

        let is_default_mbx_missing = [DRAFTS, ARCHIVE, SENT, TRASH]
            .iter()
            .map(|mbx| list.create_mailbox(mbx))
            .fold(false, |acc, r| {
                acc || matches!(r, CreatedMailbox::Created(..))
            });
        let is_inbox_missing = self.ensure_inbox_exists(&mut list, &row).await?;
        if is_default_mbx_missing && !is_inbox_missing {
            // It's the only case where we created some mailboxes and not saved them
            // So we save them!
            self.save_mailbox_list(&list, row.clone()).await?;
        }

        Ok((list, row))
    }

    async fn ensure_inbox_exists(
        &self,
        list: &mut MailboxList,
        ct: &Option<storage::RowRef>,
    ) -> Result<bool> {
        // If INBOX doesn't exist, create a new mailbox with that name
        // and save new mailbox list.
        // Also, ensure that the mpsc::watch that keeps track of the
        // inbox id is up-to-date.
        let saved;
        let inbox_id = match list.create_mailbox(INBOX) {
            CreatedMailbox::Created(i) => {
                self.save_mailbox_list(list, ct.clone()).await?;
                saved = true;
                i
            }
            CreatedMailbox::Existed(i) => {
                saved = false;
                i
            }
        };
        let inbox_id = Some(inbox_id);
        if *self.tx_inbox_id.borrow() != inbox_id {
            self.tx_inbox_id.send(inbox_id).unwrap();
        }

        Ok(saved)
    }

    async fn save_mailbox_list(
        &self,
        list: &MailboxList,
        ct: Option<storage::RowRef>,
    ) -> Result<()> {
        let list_blob = seal_serialize(list, &self.inner.creds.keys.master)?;
        let rref = ct.unwrap_or(storage::RowRef::new(MAILBOX_LIST_PK, MAILBOX_LIST_SK));
        let row_val = storage::RowVal::new(rref, list_blob);
        self.inner.creds.storage.row_update(vec![row_val]).await?;
        Ok(())
    }
}

impl MailboxNsInner {
    pub(crate) async fn open_by_id(&self, id: UniqueIdent) -> Result<Mailbox> {
        {
            let cache = self.mailboxes.lock().unwrap();
            if let Some(mbox_weak) = cache.get(&id) {
                if let Some(mb) = mbox_weak.upgrade() {
                    return Ok(mb);
                }
            }
        }

        // The idea here is that:
        //  1. Opening a mailbox that is not already opened takes a significant amount of time
        //  2. We don't want to lock the whole HashMap that contain the mailboxes during this
        //     operation which is why we droppped the lock above but take it again below.
        let mb = Mailbox::open(&self.creds, id).await?;

        let mut cache = self.mailboxes.lock().unwrap();
        if let Some(concurrent_mb_weak) = cache.get(&id) {
            if let Some(concurrent_mb) = concurrent_mb_weak.upgrade() {
                drop(mb); // we worked for nothing but at least we didn't starve someone else
                return Ok(concurrent_mb);
            }
        }
        cache.insert(id, mb.downgrade());
        Ok(mb)
    }
}

// ---- User's mailbox list (serialized in K2V) ----
// ---- these definitions are internal ----
// ---- They are purely concerned with operating on the MailboxList datastructure, ----
// ---- no I/O or storage handling there ----

#[derive(Debug, Serialize, Deserialize)]
struct MailboxList(BTreeMap<String, MailboxListEntry>);

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
struct MailboxListEntry {
    id_lww: (u64, Option<UniqueIdent>),
}

impl MailboxListEntry {
    fn merge(&mut self, other: &Self) {
        // Simple CRDT merge rule
        if other.id_lww.0 > self.id_lww.0
            || (other.id_lww.0 == self.id_lww.0 && other.id_lww.1 > self.id_lww.1)
        {
            self.id_lww = other.id_lww;
        }
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

    fn get_mailbox(&self, name: &str) -> Option<UniqueIdent> {
        self.0
            .get(name)
            .map(
                |MailboxListEntry {
                     id_lww: (_, mailbox_id),
                 }| *mailbox_id,
            )
            .flatten()
    }

    /// Ensures mailbox `name` maps to id `id`.
    /// If it already mapped to that, returns false.
    /// If a change had to be done, returns true.
    fn set_mailbox(&mut self, name: &str, id: Option<UniqueIdent>) -> bool {
        let (ts, id) = match self.0.get_mut(name) {
            None => {
                if id.is_none() {
                    return false;
                } else {
                    (now_msec(), id)
                }
            }
            Some(MailboxListEntry { id_lww }) => {
                if id_lww.1 == id {
                    return false;
                } else {
                    (std::cmp::max(id_lww.0 + 1, now_msec()), id)
                }
            }
        };

        self.0
            .insert(name.into(), MailboxListEntry { id_lww: (ts, id) });
        true
    }

    fn create_mailbox(&mut self, name: &str) -> CreatedMailbox {
        if let Some(id) = self.get_mailbox(name) {
            return CreatedMailbox::Existed(id);
        }

        let id = gen_ident();
        self.set_mailbox(name, Some(id));
        CreatedMailbox::Created(id)
    }

    fn rename_mailbox(&mut self, old_name: &str, new_name: &str) -> Result<()> {
        if let Some(mbid) = self.get_mailbox(old_name) {
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
    Created(UniqueIdent),
    Existed(UniqueIdent),
}
