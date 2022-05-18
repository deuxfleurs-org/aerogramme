mod bayou;
mod cryptoblob;
mod time;
mod uidindex;

use anyhow::Result;

use rand::prelude::*;
use rusoto_credential::{EnvironmentProvider, ProvideAwsCredentials};
use rusoto_signature::Region;

use bayou::*;
use cryptoblob::Key;
use uidindex::*;

#[tokio::main]
async fn main() {
    do_stuff().await.expect("Something failed");
}

async fn do_stuff() -> Result<()> {
    let creds = EnvironmentProvider::default().credentials().await.unwrap();

    let k2v_region = Region::Custom {
        name: "garage-staging".to_owned(),
        endpoint: "https://k2v-staging.home.adnab.me".to_owned(),
    };

    let s3_region = Region::Custom {
        name: "garage-staging".to_owned(),
        endpoint: "https://garage-staging.home.adnab.me".to_owned(),
    };

    let key = Key::from_slice(&[0u8; 32]).unwrap();

    let mut uid_index = Bayou::<UidIndex>::new(
        creds,
        k2v_region,
        s3_region,
        "mail".into(),
        "TestMailbox".into(),
        key,
    )?;

    uid_index.sync().await?;

    dump(&uid_index);

    let mut rand_id = [0u8; 24];
    rand_id[..8].copy_from_slice(&u64::to_be_bytes(thread_rng().gen()));
    let add_mail_op = uid_index
        .state()
        .op_mail_add(MailUuid(rand_id), vec!["\\Unseen".into()]);
    uid_index.push(add_mail_op).await?;

    dump(&uid_index);

    Ok(())
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
