use anyhow::Result;
use k2v_client::K2vClient;
use rand::prelude::*;
use rusoto_s3::S3Client;
use rusoto_signature::Region;

use crate::bayou::Bayou;
use crate::cryptoblob::Key;
use crate::login::Credentials;
use crate::uidindex::*;

pub struct Mailbox {
    bucket: String,
    name: String,
    key: Key,

    k2v: K2vClient,
    s3: S3Client,

    uid_index: Bayou<UidIndex>,
}

impl Mailbox {
    pub async fn new(
        k2v_region: &Region,
        s3_region: &Region,
        creds: &Credentials,
        name: String,
    ) -> Result<Self> {
        let uid_index = Bayou::<UidIndex>::new(k2v_region, s3_region, creds, name.clone())?;

        Ok(Self {
            bucket: creds.bucket.clone(),
            name,
            key: creds.master_key.clone(),
            k2v: creds.k2v_client(&k2v_region)?,
            s3: creds.s3_client(&s3_region)?,
            uid_index,
        })
    }

    pub async fn test(&mut self) -> Result<()> {
        self.uid_index.sync().await?;

        dump(&self.uid_index);

        let mut rand_id = [0u8; 24];
        rand_id[..16].copy_from_slice(&u128::to_be_bytes(thread_rng().gen()));
        let add_mail_op = self
            .uid_index
            .state()
            .op_mail_add(MailUuid(rand_id), vec!["\\Unseen".into()]);
        self.uid_index.push(add_mail_op).await?;

        dump(&self.uid_index);

        if self.uid_index.state().mails_by_uid.len() > 6 {
            for i in 0..2 {
                let (_, uuid) = self
                    .uid_index
                    .state()
                    .mails_by_uid
                    .iter()
                    .skip(3 + i)
                    .next()
                    .unwrap();
                let del_mail_op = self.uid_index.state().op_mail_del(*uuid);
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
    for (uid, uuid) in s.mails_by_uid.iter() {
        println!(
            "{} {} {}",
            uid,
            hex::encode(uuid.0),
            s.mail_flags
                .get(uuid)
                .cloned()
                .unwrap_or_default()
                .join(", ")
        );
    }
    println!("");
}
