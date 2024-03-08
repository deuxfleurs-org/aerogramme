use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Weak};

use anyhow::{anyhow, bail, Result};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use crate::cryptoblob::{open_deserialize, seal_serialize};
use crate::login::Credentials;
use crate::mail::incoming::incoming_mail_watch_process;
use crate::mail::mailbox::Mailbox;
use crate::mail::uidindex::ImapUidvalidity;
use crate::mail::unique_ident::{gen_ident, UniqueIdent};
use crate::storage;
use crate::timestamp::now_msec;

use crate::mail::namespace::{MAILBOX_HIERARCHY_DELIMITER, INBOX, DRAFTS, ARCHIVE, SENT, TRASH, MAILBOX_LIST_PK, MAILBOX_LIST_SK,MailboxList,CreatedMailbox};

//@FIXME User should be totally rewriten
//to extract the local mailbox list
//to the mail/namespace.rs file (and mailbox list should be reworded as mail namespace)

pub struct User {
    pub username: String,
    pub creds: Credentials,
    pub storage: storage::Store,
    pub mailboxes: std::sync::Mutex<HashMap<UniqueIdent, Weak<Mailbox>>>,

    tx_inbox_id: watch::Sender<Option<(UniqueIdent, ImapUidvalidity)>>,
}

impl User {
    pub async fn new(username: String, creds: Credentials) -> Result<Arc<Self>> {
        let cache_key = (username.clone(), creds.storage.unique());

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

        //@FIXME it could be a trace or an opentelemtry trace thing.
        // Be careful to not leak sensible data
        /*
        eprintln!("List of mailboxes:");
        for ent in list.0.iter() {
            eprintln!(" - {:?}", ent);
        }
        */

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
            //@TODO: actually delete mailbox contents
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
        let storage = creds.storage.build().await?;

        let (tx_inbox_id, rx_inbox_id) = watch::channel(None);

        let user = Arc::new(Self {
            username,
            creds: creds.clone(),
            storage,
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

    async fn load_mailbox_list(&self) -> Result<(MailboxList, Option<storage::RowRef>)> {
        let row_ref = storage::RowRef::new(MAILBOX_LIST_PK, MAILBOX_LIST_SK);
        let (mut list, row) = match self
            .storage
            .row_fetch(&storage::Selector::Single(&row_ref))
            .await
        {
            Err(storage::StorageError::NotFound) => (MailboxList::new(), None),
            Err(e) => return Err(e.into()),
            Ok(rv) => {
                let mut list = MailboxList::new();
                let (row_ref, row_vals) = match rv.into_iter().next() {
                    Some(row_val) => (row_val.row_ref, row_val.value),
                    None => (row_ref, vec![]),
                };

                for v in row_vals {
                    if let storage::Alternative::Value(vbytes) = v {
                        let list2 =
                            open_deserialize::<MailboxList>(&vbytes, &self.creds.keys.master)?;
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
        ct: Option<storage::RowRef>,
    ) -> Result<()> {
        let list_blob = seal_serialize(list, &self.creds.keys.master)?;
        let rref = ct.unwrap_or(storage::RowRef::new(MAILBOX_LIST_PK, MAILBOX_LIST_SK));
        let row_val = storage::RowVal::new(rref, list_blob);
        self.storage.row_insert(vec![row_val]).await?;
        Ok(())
    }
}

// ---- User cache ----

lazy_static! {
    static ref USER_CACHE: std::sync::Mutex<HashMap<(String, storage::UnicityBuffer), Weak<User>>> =
        std::sync::Mutex::new(HashMap::new());
}
