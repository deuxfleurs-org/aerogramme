use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::watch;

use anyhow::{bail, Result};

use aero_user::login::Credentials;
use aero_user::storage;

use crate::crdt::Crdt;
use crate::mail::incoming::incoming_mail_watch_process;
use crate::mail::mailbox::{Mailbox, MailboxWeak};
use crate::mail::mailbox_list::{MailboxList, MailboxListVal};
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

        match list.get(name) {
            Some(MailboxListVal::Active { id }) => {
                let mb = self.inner.open_by_id(id).await?;
                Ok(Some(mb))
            }
            Some(MailboxListVal::Deleted { .. }) | None => Ok(None),
        }
    }

    /// Lists user's available mailboxes
    pub async fn list(&self) -> Result<Vec<String>> {
        let (list, _ct) = self.load_mailbox_list().await?;
        Ok(list.names())
    }

    /// Check whether mailbox exists
    pub async fn has(&self, name: &str) -> Result<bool> {
        let (list, _ct) = self.load_mailbox_list().await?;
        Ok(list.has(name))
    }

    /// Creates a new mailbox in the user's IMAP namespace.
    pub async fn create(&self, name: &str) -> Result<()> {
        // "If the mailbox name is suffixed with the server's hierarchy
        // separator character (as returned from the server by a LIST command),
        // this is a declaration that the client intends to create mailbox names
        // under this name in the hierarchy. Server implementations that do not
        // require this declaration MUST ignore the declaration."
        if name.ends_with(MAILBOX_HIERARCHY_DELIMITER) {
            return Ok(());
        }

        let (mut list, ct) = self.load_mailbox_list().await?;
        match self.create_in_list(&mut list, name).await? {
            CreateResult::Created(_id) => self.save_mailbox_list(&list, ct).await?,
            CreateResult::Existed(_id) => bail!("Mailbox {} already exists", name),
        }
        Ok(())
    }

    /// Deletes a mailbox in the user's IMAP namespace.
    pub async fn delete(&self, name: &str) -> Result<()> {
        if name == INBOX {
            bail!("Cannot delete INBOX");
        }

        let (mut list, ct) = self.load_mailbox_list().await?;
        if let DeleteResult::Deleted(mbid) = self.delete_in_list(&mut list, name).await? {
            self.save_mailbox_list(&list, ct).await?;
            // Signal to the mailbox that it has been deleted
            let mut mbox = self.inner.open_by_id(mbid).await?;
            mbox.delete_mailbox().await?;
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
            self.rename_in_list(&mut list, old_name, new_name).await?;
            let _res = self.ensure_inbox_exists(&mut list).await?;
        } else {
            let names = list.names();

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
                    self.rename_in_list(&mut list, name, new_name).await?;
                } else if let Some(tail) = name.strip_prefix(&old_name_w_delim) {
                    let nnew = format!("{}{}", new_name_w_delim, tail);
                    self.rename_in_list(&mut list, name, &nnew).await?;
                }
            }
        }
        self.save_mailbox_list(&list, ct).await?;
        Ok(())
    }

    // ---- internal mailbox list management ----

    async fn create_in_list(&self, list: &mut MailboxList, name: &str) -> Result<CreateResult> {
        let id = match list.get(name) {
            None => {
                // Mailbox is being created for the first time; just generate a
                // fresh id
                gen_ident()
            }
            Some(MailboxListVal::Deleted { prev_uidvalidity }) => {
                // Initialize a new mailbox with a starting UIDVALIDITY higher
                // than the previous mailbox with this name.
                let id = gen_ident();
                let mut mbox = self.inner.open_by_id(id).await?;
                mbox.increase_uidvalidity(prev_uidvalidity.get() + 16)
                    .await?;
                id
            }
            Some(MailboxListVal::Active { id }) => return Ok(CreateResult::Existed(id)),
        };

        list.set(name, MailboxListVal::Active { id });
        Ok(CreateResult::Created(id))
    }

    async fn delete_in_list(&self, list: &mut MailboxList, name: &str) -> Result<DeleteResult> {
        match list.get(name) {
            Some(MailboxListVal::Active { id }) => {
                let mbox = self.inner.open_by_id(id).await?;
                let prev_uidvalidity = mbox.current_uid_index().uidvalidity;
                list.set(name, MailboxListVal::Deleted { prev_uidvalidity });
                Ok(DeleteResult::Deleted(id))
            }
            Some(MailboxListVal::Deleted { .. }) | None => Ok(DeleteResult::Missing),
        }
    }

    async fn rename_in_list(
        &self,
        list: &mut MailboxList,
        old_name: &str,
        new_name: &str,
    ) -> Result<()> {
        // Check that we can perform the rename before modifying list
        let Some(MailboxListVal::Active { .. }) = list.get(old_name) else {
            bail!(
                "Cannot rename {} into {}: {} does not exist",
                old_name,
                new_name,
                old_name,
            )
        };

        if let Some(MailboxListVal::Active { .. }) = list.get(new_name) {
            bail!(
                "Cannot rename {} into {}: {} already exists",
                old_name,
                new_name,
                new_name,
            )
        }

        let res = self.delete_in_list(list, old_name).await?;
        assert!(matches!(res, DeleteResult::Deleted(_)));
        let res = self.create_in_list(list, new_name).await?;
        assert!(matches!(res, CreateResult::Created(_)));

        Ok(())
    }

    async fn load_mailbox_list(&self) -> Result<(MailboxList, Option<storage::RowRef>)> {
        let (mut list, row) =
            MailboxList::load_from_storage(&self.inner.creds, MAILBOX_LIST_PK, MAILBOX_LIST_SK)
                .await?;

        let mut created_default_mbx = false;
        for mbx in [DRAFTS, ARCHIVE, SENT, TRASH] {
            if let CreateResult::Created(_) = self.create_in_list(&mut list, mbx).await? {
                created_default_mbx = true;
            }
        }
        let inbox_res = self.ensure_inbox_exists(&mut list).await?;
        if created_default_mbx || matches!(inbox_res, CreateResult::Created(_)) {
            // We created some mailboxes and not saved them, so save them now
            self.save_mailbox_list(&list, row.clone()).await?;
        }

        Ok((list, row))
    }

    async fn ensure_inbox_exists(&self, list: &mut MailboxList) -> Result<CreateResult> {
        // If INBOX doesn't exist, create a new mailbox with that name
        // and save new mailbox list.
        // Also, ensure that the mpsc::watch that keeps track of the
        // inbox id is up-to-date.
        let res = self.create_in_list(list, INBOX).await?;
        let inbox_id = Some(res.id());
        if *self.tx_inbox_id.borrow() != inbox_id {
            self.tx_inbox_id.send(inbox_id).unwrap();
        }
        Ok(res)
    }

    async fn save_mailbox_list(
        &self,
        list: &MailboxList,
        ct: Option<storage::RowRef>,
    ) -> Result<()> {
        list.store_to_storage(&self.inner.creds, MAILBOX_LIST_PK, MAILBOX_LIST_SK, ct)
            .await
    }
}

impl MailboxNsInner {
    pub(crate) async fn open_by_id(&self, id: UniqueIdent) -> Result<Mailbox> {
        if let Some(mut mb) = {
            let cache = self.mailboxes.lock().unwrap();
            cache.get(&id).and_then(|mbox_weak| mbox_weak.upgrade())
        } {
            // Sync now after getting the mbox out of cache to get a recent mbox
            // state
            mb.sync().await?;
            return Ok(mb);
        }

        // The idea here is that:
        //  1. Opening a mailbox that is not already opened takes a significant amount of time
        //  2. We don't want to lock the whole HashMap that contain the mailboxes during this
        //     operation which is why we droppped the lock above but take it again below.
        let mb = Mailbox::open(&self.creds, "mail", id).await?;

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

enum CreateResult {
    Created(UniqueIdent),
    Existed(UniqueIdent),
}

impl CreateResult {
    fn id(&self) -> UniqueIdent {
        match self {
            Self::Created(id) => *id,
            Self::Existed(id) => *id,
        }
    }
}

enum DeleteResult {
    Deleted(UniqueIdent),
    Missing,
}
