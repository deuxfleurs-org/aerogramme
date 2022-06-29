use anyhow::{anyhow, bail, Result};
use k2v_client::K2vClient;
use k2v_client::{BatchReadOp, Filter, K2vValue};
use rusoto_s3::{
    DeleteObjectRequest, GetObjectRequest, ListObjectsV2Request, PutObjectRequest, S3Client, S3,
};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;
use tokio::sync::RwLock;

use crate::bayou::Bayou;
use crate::cryptoblob::{self, gen_key, open_deserialize, seal_serialize, Key};
use crate::login::Credentials;
use crate::mail::uidindex::*;
use crate::mail::unique_ident::*;
use crate::mail::IMF;
use crate::time::now_msec;

pub struct Mailbox {
    id: UniqueIdent,
    mbox: RwLock<MailboxInternal>,
}

impl Mailbox {
    pub(super) async fn open(creds: &Credentials, id: UniqueIdent) -> Result<Self> {
        let index_path = format!("index/{}", id);
        let mail_path = format!("mail/{}", id);

        let mut uid_index = Bayou::<UidIndex>::new(creds, index_path)?;
        uid_index.sync().await?;

        let mbox = RwLock::new(MailboxInternal {
            id,
            bucket: creds.bucket().to_string(),
            encryption_key: creds.keys.master.clone(),
            k2v: creds.k2v_client()?,
            s3: creds.s3_client()?,
            uid_index,
            mail_path,
        });

        Ok(Self { id, mbox })
    }

    /// Sync data with backing store
    pub async fn sync(&self) -> Result<()> {
        self.mbox.write().await.uid_index.sync().await
    }

    /// Get a clone of the current UID Index of this mailbox
    /// (cloning is cheap so don't hesitate to use this)
    pub async fn current_uid_index(&self) -> UidIndex {
        self.mbox.read().await.uid_index.state().clone()
    }

    /// Insert an email in the mailbox
    pub async fn append<'a>(&self, msg: IMF<'a>) -> Result<()> {
        self.mbox.write().await.append(msg, None).await
    }

    /// Copy an email from an other Mailbox to this mailbox
    /// (use this when possible, as it allows for a certain number of storage optimizations)
    pub async fn copy(&self, _from: &Mailbox, _uid: ImapUid) -> Result<()> {
        unimplemented!()
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

    /// Test procedure TODO WILL REMOVE THIS
    pub async fn test(&self) -> Result<()> {
        self.mbox.write().await.test().await
    }
}

// ----

// Non standard but common flags:
// https://www.iana.org/assignments/imap-jmap-keywords/imap-jmap-keywords.xhtml
struct MailboxInternal {
    id: UniqueIdent,
    bucket: String,
    mail_path: String,
    encryption_key: Key,

    k2v: K2vClient,
    s3: S3Client,

    uid_index: Bayou<UidIndex>,
}

impl MailboxInternal {
    async fn fetch_meta(&self, ids: &[UniqueIdent]) -> Result<Vec<MailMeta>> {
        let ids = ids.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let ops = ids
            .iter()
            .map(|id| BatchReadOp {
                partition_key: &self.mail_path,
                filter: Filter {
                    start: Some(id),
                    end: None,
                    prefix: None,
                    limit: None,
                    reverse: false,
                },
                single_item: true,
                conflicts_only: false,
                tombstones: false,
            })
            .collect::<Vec<_>>();
        let res_vec = self.k2v.read_batch(&ops).await?;

        let mut meta_vec = vec![];
        for res in res_vec {
            if res.items.len() != 1 {
                bail!("Expected 1 item, got {}", res.items.len());
            }
            let (_, cv) = res.items.iter().next().unwrap();
            if cv.value.len() != 1 {
                bail!("Expected 1 value, got {}", cv.value.len());
            }
            match &cv.value[0] {
                K2vValue::Tombstone => bail!("Expected value, got tombstone"),
                K2vValue::Value(v) => {
                    let meta = open_deserialize::<MailMeta>(v, &self.encryption_key)?;
                    meta_vec.push(meta);
                }
            }
        }

        Ok(meta_vec)
    }

    async fn fetch_full(&self, id: UniqueIdent, message_key: &Key) -> Result<Vec<u8>> {
        let mut gor = GetObjectRequest::default();
        gor.bucket = self.bucket.clone();
        gor.key = format!("{}/{}", self.mail_path, id);

        let obj_res = self.s3.get_object(gor).await?;

        let obj_body = obj_res.body.ok_or(anyhow!("Missing object body"))?;
        let mut buf = Vec::with_capacity(obj_res.content_length.unwrap_or(128) as usize);
        obj_body.into_async_read().read_to_end(&mut buf).await?;

        Ok(cryptoblob::open(&buf, &message_key)?)
    }

    async fn append(&mut self, mail: IMF<'_>, ident: Option<UniqueIdent>) -> Result<()> {
        let ident = ident.unwrap_or_else(|| gen_ident());
        let message_key = gen_key();

        futures::try_join!(
            async {
                // Encrypt and save mail body
                let message_blob = cryptoblob::seal(mail.raw, &message_key)?;
                let mut por = PutObjectRequest::default();
                por.bucket = self.bucket.clone();
                por.key = format!("{}/{}", self.mail_path, ident);
                por.body = Some(message_blob.into());
                self.s3.put_object(por).await?;
                Ok::<_, anyhow::Error>(())
            },
            async {
                // Save mail meta
                let meta = MailMeta {
                    internaldate: now_msec(),
                    headers: mail.raw[..mail.parsed.offset_body].to_vec(),
                    message_key: message_key.clone(),
                    rfc822_size: mail.raw.len(),
                };
                let meta_blob = cryptoblob::seal_serialize(&meta, &self.encryption_key)?;
                self.k2v
                    .insert_item(&self.mail_path, &ident.to_string(), meta_blob, None)
                    .await?;
                Ok::<_, anyhow::Error>(())
            }
        )?;

        // Add mail to Bayou mail index
        let add_mail_op = self
            .uid_index
            .state()
            .op_mail_add(ident, vec!["\\Unseen".into()]);
        self.uid_index.push(add_mail_op).await?;

        Ok(())
    }

    async fn delete(&mut self, ident: UniqueIdent) -> Result<()> {
        let del_mail_op = self.uid_index.state().op_mail_del(ident);
        self.uid_index.push(del_mail_op).await?;

        futures::try_join!(
            async {
                // Delete mail body from S3
                let mut dor = DeleteObjectRequest::default();
                dor.bucket = self.bucket.clone();
                dor.key = format!("{}/{}", self.mail_path, ident);
                self.s3.delete_object(dor).await?;
                Ok::<_, anyhow::Error>(())
            },
            async {
                // Delete mail meta from K2V
                let sk = ident.to_string();
                let v = self.k2v.read_item(&self.mail_path, &sk).await?;
                self.k2v
                    .delete_item(&self.mail_path, &sk, v.causality)
                    .await?;
                Ok::<_, anyhow::Error>(())
            }
        )?;
        Ok(())
    }

    // ----

    async fn test(&mut self) -> Result<()> {
        self.uid_index.sync().await?;

        dump(&self.uid_index);

        let mail = br#"From: Garage team <garagehq@deuxfleurs.fr>
Subject: Welcome to Aerogramme!!

This is just a test email, feel free to ignore."#;
        let mail = IMF::try_from(&mail[..]).unwrap();
        self.append(mail, None).await?;

        dump(&self.uid_index);

        if self.uid_index.state().idx_by_uid.len() > 6 {
            for i in 0..2 {
                let (_, ident) = self
                    .uid_index
                    .state()
                    .idx_by_uid
                    .iter()
                    .skip(3 + i)
                    .next()
                    .unwrap();

                self.delete(*ident).await?;

                dump(&self.uid_index);
            }
        }

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
    println!("");
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
