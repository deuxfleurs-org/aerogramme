use std::sync::{Arc, Weak};
use std::time::Duration;

use tokio::sync::watch;

use crate::mail::unique_ident::UniqueIdent;
use crate::mail::user::User;
use crate::mail::uidindex::ImapUidvalidity;

pub async fn incoming_mail_watch_process(user: Weak<User>, rx_inbox_id: watch::Receiver<Option<(UniqueIdent, ImapUidvalidity)>>) {
    while Weak::upgrade(&user).is_some() {
        eprintln!("User still available");
        tokio::time::sleep(Duration::from_secs(10)).await;
    }
    drop(rx_inbox_id);
}
