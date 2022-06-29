use std::convert::TryFrom;

use anyhow::Result;
use k2v_client::K2vClient;
use rusoto_s3::S3Client;

use crate::bayou::Bayou;
use crate::cryptoblob::Key;
use crate::login::Credentials;
use crate::mail::mail_ident::*;
use crate::mail::uidindex::*;
use crate::mail::IMF;

pub struct Summary<'a> {
    pub validity: ImapUidvalidity,
    pub next: ImapUid,
    pub exists: u32,
    pub recent: u32,
    pub flags: FlagIter<'a>,
    pub unseen: Option<&'a ImapUid>,
}
impl std::fmt::Display for Summary<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "uidvalidity: {}, uidnext: {}, exists: {}",
            self.validity, self.next, self.exists
        )
    }
}

// Non standard but common flags:
// https://www.iana.org/assignments/imap-jmap-keywords/imap-jmap-keywords.xhtml
pub struct Mailbox {
    bucket: String,
    pub name: String,
    key: Key,

    k2v: K2vClient,
    s3: S3Client,

    uid_index: Bayou<UidIndex>,
    mail_path: String,
}

impl Mailbox {
    pub(super) fn new(creds: &Credentials, name: String) -> Result<Self> {
        let index_path = format!("index/{}", name);
        let mail_path = format!("mail/{}", name);
        let uid_index = Bayou::<UidIndex>::new(creds, index_path)?;

        Ok(Self {
            bucket: creds.bucket().to_string(),
            name,
            key: creds.keys.master.clone(),
            k2v: creds.k2v_client()?,
            s3: creds.s3_client()?,
            uid_index,
            mail_path,
        })
    }

    // Get a summary of the mailbox, useful for the SELECT command for example
    pub async fn summary(&mut self) -> Result<Summary> {
        self.uid_index.sync().await?;
        let state = self.uid_index.state();

        let unseen = state
            .idx_by_flag
            .get(&"$unseen".to_string())
            .and_then(|os| os.get_min());
        let recent = state
            .idx_by_flag
            .get(&"\\Recent".to_string())
            .map(|os| os.len())
            .unwrap_or(0);

        return Ok(Summary {
            validity: state.uidvalidity,
            next: state.uidnext,
            exists: u32::try_from(state.idx_by_uid.len())?,
            recent: u32::try_from(recent)?,
            flags: state.idx_by_flag.flags(),
            unseen,
        });
    }

    // Insert an email in the mailbox
    pub async fn append(&mut self, _msg: IMF) -> Result<()> {
        Ok(())
    }

    // Copy an email from an external to this mailbox
    // @FIXME is it needed or could we implement it with append?
    pub async fn copy(&mut self, _mailbox: String, _uid: ImapUid) -> Result<()> {
        Ok(())
    }

    // Delete all emails with the \Delete flag in the mailbox
    // Can be called by CLOSE and EXPUNGE
    // @FIXME do we want to implement this feature or a simpler "delete" command
    // The controller could then "fetch \Delete" and call delete on each email?
    pub async fn expunge(&mut self) -> Result<()> {
        Ok(())
    }

    // Update flags of a range of emails
    pub async fn store(&mut self) -> Result<()> {
        Ok(())
    }

    pub async fn fetch(&mut self) -> Result<()> {
        Ok(())
    }

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
