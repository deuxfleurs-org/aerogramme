use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::bayou::Bayou;
use crate::cryptoblob::{self, gen_key, open_deserialize, seal_serialize, Key};
use crate::login::Credentials;
use crate::mail::uidindex::*;
use crate::mail::unique_ident::*;
use crate::mail::IMF;
use crate::storage::{self, BlobRef, BlobVal, RowRef, RowVal, Selector, Store};
use crate::timestamp::now_msec;

pub struct Mailbox {
    pub(super) id: UniqueIdent,
    mbox: RwLock<MailboxInternal>,
}

impl Mailbox {
    pub(super) async fn open(
        creds: &Credentials,
        id: UniqueIdent,
        min_uidvalidity: ImapUidvalidity,
    ) -> Result<Self> {
        let index_path = format!("index/{}", id);
        let mail_path = format!("mail/{}", id);

        let mut uid_index = Bayou::<UidIndex>::new(creds, index_path).await?;
        uid_index.sync().await?;

        let uidvalidity = uid_index.state().uidvalidity;
        if uidvalidity < min_uidvalidity {
            uid_index
                .push(
                    uid_index
                        .state()
                        .op_bump_uidvalidity(min_uidvalidity.get() - uidvalidity.get()),
                )
                .await?;
        }

        dump(&uid_index);

        let mbox = RwLock::new(MailboxInternal {
            id,
            encryption_key: creds.keys.master.clone(),
            storage: creds.storage.build().await?,
            uid_index,
            mail_path,
        });

        Ok(Self { id, mbox })
    }

    /// Sync data with backing store
    pub async fn force_sync(&self) -> Result<()> {
        self.mbox.write().await.force_sync().await
    }

    /// Sync data with backing store only if changes are detected
    /// or last sync is too old
    pub async fn opportunistic_sync(&self) -> Result<()> {
        self.mbox.write().await.opportunistic_sync().await
    }

    // ---- Functions for reading the mailbox ----

    /// Get a clone of the current UID Index of this mailbox
    /// (cloning is cheap so don't hesitate to use this)
    pub async fn current_uid_index(&self) -> UidIndex {
        self.mbox.read().await.uid_index.state().clone()
    }

    /// Fetch the metadata (headers + some more info) of the specified
    /// mail IDs
    pub async fn fetch_meta(&self, ids: &[UniqueIdent]) -> Result<Vec<MailMeta>> {
        self.mbox.read().await.fetch_meta(ids).await
    }

    /// Fetch an entire e-mail
    pub async fn fetch_full(&self, id: UniqueIdent, message_key: &Key) -> Result<Vec<u8>> {
        self.mbox.read().await.fetch_full(id, message_key).await
    }

    async fn frozen(self: &std::sync::Arc<Self>) -> super::snapshot::FrozenMailbox {
        super::snapshot::FrozenMailbox::new(self.clone()).await
    }

    // ---- Functions for changing the mailbox ----

    /// Add flags to message
    pub async fn add_flags<'a>(&self, id: UniqueIdent, flags: &[Flag]) -> Result<()> {
        self.mbox.write().await.add_flags(id, flags).await
    }

    /// Delete flags from message
    pub async fn del_flags<'a>(&self, id: UniqueIdent, flags: &[Flag]) -> Result<()> {
        self.mbox.write().await.del_flags(id, flags).await
    }

    /// Define the new flags for this message
    pub async fn set_flags<'a>(&self, id: UniqueIdent, flags: &[Flag]) -> Result<()> {
        self.mbox.write().await.set_flags(id, flags).await
    }

    /// Insert an email into the mailbox
    pub async fn append<'a>(
        &self,
        msg: IMF<'a>,
        ident: Option<UniqueIdent>,
        flags: &[Flag],
    ) -> Result<(ImapUidvalidity, ImapUid)> {
        self.mbox.write().await.append(msg, ident, flags).await
    }

    /// Insert an email into the mailbox, copying it from an existing S3 object
    pub async fn append_from_s3<'a>(
        &self,
        msg: IMF<'a>,
        ident: UniqueIdent,
        blob_ref: storage::BlobRef,
        message_key: Key,
    ) -> Result<()> {
        self.mbox
            .write()
            .await
            .append_from_s3(msg, ident, blob_ref, message_key)
            .await
    }

    /// Delete a message definitively from the mailbox
    pub async fn delete<'a>(&self, id: UniqueIdent) -> Result<()> {
        self.mbox.write().await.delete(id).await
    }

    /// Copy an email from an other Mailbox to this mailbox
    /// (use this when possible, as it allows for a certain number of storage optimizations)
    pub async fn copy_from(&self, from: &Mailbox, uuid: UniqueIdent) -> Result<UniqueIdent> {
        if self.id == from.id {
            bail!("Cannot copy into same mailbox");
        }

        let (mut selflock, fromlock);
        if self.id < from.id {
            selflock = self.mbox.write().await;
            fromlock = from.mbox.write().await;
        } else {
            fromlock = from.mbox.write().await;
            selflock = self.mbox.write().await;
        };
        selflock.copy_from(&fromlock, uuid).await
    }

    /// Move an email from an other Mailbox to this mailbox
    /// (use this when possible, as it allows for a certain number of storage optimizations)
    pub async fn move_from(&self, from: &Mailbox, uuid: UniqueIdent) -> Result<()> {
        if self.id == from.id {
            bail!("Cannot copy move same mailbox");
        }

        let (mut selflock, mut fromlock);
        if self.id < from.id {
            selflock = self.mbox.write().await;
            fromlock = from.mbox.write().await;
        } else {
            fromlock = from.mbox.write().await;
            selflock = self.mbox.write().await;
        };
        selflock.move_from(&mut fromlock, uuid).await
    }
}

// ----

// Non standard but common flags:
// https://www.iana.org/assignments/imap-jmap-keywords/imap-jmap-keywords.xhtml
struct MailboxInternal {
    // 2023-05-15 will probably be used later.
    #[allow(dead_code)]
    id: UniqueIdent,
    mail_path: String,
    encryption_key: Key,
    storage: Store,
    uid_index: Bayou<UidIndex>,
}

impl MailboxInternal {
    async fn force_sync(&mut self) -> Result<()> {
        self.uid_index.sync().await?;
        Ok(())
    }

    async fn opportunistic_sync(&mut self) -> Result<()> {
        self.uid_index.opportunistic_sync().await?;
        Ok(())
    }

    // ---- Functions for reading the mailbox ----

    async fn fetch_meta(&self, ids: &[UniqueIdent]) -> Result<Vec<MailMeta>> {
        let ids = ids.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let ops = ids
            .iter()
            .map(|id| RowRef::new(self.mail_path.as_str(), id.as_str()))
            .collect::<Vec<_>>();
        let res_vec = self.storage.row_fetch(&Selector::List(ops)).await?;

        let mut meta_vec = vec![];
        for res in res_vec.into_iter() {
            let mut meta_opt = None;

            // Resolve conflicts
            for v in res.value.iter() {
                match v {
                    storage::Alternative::Tombstone => (),
                    storage::Alternative::Value(v) => {
                        let meta = open_deserialize::<MailMeta>(v, &self.encryption_key)?;
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
                meta_vec.push(meta);
            } else {
                bail!("No valid meta value in k2v for {:?}", res.row_ref);
            }
        }

        Ok(meta_vec)
    }

    async fn fetch_full(&self, id: UniqueIdent, message_key: &Key) -> Result<Vec<u8>> {
        let obj_res = self
            .storage
            .blob_fetch(&BlobRef(format!("{}/{}", self.mail_path, id)))
            .await?;
        let body = obj_res.value;
        cryptoblob::open(&body, message_key)
    }

    // ---- Functions for changing the mailbox ----

    async fn add_flags(&mut self, ident: UniqueIdent, flags: &[Flag]) -> Result<()> {
        let add_flag_op = self.uid_index.state().op_flag_add(ident, flags.to_vec());
        self.uid_index.push(add_flag_op).await
    }

    async fn del_flags(&mut self, ident: UniqueIdent, flags: &[Flag]) -> Result<()> {
        let del_flag_op = self.uid_index.state().op_flag_del(ident, flags.to_vec());
        self.uid_index.push(del_flag_op).await
    }

    async fn set_flags(&mut self, ident: UniqueIdent, flags: &[Flag]) -> Result<()> {
        let set_flag_op = self.uid_index.state().op_flag_set(ident, flags.to_vec());
        self.uid_index.push(set_flag_op).await
    }

    async fn append(
        &mut self,
        mail: IMF<'_>,
        ident: Option<UniqueIdent>,
        flags: &[Flag],
    ) -> Result<(ImapUidvalidity, ImapUid)> {
        let ident = ident.unwrap_or_else(gen_ident);
        let message_key = gen_key();

        futures::try_join!(
            async {
                // Encrypt and save mail body
                let message_blob = cryptoblob::seal(mail.raw, &message_key)?;
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
                    headers: mail.parsed.raw_headers.to_vec(),
                    message_key: message_key.clone(),
                    rfc822_size: mail.raw.len(),
                };
                let meta_blob = seal_serialize(&meta, &self.encryption_key)?;
                self.storage
                    .row_insert(vec![RowVal::new(
                        RowRef::new(&self.mail_path, &ident.to_string()),
                        meta_blob,
                    )])
                    .await?;
                Ok::<_, anyhow::Error>(())
            },
            self.uid_index.opportunistic_sync()
        )?;

        // Add mail to Bayou mail index
        let uid_state = self.uid_index.state();
        let add_mail_op = uid_state.op_mail_add(ident, flags.to_vec());

        let uidvalidity = uid_state.uidvalidity;
        let uid = match add_mail_op {
            UidIndexOp::MailAdd(_, uid, _) => uid,
            _ => unreachable!(),
        };

        self.uid_index.push(add_mail_op).await?;

        Ok((uidvalidity, uid))
    }

    async fn append_from_s3<'a>(
        &mut self,
        mail: IMF<'a>,
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
                    headers: mail.parsed.raw_headers.to_vec(),
                    message_key: message_key.clone(),
                    rfc822_size: mail.raw.len(),
                };
                let meta_blob = seal_serialize(&meta, &self.encryption_key)?;
                self.storage
                    .row_insert(vec![RowVal::new(
                        RowRef::new(&self.mail_path, &ident.to_string()),
                        meta_blob,
                    )])
                    .await?;
                Ok::<_, anyhow::Error>(())
            },
            self.uid_index.opportunistic_sync()
        )?;

        // Add mail to Bayou mail index
        let add_mail_op = self.uid_index.state().op_mail_add(ident, vec![]);
        self.uid_index.push(add_mail_op).await?;

        Ok(())
    }

    async fn delete(&mut self, ident: UniqueIdent) -> Result<()> {
        if !self.uid_index.state().table.contains_key(&ident) {
            bail!("Cannot delete mail that doesn't exit");
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
                let res = self
                    .storage
                    .row_fetch(&storage::Selector::Single(&RowRef::new(
                        &self.mail_path,
                        &sk,
                    )))
                    .await?;
                if let Some(row_val) = res.into_iter().next() {
                    self.storage
                        .row_rm(&storage::Selector::Single(&row_val.row_ref))
                        .await?;
                }
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

    async fn move_from(&mut self, from: &mut MailboxInternal, id: UniqueIdent) -> Result<()> {
        self.copy_internal(from, id, id).await?;
        from.delete(id).await?;
        Ok(())
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
            .1
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
                let meta = &from.fetch_meta(&[source_id]).await?[0];
                let meta_blob = seal_serialize(meta, &self.encryption_key)?;
                self.storage
                    .row_insert(vec![RowVal::new(
                        RowRef::new(&self.mail_path, &new_id.to_string()),
                        meta_blob,
                    )])
                    .await?;
                Ok::<_, anyhow::Error>(())
            },
            self.uid_index.opportunistic_sync(),
        )?;

        // Add mail to Bayou mail index
        let add_mail_op = self.uid_index.state().op_mail_add(new_id, flags);
        self.uid_index.push(add_mail_op).await?;

        Ok(())
    }
}

fn dump(uid_index: &Bayou<UidIndex>) {
    let s = uid_index.state();
    println!("---- MAILBOX STATE ----");
    println!("UIDVALIDITY {}", s.uidvalidity);
    println!("UIDNEXT {}", s.uidnext);
    println!("INTERNALSEQ {}", s.internalseq);
    for (uid, ident) in s.idx_by_uid.iter() {
        println!(
            "{} {} {}",
            uid,
            hex::encode(ident.0),
            s.table.get(ident).cloned().unwrap().1.join(", ")
        );
    }
    println!();
}

// ----

/// The metadata of a message that is stored in K2V
/// at pk = mail/<mailbox uuid>, sk = <message uuid>
#[derive(Serialize, Deserialize)]
pub struct MailMeta {
    /// INTERNALDATE field (milliseconds since epoch)
    pub internaldate: u64,
    /// Headers of the message
    pub headers: Vec<u8>,
    /// Secret key for decrypting entire message
    pub message_key: Key,
    /// RFC822 size
    pub rfc822_size: usize,
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
