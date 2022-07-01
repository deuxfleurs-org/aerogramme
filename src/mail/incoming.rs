use std::collections::HashMap;
use std::pin::Pin;
use std::sync::{Arc, Weak};
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use futures::{future::BoxFuture, Future, FutureExt};
use k2v_client::{CausalValue, CausalityToken, K2vClient, K2vValue};
use rusoto_s3::{PutObjectRequest, S3Client, S3};
use tokio::sync::watch;
use tracing::{error, info};

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

// When a lock is held, it is held for LOCK_DURATION (here 5 minutes)
// It is renewed every LOCK_DURATION/3
// If we are at 2*LOCK_DURATION/3 and haven't renewed, we assume we
// lost the lock.
const LOCK_DURATION: Duration = Duration::from_secs(300);

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
            info!("incoming lock held");

            let wait_new_mail = async {
                loop {
                    match k2v_wait_value_changed(&k2v, &INCOMING_PK, &INCOMING_WATCH_SK, &prev_ct)
                        .await
                    {
                        Ok(cv) => break cv,
                        Err(e) => {
                            error!("Error in wait_new_mail: {}", e);
                            tokio::time::sleep(Duration::from_secs(30)).await;
                        }
                    }
                }
            };

            tokio::select! {
                cv = wait_new_mail => Some(cv.causality),
                _ = tokio::time::sleep(Duration::from_secs(300)) => prev_ct.take(),
                _ = lock_held.changed() => None,
                _ = rx_inbox_id.changed() => None,
            }
        } else {
            info!("incoming lock not held");
            tokio::select! {
                _ = lock_held.changed() => None,
                _ = rx_inbox_id.changed() => None,
            }
        };

        if let Some(user) = Weak::upgrade(&user) {
            info!("User still available");

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
            info!("User no longer available, exiting incoming loop.");
            break;
        }
    }
    drop(rx_inbox_id);
    Ok(())
}

async fn handle_incoming_mail(user: &Arc<User>, s3: &S3Client, inbox: &Arc<Mailbox>) -> Result<()> {
    unimplemented!()
}

// ---- UTIL: K2V locking loop, use this to try to grab a lock using a K2V entry as a signal ----

fn k2v_lock_loop(k2v: K2vClient, pk: &'static str, sk: &'static str) -> watch::Receiver<bool> {
    let (held_tx, held_rx) = watch::channel(false);

    tokio::spawn(k2v_lock_loop_internal(k2v, pk, sk, held_tx));

    held_rx
}

#[derive(Clone, Debug)]
enum LockState {
    Unknown,
    Empty,
    Held(UniqueIdent, u64, CausalityToken),
}

async fn k2v_lock_loop_internal(
    k2v: K2vClient,
    pk: &'static str,
    sk: &'static str,
    held_tx: watch::Sender<bool>,
) {
    let (state_tx, mut state_rx) = watch::channel::<LockState>(LockState::Unknown);
    let mut state_rx_2 = state_rx.clone();

    let our_pid = gen_ident();

    // Loop 1: watch state of lock in K2V, save that in corresponding watch channel
    let watch_lock_loop: BoxFuture<Result<()>> = async {
        let mut ct = None;
        loop {
            match k2v_wait_value_changed(&k2v, pk, sk, &ct).await {
                Err(e) => {
                    error!(
                        "Error in k2v wait value changed: {} ; assuming we no longer hold lock.",
                        e
                    );
                    state_tx.send(LockState::Unknown)?;
                    tokio::time::sleep(Duration::from_secs(30)).await;
                }
                Ok(cv) => {
                    let mut lock_state = None;
                    for v in cv.value.iter() {
                        if let K2vValue::Value(vbytes) = v {
                            if vbytes.len() == 32 {
                                let ts = u64::from_be_bytes(vbytes[..8].try_into().unwrap());
                                let pid = UniqueIdent(vbytes[8..].try_into().unwrap());
                                if lock_state
                                    .map(|(pid2, ts2)| ts > ts2 || (ts == ts2 && pid > pid2))
                                    .unwrap_or(true)
                                {
                                    lock_state = Some((pid, ts));
                                }
                            }
                        }
                    }
                    state_tx.send(
                        lock_state
                            .map(|(pid, ts)| LockState::Held(pid, ts, cv.causality.clone()))
                            .unwrap_or(LockState::Empty),
                    )?;
                    ct = Some(cv.causality);
                }
            }
            info!("Stopping lock state watch");
        }
    }
    .boxed();

    // Loop 2: notify user whether we are holding the lock or not
    let lock_notify_loop: BoxFuture<Result<()>> = async {
        loop {
            let now = now_msec();
            let held_with_expiration_time = match &*state_rx.borrow_and_update() {
                LockState::Held(pid, ts, _ct) if *pid == our_pid => {
                    let expiration_time = *ts - (LOCK_DURATION / 3).as_millis() as u64;
                    if now < expiration_time {
                        Some(expiration_time)
                    } else {
                        None
                    }
                }
                _ => None,
            };
            held_tx.send(held_with_expiration_time.is_some())?;

            let await_expired = async {
                match held_with_expiration_time {
                    None => futures::future::pending().await,
                    Some(expiration_time) => {
                        tokio::time::sleep(Duration::from_millis(expiration_time - now)).await
                    }
                };
            };

            tokio::select!(
                r = state_rx.changed() => {
                    r?;
                }
                _ = held_tx.closed() => bail!("held_tx closed, don't need to hold lock anymore"),
                _ = await_expired => continue,
            );
        }
    }
    .boxed();

    // Loop 3: acquire lock when relevant
    let take_lock_loop: BoxFuture<Result<()>> = async {
        loop {
            let now = now_msec();
            let state: LockState = state_rx_2.borrow_and_update().clone();
            let (acquire_at, ct) = match state {
                LockState::Unknown => {
                    // If state of the lock is unknown, don't try to acquire
                    state_rx_2.changed().await?;
                    continue;
                }
                LockState::Empty => (now, None),
                LockState::Held(pid, ts, ct) => {
                    if pid == our_pid {
                        (ts - (2 * LOCK_DURATION / 3).as_millis() as u64, Some(ct))
                    } else {
                        (ts, Some(ct))
                    }
                }
            };

            // Wait until it is time to acquire lock
            if acquire_at > now {
                tokio::select!(
                    r = state_rx_2.changed() => {
                        // If lock state changed in the meantime, don't acquire and loop around
                        r?;
                        continue;
                    }
                    _ = tokio::time::sleep(Duration::from_millis(acquire_at - now)) => ()
                );
            }

            // Acquire lock
            let mut lock = vec![0u8; 32];
            lock[..8].copy_from_slice(&u64::to_be_bytes(now_msec()));
            lock[8..].copy_from_slice(&our_pid.0);
            if let Err(e) = k2v.insert_item(pk, sk, lock, ct).await {
                error!("Could not take lock: {}", e);
                tokio::time::sleep(Duration::from_secs(30)).await;
            }

            // Wait for new information to loop back
            state_rx_2.changed().await?;
        }
    }
    .boxed();

    let res = futures::try_join!(watch_lock_loop, lock_notify_loop, take_lock_loop);
    info!("lock loop exited: {:?}", res);
}

// ---- UTIL: function to wait for a value to have changed in K2V ----

async fn k2v_wait_value_changed<'a>(
    k2v: &'a K2vClient,
    pk: &'static str,
    sk: &'static str,
    prev_ct: &'a Option<CausalityToken>,
) -> Result<CausalValue> {
    loop {
        if let Some(ct) = prev_ct {
            match k2v.poll_item(pk, sk, ct.clone(), None).await? {
                None => continue,
                Some(cv) => return Ok(cv),
            }
        } else {
            match k2v.read_item(pk, sk).await {
                Err(k2v_client::Error::NotFound) => {
                    k2v.insert_item(pk, sk, vec![0u8], None).await?;
                }
                Err(e) => return Err(e.into()),
                Ok(cv) => return Ok(cv),
            }
        }
    }
}

// ---- LMTP SIDE: storing messages encrypted with user's pubkey ----

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
