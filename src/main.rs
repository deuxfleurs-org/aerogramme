use anyhow::Result;

use rusoto_credential::{EnvironmentProvider, ProvideAwsCredentials};
use rusoto_signature::Region;

mod bayou;
mod cryptoblob;
mod time;
mod uidindex;

use bayou::Bayou;
use cryptoblob::Key;
use uidindex::{UidIndex, UidIndexOp};

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

    let mut mail_index = Bayou::<UidIndex>::new(
        creds,
        k2v_region,
        s3_region,
        "alex".into(),
        "TestMailbox".into(),
        key,
    )?;

    mail_index.sync().await?;

    Ok(())
}
