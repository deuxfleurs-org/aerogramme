use std::convert::TryFrom;

use anyhow::Result;
use k2v_client::K2vClient;
use rusoto_s3::S3Client;
use tokio::sync::RwLock;

use crate::bayou::Bayou;
use crate::cryptoblob::Key;
use crate::login::Credentials;
use crate::mail::mail_ident::*;
use crate::mail::uidindex::*;
use crate::mail::IMF;

pub struct Summary {
    pub validity: ImapUidvalidity,
    pub next: ImapUid,
    pub exists: u32,
    pub recent: u32,
    pub flags: Vec<String>,
    pub unseen: Option<ImapUid>,
}

impl std::fmt::Display for Summary {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "uidvalidity: {}, uidnext: {}, exists: {}",
            self.validity, self.next, self.exists
        )
    }
}

pub struct Mailbox(RwLock<MailboxInternal>);

impl Mailbox {
    pub(super) async fn open(creds: &Credentials, name: &str) -> Result<Self> {
        let index_path = format!("index/{}", name);
        let mail_path = format!("mail/{}", name);

        let mut uid_index = Bayou::<UidIndex>::new(creds, index_path)?;
        uid_index.sync().await?;

        Ok(Self(RwLock::new(MailboxInternal {
            bucket: creds.bucket().to_string(),
            key: creds.keys.master.clone(),
            k2v: creds.k2v_client()?,
            s3: creds.s3_client()?,
            uid_index,
            mail_path,
        })))
    }

    /// Get a clone of the current UID Index of this mailbox
    /// (cloning is cheap so don't hesitate to use this)
    pub async fn current_uid_index(&self) -> UidIndex {
        self.0.read().await.uid_index.state().clone()
    }

    /// Insert an email in the mailbox
    pub async fn append<'a>(&self, _msg: IMF<'a>) -> Result<()> {
        unimplemented!()
    }

    /// Copy an email from an other Mailbox to this mailbox
    /// (use this when possible, as it allows for a certain number of storage optimizations)
    pub async fn copy(&self, _from: &Mailbox, _uid: ImapUid) -> Result<()> {
        unimplemented!()
    }

    /// Delete all emails with the \Delete flag in the mailbox
    /// Can be called by CLOSE and EXPUNGE
    /// @FIXME do we want to implement this feature or a simpler "delete" command
    /// The controller could then "fetch \Delete" and call delete on each email?
    pub async fn expunge(&self) -> Result<()> {
        unimplemented!()
    }

    /// Update flags of a range of emails
    pub async fn store(&self) -> Result<()> {
        unimplemented!()
    }

    pub async fn fetch(&self) -> Result<()> {
        unimplemented!()
    }
}

// ----

// Non standard but common flags:
// https://www.iana.org/assignments/imap-jmap-keywords/imap-jmap-keywords.xhtml
struct MailboxInternal {
    bucket: String,
    key: Key,

    k2v: K2vClient,
    s3: S3Client,

    uid_index: Bayou<UidIndex>,
    mail_path: String,
}

impl MailboxInternal {
    pub async fn test(&mut self) -> Result<()> {
        self.uid_index.sync().await?;

        dump(&self.uid_index);

        let add_mail_op = self
            .uid_index
            .state()
            .op_mail_add(gen_ident(), vec!["\\Unseen".into()]);
        self.uid_index.push(add_mail_op).await?;

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
                let del_mail_op = self.uid_index.state().op_mail_del(*ident);
                self.uid_index.push(del_mail_op).await?;

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
