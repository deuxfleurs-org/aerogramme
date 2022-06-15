use anyhow::Result;
use k2v_client::K2vClient;
use rusoto_s3::S3Client;

use crate::bayou::Bayou;
use crate::cryptoblob::Key;
use crate::login::Credentials;
use crate::mail_ident::*;
use crate::uidindex::*;

pub struct Summary {
    pub validity: ImapUidvalidity,
    pub next: ImapUid,
    pub exists: usize,
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

pub struct Mailbox {
    bucket: String,
    pub name: String,
    key: Key,

    k2v: K2vClient,
    s3: S3Client,

    uid_index: Bayou<UidIndex>,
}

impl Mailbox {
    pub fn new(creds: &Credentials, name: String) -> Result<Self> {
        let uid_index = Bayou::<UidIndex>::new(creds, name.clone())?;

        Ok(Self {
            bucket: creds.bucket().to_string(),
            name,
            key: creds.keys.master.clone(),
            k2v: creds.k2v_client()?,
            s3: creds.s3_client()?,
            uid_index,
        })
    }

    pub async fn summary(&mut self) -> Result<Summary> {
        self.uid_index.sync().await?;
        let state = self.uid_index.state();

        return Ok(Summary {
            validity: state.uidvalidity,
            next: state.uidnext,
            exists: state.idx_by_uid.len(),
        });
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
            s.table.get(ident).cloned().unwrap_or_default().1.join(", ")
        );
    }
    println!("");
}
