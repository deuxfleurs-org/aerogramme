use std::collections::BTreeSet;

use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};

use aero_bayou::timestamp::now_msec;
use aero_bayou::{Bayou, BayouWeak};
use aero_user::cryptoblob::{self, gen_key, Key};
use aero_user::login::Credentials;
use aero_user::migrate::Migrate;
use aero_user::storage::{self, BlobRef, BlobVal, RowRef, RowVal, Selector, StorageError, Store};

use crate::mail::query::{Query, QueryScope};
use crate::mail::uidindex::*;
use crate::unique_ident::*;

/// A mailbox stored in the backing store.
///
/// Remote updates are only applied to the mailbox state when calling `sync`.
/// Between calls to `sync`, the mailbox state only changes when calling mailbox
/// update functions (e.g. `append`).
///
/// Note that mailbox updates are immediately sent to the backing store, and are
/// thus available for other replicas to read (as soon as they call `sync`).
/// It is recommended to `sync` as often as possible to minimize the risk of
/// conflicts between concurrent updates.
///
/// These guarantees allow safely operating on the "local view" of a
/// mailbox without it being disturbed by concurrent modifications: you can just
/// avoid calling `sync` until you are done.
///
/// LIMITATION: the "local view" guarantees apply to mail indexing/numbering.
/// Furthermore, email metadata and bodies are immutable: they cannot be
/// mutated. However, email metadata and bodies CAN be concurrently *deleted* by
/// other replicas, and it will be visible immediately. It is thus possible that
/// fetching an email fails even though the email is referenced in the local
/// mailbox. This means that the local mailbox state is stale and must be
/// updated using `sync`.
///
/// A `Mailbox` is cheap to clone: copies will reuse the same underlying
/// ressources. It is thus more efficient to clone an existing `Mailbox` than to
/// `open` one from scratch.
#[derive(Clone)]
pub struct Mailbox {
    pub id: UniqueIdent,
    mbox: MailboxInternal,
}

impl Mailbox {
    pub(crate) async fn open(creds: &Credentials, prefix: &str, id: UniqueIdent) -> Result<Self> {
        let index_path = format!("{}/index/{}", prefix, id);
        let mail_path = format!("{}/blob/{}", prefix, id);

        let mut uid_index = Bayou::<UidIndex>::new(creds, index_path).await?;
        uid_index.sync().await?;

        // @FIXME reporting through opentelemetry or some logs
        // info on the "shape" of the mailbox would be welcomed
        /*
        dump(&uid_index);
        */

        let mbox = MailboxInternal {
            encryption_key: creds.keys.master.clone(),
            storage: creds.storage.clone(),
            uid_index,
            mail_path,
        };

        Ok(Self { id, mbox })
    }

    /// Sync data with backing store. This updates the mailbox state.
    pub async fn sync(&mut self) -> Result<()> {
        self.mbox.sync().await
    }

    /// Block until some updates are availble in the backing store.
    /// This does not update the mailbox state, you need to call `sync` to
    /// import the updates.
    pub fn notify(&self) -> std::sync::Weak<tokio::sync::Notify> {
        self.mbox.notifier()
    }

    // ---- Functions for reading the mailbox ----

    /// Get a clone of the current UID Index of this mailbox
    /// (cloning is cheap so don't hesitate to use this)
    pub fn current_uid_index(&self) -> UidIndex {
        self.mbox.uid_index.state().clone()
    }

    /// Fetch the metadata (headers + some more info) of the specified
    /// mail IDs
    pub async fn fetch_meta(&self, ids: &[UniqueIdent]) -> Result<Vec<MailMetaResult>> {
        self.mbox.fetch_meta(ids).await
    }

    /// Fetch an entire e-mail
    pub async fn fetch_full(&self, id: UniqueIdent, message_key: &Key) -> Result<Option<Vec<u8>>> {
        self.mbox.fetch_full(id, message_key).await
    }

    /// Build a query on this mailbox.
    pub fn query(&self, uuids: Vec<UniqueIdent>, scope: QueryScope) -> Query {
        Query {
            mailbox: self.clone(),
            emails: uuids,
            scope,
        }
    }

    // ---- Functions for changing the mailbox ----

    /// Add flags to message
    pub async fn add_flags<'a>(&mut self, id: UniqueIdent, flags: &Flags) -> Result<()> {
        self.mbox.add_flags(id, flags).await
    }

    /// Delete flags from message
    pub async fn del_flags<'a>(&mut self, id: UniqueIdent, flags: &Flags) -> Result<()> {
        self.mbox.del_flags(id, flags).await
    }

    /// Define the new flags for this message
    pub async fn set_flags<'a>(&mut self, id: UniqueIdent, flags: &Flags) -> Result<()> {
        self.mbox.set_flags(id, flags).await
    }

    /// Set all current emails as not "recent" anymore
    pub async fn bump_recent(&mut self) -> Result<()> {
        self.mbox.bump_recent().await
    }

    pub async fn increase_uidvalidity(&mut self, delta: u32) -> Result<()> {
        self.mbox.increase_uidvalidity(delta).await
    }

    /// Insert an email into the mailbox
    pub async fn append<'a>(
        &mut self,
        raw_mail: &[u8],
        flags: &Flags,
    ) -> Result<(ImapUid, ModSeq)> {
        self.mbox.append(raw_mail, flags).await
    }

    /// Insert an email into the mailbox, copying it from an existing S3 object
    pub async fn append_from_s3<'a>(
        &mut self,
        raw_mail: &[u8],
        ident: UniqueIdent,
        blob_ref: storage::BlobRef,
        message_key: Key,
    ) -> Result<()> {
        self.mbox
            .append_from_s3(raw_mail, ident, blob_ref, message_key)
            .await
    }

    /// Delete a message definitively from the mailbox
    pub async fn delete<'a>(&mut self, id: UniqueIdent) -> Result<()> {
        self.mbox.delete(id).await
    }

    /// Copy an email from an other Mailbox to this mailbox
    /// (use this when possible, as it allows for a certain number of storage optimizations)
    pub async fn copy_from(&mut self, from: &Mailbox, uuid: UniqueIdent) -> Result<UniqueIdent> {
        if self.id == from.id {
            bail!("Cannot copy into same mailbox");
        }

        self.mbox.copy_from(&from.mbox, uuid).await
    }

    /// Set the mailbox as deleted and empty its contents.
    pub async fn delete_mailbox(&mut self) -> Result<()> {
        //@TODO: actually delete mailbox contents
        let delete_mailbox_op = self.mbox.uid_index.state().op_mailbox_delete();
        self.mbox.uid_index.push(delete_mailbox_op).await
    }

    pub fn downgrade(&self) -> MailboxWeak {
        MailboxWeak {
            id: self.id.clone(),
            mail_path: self.mbox.mail_path.clone(),
            encryption_key: self.mbox.encryption_key.clone(),
            storage: self.mbox.storage.clone(),
            uid_index: self.mbox.uid_index.downgrade(),
        }
    }
}

/// A "weak reference" to a mailbox.
///
/// `Mailbox`/`MailboxWeak` work similarly to `Arc`/`Weak`.
///
/// This is useful to reference the mailbox in a cache while allowing its
/// resources to be destroyed if it is not used elsewhere.
pub struct MailboxWeak {
    id: UniqueIdent,
    mail_path: String,
    encryption_key: Key,
    storage: Store,
    uid_index: BayouWeak<UidIndex>,
}

impl MailboxWeak {
    pub fn upgrade(&self) -> Option<Mailbox> {
        let uid_index = self.uid_index.upgrade()?;
        Some(Mailbox {
            id: self.id.clone(),
            mbox: MailboxInternal {
                mail_path: self.mail_path.clone(),
                encryption_key: self.encryption_key.clone(),
                storage: self.storage.clone(),
                uid_index,
            },
        })
    }
}

// ---- internals

// Non standard but common flags:
// https://www.iana.org/assignments/imap-jmap-keywords/imap-jmap-keywords.xhtml
#[derive(Clone)]
struct MailboxInternal {
    mail_path: String,
    encryption_key: Key,
    storage: Store,
    uid_index: Bayou<UidIndex>,
}

impl MailboxInternal {
    async fn sync(&mut self) -> Result<()> {
        self.uid_index.sync().await?;
        Ok(())
    }

    fn notifier(&self) -> std::sync::Weak<tokio::sync::Notify> {
        self.uid_index.notifier()
    }

    // ---- Functions for reading the mailbox ----

    async fn fetch_meta(&self, ids: &[UniqueIdent]) -> Result<Vec<MailMetaResult>> {
        let ids = ids.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let sort_list = ids.iter().map(|x| x.as_str()).collect::<Vec<_>>();
        let res_vec = self
            .storage
            .row_fetch_batch(&Selector::List {
                shard: self.mail_path.as_str(),
                sort_list: &sort_list,
            })
            .await?;

        let mut meta_vec = vec![];
        for res in res_vec.into_iter() {
            let mut meta_opt = None;

            // Resolve conflicts
            for v in res.value.iter() {
                match v {
                    storage::Alternative::Tombstone => (),
                    storage::Alternative::Value(v) => {
                        let meta = MailMeta::open_decode(v, &self.encryption_key)?;
                        match meta_opt.as_mut() {
                            None => {
                                meta_opt = Some(meta);
                            }
                            Some(prevmeta) => {
                                prevmeta.try_merge(meta)?;
                            }
                        }
                    }
                }
            }
            if let Some(meta) = meta_opt {
                meta_vec.push(MailMetaResult::MailMeta(meta));
            } else {
                meta_vec.push(MailMetaResult::NotFound)
            }
        }

        Ok(meta_vec)
    }

    async fn fetch_full(&self, id: UniqueIdent, message_key: &Key) -> Result<Option<Vec<u8>>> {
        let obj_res = self
            .storage
            .blob_fetch(&BlobRef(format!("{}/{}", self.mail_path, id)))
            .await;
        match obj_res {
            Ok(obj) => {
                let bytes = cryptoblob::open(&obj.value, message_key)?;
                Ok(Some(bytes))
            }
            Err(StorageError::NotFound) => Ok(None),
            Err(e) => Err(e)?,
        }
    }

    // ---- Functions for changing the mailbox ----

    async fn add_flags(&mut self, ident: UniqueIdent, flags: &Flags) -> Result<()> {
        let add_flag_op = self.uid_index.state().op_flag_add(ident, flags.clone());
        self.uid_index.push(add_flag_op).await
    }

    async fn del_flags(&mut self, ident: UniqueIdent, flags: &Flags) -> Result<()> {
        let del_flag_op = self.uid_index.state().op_flag_del(ident, flags.clone());
        self.uid_index.push(del_flag_op).await
    }

    async fn set_flags(&mut self, ident: UniqueIdent, flags: &Flags) -> Result<()> {
        let set_flag_op = self.uid_index.state().op_flag_set(ident, flags.clone());
        self.uid_index.push(set_flag_op).await
    }

    async fn bump_recent(&mut self) -> Result<()> {
        let state = self.uid_index.state();
        // NOTE this function is called often (after each IMAP command), it is
        // important that it does not push a log operation each time...
        if state.recent_from != state.uidnext() {
            let bump_op = self.uid_index.state().op_bump_recent();
            self.uid_index.push(bump_op).await
        } else {
            Ok(())
        }
    }

    async fn increase_uidvalidity(&mut self, delta: u32) -> Result<()> {
        if delta > 0 {
            let op = self.uid_index.state().op_uidvalidity_add(delta);
            self.uid_index.push(op).await
        } else {
            Ok(())
        }
    }

    async fn append(&mut self, raw_mail: &[u8], flags: &Flags) -> Result<(ImapUid, ModSeq)> {
        let ident = gen_ident();
        let message_key = gen_key();

        futures::try_join!(
            async {
                // Encrypt and save mail body
                let message_blob = cryptoblob::seal(raw_mail, &message_key)?;
                self.storage
                    .blob_insert(BlobVal::new(
                        BlobRef(format!("{}/{}", self.mail_path, ident)),
                        message_blob,
                    ))
                    .await?;
                Ok::<_, anyhow::Error>(())
            },
            async {
                // Save mail meta
                let meta = MailMeta {
                    internaldate: now_msec(),
                    headers: eml_codec::raw_headers(raw_mail).to_vec(),
                    message_key: message_key.clone(),
                    rfc822_size: raw_mail.len(),
                };
                let meta_blob = meta.seal_encode(&self.encryption_key)?;
                self.storage
                    .row_update(vec![RowVal::new(
                        RowRef::new(&self.mail_path, &ident.to_string()),
                        meta_blob,
                    )])
                    .await?;
                Ok::<_, anyhow::Error>(())
            },
            async {
                self.uid_index.internal_sync_hint().await;
                Ok(())
            },
        )?;

        // Add mail to Bayou mail index
        let uid_state = self.uid_index.state();
        let add_mail_op = uid_state.op_mail_add(ident, flags.clone());
        self.uid_index.push(add_mail_op).await?;
        let (uid, modseq, _flags) = self.uid_index.state().table.get(&ident).unwrap();
        Ok((*uid, *modseq))
    }

    async fn append_from_s3<'a>(
        &mut self,
        raw_mail: &'a [u8],
        ident: UniqueIdent,
        blob_src: storage::BlobRef,
        message_key: Key,
    ) -> Result<()> {
        futures::try_join!(
            async {
                // Copy mail body from previous location
                let blob_dst = BlobRef(format!("{}/{}", self.mail_path, ident));
                self.storage.blob_copy(&blob_src, &blob_dst).await?;
                Ok::<_, anyhow::Error>(())
            },
            async {
                // Save mail meta
                let meta = MailMeta {
                    internaldate: now_msec(),
                    headers: eml_codec::raw_headers(raw_mail).to_vec(),
                    message_key: message_key.clone(),
                    rfc822_size: raw_mail.len(),
                };
                let meta_blob = meta.seal_encode(&self.encryption_key)?;
                self.storage
                    .row_update(vec![RowVal::new(
                        RowRef::new(&self.mail_path, &ident.to_string()),
                        meta_blob,
                    )])
                    .await?;
                Ok::<_, anyhow::Error>(())
            },
            async {
                self.uid_index.internal_sync_hint().await;
                Ok(())
            },
        )?;

        // Add mail to Bayou mail index
        let add_mail_op = self.uid_index.state().op_mail_add(ident, BTreeSet::new());
        self.uid_index.push(add_mail_op).await?;

        Ok(())
    }

    async fn delete(&mut self, ident: UniqueIdent) -> Result<()> {
        if !self.uid_index.state().table.contains_key(&ident) {
            bail!("Cannot delete mail that doesn't exist");
        }

        let del_mail_op = self.uid_index.state().op_mail_del(ident);
        self.uid_index.push(del_mail_op).await?;

        futures::try_join!(
            async {
                // Delete mail body from S3
                self.storage
                    .blob_rm(&BlobRef(format!("{}/{}", self.mail_path, ident)))
                    .await?;
                Ok::<_, anyhow::Error>(())
            },
            async {
                // Delete mail meta from K2V
                let sk = ident.to_string();
                let rv = self.storage.row_fetch(&self.mail_path, &sk).await?;
                self.storage
                    .row_update(vec![storage::RowVal::deleted(rv.row_ref)])
                    .await?;
                Ok::<_, anyhow::Error>(())
            }
        )?;
        Ok(())
    }

    async fn copy_from(
        &mut self,
        from: &MailboxInternal,
        source_id: UniqueIdent,
    ) -> Result<UniqueIdent> {
        let new_id = gen_ident();
        self.copy_internal(from, source_id, new_id).await?;
        Ok(new_id)
    }

    async fn copy_internal(
        &mut self,
        from: &MailboxInternal,
        source_id: UniqueIdent,
        new_id: UniqueIdent,
    ) -> Result<()> {
        if self.encryption_key != from.encryption_key {
            bail!("Message to be copied/moved does not belong to same account.");
        }

        let flags = from
            .uid_index
            .state()
            .table
            .get(&source_id)
            .ok_or(anyhow!("Source mail not found"))?
            .2
            .clone();

        futures::try_join!(
            async {
                let dst = BlobRef(format!("{}/{}", self.mail_path, new_id));
                let src = BlobRef(format!("{}/{}", from.mail_path, source_id));
                self.storage.blob_copy(&src, &dst).await?;
                Ok::<_, anyhow::Error>(())
            },
            async {
                // Copy mail meta in K2V
                let meta_res = &from.fetch_meta(&[source_id]).await?[0];
                let meta = match meta_res {
                    MailMetaResult::NotFound => bail!("metadata not found for id {}", source_id),
                    MailMetaResult::MailMeta(meta) => meta,
                };
                let meta_blob = meta.seal_encode(&self.encryption_key)?;
                self.storage
                    .row_update(vec![RowVal::new(
                        RowRef::new(&self.mail_path, &new_id.to_string()),
                        meta_blob,
                    )])
                    .await?;
                Ok::<_, anyhow::Error>(())
            },
            async {
                self.uid_index.internal_sync_hint().await;
                Ok(())
            },
        )?;

        // Add mail to Bayou mail index
        let add_mail_op = self.uid_index.state().op_mail_add(new_id, flags);
        self.uid_index.push(add_mail_op).await?;

        Ok(())
    }
}

// ----

mod v06 {
    use aero_user::cryptoblob::Key;
    use aero_user::migrate::InitialFormat;
    use serde::{Deserialize, Serialize};

    /// The metadata of a message that is stored in K2V
    /// at pk = mail/<mailbox uuid>, sk = <message uuid>
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct MailMeta {
        /// INTERNALDATE field (milliseconds since epoch)
        pub internaldate: u64,
        /// Headers of the message. Used for search queries.
        pub headers: Vec<u8>,
        /// Secret key for decrypting entire message
        pub message_key: Key,
        /// RFC822 size
        pub rfc822_size: usize,
    }

    impl InitialFormat for MailMeta {
        const VERSION_MARKER: &'static [u8] = b"A06mailmt";
    }
}

pub use v06::*;

/// The result of fetching an email metadata entry (see `fetch_meta`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MailMetaResult {
    /// Successfully fetched mail metadata from storage.
    MailMeta(MailMeta),
    /// The mail metadata was not found in storage.
    NotFound,
}

impl MailMeta {
    fn try_merge(&mut self, other: Self) -> Result<()> {
        if self.headers != other.headers
            || self.message_key != other.message_key
            || self.rfc822_size != other.rfc822_size
        {
            bail!("Conflicting MailMeta values.");
        }
        self.internaldate = std::cmp::max(self.internaldate, other.internaldate);
        Ok(())
    }
}
