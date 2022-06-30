use std::collections::HashMap;
use std::sync::{Arc, Weak};
use std::time::Duration;

use anyhow::Result;
use k2v_client::{CausalityToken, K2vClient, K2vValue};
use rusoto_s3::{PutObjectRequest, S3Client, S3};
use tokio::sync::watch;
use tracing::error;

use crate::cryptoblob;
use crate::login::{Credentials, PublicCredentials};
use crate::mail::mailbox::Mailbox;
use crate::mail::uidindex::ImapUidvalidity;
use crate::mail::unique_ident::*;
use crate::mail::user::User;
use crate::time::now_msec;

const INCOMING_PK: &str = "incoming";
const INCOMING_LOCK_SK: &str = "lock";
const INCOMING_WATCH_SK: &str = "watch";

pub async fn incoming_mail_watch_process(
    user: Weak<User>,
    creds: Credentials,
    rx_inbox_id: watch::Receiver<Option<(UniqueIdent, ImapUidvalidity)>>,
) {
    if let Err(e) = incoming_mail_watch_process_internal(user, creds, rx_inbox_id).await {
        error!("Error in incoming mail watch process: {}", e);
    }
}

async fn incoming_mail_watch_process_internal(
    user: Weak<User>,
    creds: Credentials,
    mut rx_inbox_id: watch::Receiver<Option<(UniqueIdent, ImapUidvalidity)>>,
) -> Result<()> {
    let mut lock_held = k2v_lock_loop(creds.k2v_client()?, INCOMING_PK, INCOMING_LOCK_SK);

    let k2v = creds.k2v_client()?;
    let s3 = creds.s3_client()?;

    let mut inbox: Option<Arc<Mailbox>> = None;
    let mut prev_ct: Option<CausalityToken> = None;

    loop {
        let new_mail = if *lock_held.borrow() {
            tokio::select! {
                ct = wait_new_mail(&k2v, &prev_ct) => Some(ct),
                _ = tokio::time::sleep(Duration::from_secs(300)) => prev_ct.take(),
                _ = lock_held.changed() => None,
                _ = rx_inbox_id.changed() => None,
            }
        } else {
            tokio::select! {
                _ = lock_held.changed() => None,
                _ = rx_inbox_id.changed() => None,
            }
        };

        if let Some(user) = Weak::upgrade(&user) {
            eprintln!("User still available");

            // If INBOX no longer is same mailbox, open new mailbox
            let inbox_id = rx_inbox_id.borrow().clone();
            if let Some((id, uidvalidity)) = inbox_id {
                if Some(id) != inbox.as_ref().map(|b| b.id) {
                    match user.open_mailbox_by_id(id, uidvalidity).await {
                        Ok(mb) => {
                            inbox = mb;
                        }
                        Err(e) => {
                            inbox = None;
                            error!("Error when opening inbox ({}): {}", id, e);
                            tokio::time::sleep(Duration::from_secs(30)).await;
                        }
                    }
                }
            }

            // If we were able to open INBOX, and we have mail (implies lock is held),
            // fetch new mail
            if let (Some(inbox), Some(new_ct)) = (&inbox, new_mail) {
                match handle_incoming_mail(&user, &s3, inbox).await {
                    Ok(()) => {
                        prev_ct = Some(new_ct);
                    }
                    Err(e) => {
                        error!("Could not fetch incoming mail: {}", e);
                        tokio::time::sleep(Duration::from_secs(30)).await;
                    }
                }
            }
        } else {
            eprintln!("User no longer available, exiting incoming loop.");
            break;
        }
    }
    drop(rx_inbox_id);
    Ok(())
}

async fn wait_new_mail(k2v: &K2vClient, prev_ct: &Option<CausalityToken>) -> CausalityToken {
    loop {
        if let Some(ct) = &prev_ct {
            match k2v
                .poll_item(INCOMING_PK, INCOMING_WATCH_SK, ct.clone(), None)
                .await
            {
                Err(e) => {
                    error!("Error when waiting for incoming watch: {}, sleeping", e);
                    tokio::time::sleep(Duration::from_secs(30)).await;
                }
                Ok(None) => continue,
                Ok(Some(cv)) => {
                    return cv.causality;
                }
            }
        } else {
            match k2v.read_item(INCOMING_PK, INCOMING_WATCH_SK).await {
                Err(k2v_client::Error::NotFound) => {
                    if let Err(e) = k2v
                        .insert_item(INCOMING_PK, INCOMING_WATCH_SK, vec![0u8], None)
                        .await
                    {
                        error!("Error when waiting for incoming watch: {}, sleeping", e);
                        tokio::time::sleep(Duration::from_secs(30)).await;
                    }
                }
                Err(e) => {
                    error!("Error when waiting for incoming watch: {}, sleeping", e);
                    tokio::time::sleep(Duration::from_secs(30)).await;
                }
                Ok(cv) => {
                    return cv.causality;
                }
            }
        }
    }
}

async fn handle_incoming_mail(user: &Arc<User>, s3: &S3Client, inbox: &Arc<Mailbox>) -> Result<()> {
    unimplemented!()
}

fn k2v_lock_loop(k2v: K2vClient, pk: &'static str, sk: &'static str) -> watch::Receiver<bool> {
    let (held_tx, held_rx) = watch::channel(false);

    tokio::spawn(async move {
        if let Err(e) = k2v_lock_loop_internal(k2v, pk, sk, &held_tx).await {
            error!("Error in k2v locking loop: {}", e);
        }
        let _ = held_tx.send(false);
    });

    held_rx
}

async fn k2v_lock_loop_internal(
    k2v: K2vClient,
    pk: &'static str,
    sk: &'static str,
    held_tx: &watch::Sender<bool>,
) -> Result<()> {
    unimplemented!()
}

// ----

pub struct EncryptedMessage {
    key: cryptoblob::Key,
    encrypted_body: Vec<u8>,
}

impl EncryptedMessage {
    pub fn new(body: Vec<u8>) -> Result<Self> {
        let key = cryptoblob::gen_key();
        let encrypted_body = cryptoblob::seal(&body, &key)?;
        Ok(Self {
            key,
            encrypted_body,
        })
    }

    pub async fn deliver_to(self: Arc<Self>, creds: PublicCredentials) -> Result<()> {
        let s3_client = creds.storage.s3_client()?;
        let k2v_client = creds.storage.k2v_client()?;

        // Get causality token of previous watch key
        let watch_ct = match k2v_client.read_item(INCOMING_PK, INCOMING_WATCH_SK).await {
            Err(_) => None,
            Ok(cv) => Some(cv.causality),
        };

        // Write mail to encrypted storage
        let encrypted_key =
            sodiumoxide::crypto::sealedbox::seal(self.key.as_ref(), &creds.public_key);
        let key_header = base64::encode(&encrypted_key);

        let mut por = PutObjectRequest::default();
        por.bucket = creds.storage.bucket.clone();
        por.key = format!("incoming/{}", gen_ident().to_string());
        por.metadata = Some(
            [("Message-Key".to_string(), key_header)]
                .into_iter()
                .collect::<HashMap<_, _>>(),
        );
        por.body = Some(self.encrypted_body.clone().into());
        s3_client.put_object(por).await?;

        // Update watch key to signal new mail
        k2v_client
            .insert_item(
                INCOMING_PK,
                INCOMING_WATCH_SK,
                gen_ident().0.to_vec(),
                watch_ct,
            )
            .await?;

        Ok(())
    }
}
