//use std::collections::HashMap;
use std::convert::TryFrom;

use std::sync::{Arc, Weak};
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use futures::{future::BoxFuture, FutureExt};
//use tokio::io::AsyncReadExt;
use tokio::sync::watch;
use tracing::{error, info, warn};

use crate::cryptoblob;
use crate::login::{Credentials, PublicCredentials};
use crate::mail::mailbox::Mailbox;
use crate::mail::uidindex::ImapUidvalidity;
use crate::mail::unique_ident::*;
use crate::mail::user::User;
use crate::mail::IMF;
use crate::storage;
use crate::timestamp::now_msec;

const INCOMING_PK: &str = "incoming";
const INCOMING_LOCK_SK: &str = "lock";
const INCOMING_WATCH_SK: &str = "watch";

const MESSAGE_KEY: &str = "message-key";

// When a lock is held, it is held for LOCK_DURATION (here 5 minutes)
// It is renewed every LOCK_DURATION/3
// If we are at 2*LOCK_DURATION/3 and haven't renewed, we assume we
// lost the lock.
const LOCK_DURATION: Duration = Duration::from_secs(300);

// In addition to checking when notified, also check for new mail every 10 minutes
const MAIL_CHECK_INTERVAL: Duration = Duration::from_secs(600);

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
    let mut lock_held = k2v_lock_loop(creds.row_client()?, INCOMING_PK, INCOMING_LOCK_SK);

    let k2v = creds.row_client()?;
    let s3 = creds.blob_client()?;

    let mut inbox: Option<Arc<Mailbox>> = None;
    let mut incoming_key = k2v.row(INCOMING_PK, INCOMING_WATCH_SK);

    loop {
        let maybe_updated_incoming_key = if *lock_held.borrow() {
            info!("incoming lock held");

            let wait_new_mail = async {
                loop {
                    match incoming_key.poll().await
                    {
                        Ok(row_val) => break row_val.to_ref(),
                        Err(e) => {
                            error!("Error in wait_new_mail: {}", e);
                            tokio::time::sleep(Duration::from_secs(30)).await;
                        }
                    }
                }
            };

            tokio::select! {
                inc_k = wait_new_mail => Some(inc_k),
                _     = tokio::time::sleep(MAIL_CHECK_INTERVAL) => Some(k2v.from_orphan(incoming_key.to_orphan())),
                _     = lock_held.changed() => None,
                _     = rx_inbox_id.changed() => None,
            }
        } else {
            info!("incoming lock not held");
            tokio::select! {
                _ = lock_held.changed() => None,
                _ = rx_inbox_id.changed() => None,
            }
        };

        let user = match Weak::upgrade(&user) {
            Some(user) => user,
            None => {
                info!("User no longer available, exiting incoming loop.");
                break;
            }
        };
        info!("User still available");

        // If INBOX no longer is same mailbox, open new mailbox
        let inbox_id = *rx_inbox_id.borrow();
        if let Some((id, uidvalidity)) = inbox_id {
            if Some(id) != inbox.as_ref().map(|b| b.id) {
                match user.open_mailbox_by_id(id, uidvalidity).await {
                    Ok(mb) => {
                        inbox = Some(mb);
                    }
                    Err(e) => {
                        inbox = None;
                        error!("Error when opening inbox ({}): {}", id, e);
                        tokio::time::sleep(Duration::from_secs(30)).await;
                        continue;
                    }
                }
            }
        }

        // If we were able to open INBOX, and we have mail,
        // fetch new mail
        if let (Some(inbox), Some(updated_incoming_key)) = (&inbox, maybe_updated_incoming_key) {
            match handle_incoming_mail(&user, &s3, inbox, &lock_held).await {
                Ok(()) => {
                    incoming_key = updated_incoming_key;
                }
                Err(e) => {
                    error!("Could not fetch incoming mail: {}", e);
                    tokio::time::sleep(Duration::from_secs(30)).await;
                }
            }
        }
    }
    drop(rx_inbox_id);
    Ok(())
}

async fn handle_incoming_mail(
    user: &Arc<User>,
    blobs: &storage::BlobStore,
    inbox: &Arc<Mailbox>,
    lock_held: &watch::Receiver<bool>,
) -> Result<()> {
    let mails_res = blobs.list("incoming/").await?;

    for object in mails_res {
        if !*lock_held.borrow() {
            break;
        }
        let key = object.key();
        if let Some(mail_id) = key.strip_prefix("incoming/") {
            if let Ok(mail_id) = mail_id.parse::<UniqueIdent>() {
                move_incoming_message(user, blobs, inbox, mail_id).await?;
            }
        }
    }

    Ok(())
}

async fn move_incoming_message(
    user: &Arc<User>,
    s3: &storage::BlobStore,
    inbox: &Arc<Mailbox>,
    id: UniqueIdent,
) -> Result<()> {
    info!("Moving incoming message: {}", id);

    let object_key = format!("incoming/{}", id);

    // 1. Fetch message from S3
    let object = s3.blob(&object_key).fetch().await?;

    // 1.a decrypt message key from headers
    //info!("Object metadata: {:?}", get_result.metadata);
    let key_encrypted_b64 = object
        .get_meta(MESSAGE_KEY)
        .ok_or(anyhow!("Missing key in metadata"))?;
    let key_encrypted = base64::decode(key_encrypted_b64)?;
    let message_key = sodiumoxide::crypto::sealedbox::open(
        &key_encrypted,
        &user.creds.keys.public,
        &user.creds.keys.secret,
    )
    .map_err(|_| anyhow!("Cannot decrypt message key"))?;
    let message_key =
        cryptoblob::Key::from_slice(&message_key).ok_or(anyhow!("Invalid message key"))?;

    // 1.b retrieve message body
    let obj_body = object.content().ok_or(anyhow!("Missing object body"))?;
    let plain_mail = cryptoblob::open(&obj_body, &message_key)
        .map_err(|_| anyhow!("Cannot decrypt email content"))?;

    // 2 parse mail and add to inbox
    let msg = IMF::try_from(&plain_mail[..]).map_err(|_| anyhow!("Invalid email body"))?;
    inbox
        .append_from_s3(msg, id, object.to_ref(), message_key)
        .await?;

    // 3 delete from incoming
    object.to_ref().rm().await?;

    Ok(())
}

// ---- UTIL: K2V locking loop, use this to try to grab a lock using a K2V entry as a signal ----

fn k2v_lock_loop(k2v: storage::RowStore, pk: &'static str, sk: &'static str) -> watch::Receiver<bool> {
    let (held_tx, held_rx) = watch::channel(false);

    tokio::spawn(k2v_lock_loop_internal(k2v, pk, sk, held_tx));

    held_rx
}

#[derive(Clone, Debug)]
enum LockState {
    Unknown,
    Empty,
    Held(UniqueIdent, u64, storage::OrphanRowRef),
}

async fn k2v_lock_loop_internal(
    k2v: storage::RowStore,
    pk: &'static str,
    sk: &'static str,
    held_tx: watch::Sender<bool>,
) {
    let (state_tx, mut state_rx) = watch::channel::<LockState>(LockState::Unknown);
    let mut state_rx_2 = state_rx.clone();

    let our_pid = gen_ident();

    // Loop 1: watch state of lock in K2V, save that in corresponding watch channel
    let watch_lock_loop: BoxFuture<Result<()>> = async {
        let mut ct = k2v.row(pk, sk);
        loop {
            info!("k2v watch lock loop iter: ct = {:?}", ct);
            match ct.poll().await {
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
                    for v in cv.content().iter() {
                        if let storage::Alternative::Value(vbytes) = v {
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
                    let new_ct = cv.to_ref();

                    info!(
                        "k2v watch lock loop: changed, old ct = {:?}, new ct = {:?}, v = {:?}",
                        ct, new_ct, lock_state
                    );
                    state_tx.send(
                        lock_state
                            .map(|(pid, ts)| LockState::Held(pid, ts, new_ct.to_orphan()))
                            .unwrap_or(LockState::Empty),
                    )?;
                    ct = new_ct;
                }
            }
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
            let held = held_with_expiration_time.is_some();
            if held != *held_tx.borrow() {
                held_tx.send(held)?;
            }

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
            lock[..8].copy_from_slice(&u64::to_be_bytes(
                now_msec() + LOCK_DURATION.as_millis() as u64,
            ));
            lock[8..].copy_from_slice(&our_pid.0);
            let row = match ct {
                Some(orphan) => k2v.from_orphan(orphan),
                None => k2v.row(pk, sk),
            };
            if let Err(e) = row.set_value(lock).push().await {
                error!("Could not take lock: {}", e);
                tokio::time::sleep(Duration::from_secs(30)).await;
            }

            // Wait for new information to loop back
            state_rx_2.changed().await?;
        }
    }
    .boxed();

    let _ = futures::try_join!(watch_lock_loop, lock_notify_loop, take_lock_loop);

    info!("lock loop exited, releasing");

    if !held_tx.is_closed() {
        warn!("wierd...");
        let _ = held_tx.send(false);
    }

    // If lock is ours, release it
    let release = match &*state_rx.borrow() {
        LockState::Held(pid, _, ct) if *pid == our_pid => Some(ct.clone()),
        _ => None,
    };
    if let Some(ct) = release {
        let row = k2v.from_orphan(ct);
        let _ = row.rm().await;
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
        let s3_client = creds.storage.blob_store()?;
        let k2v_client = creds.storage.row_store()?;

        // Get causality token of previous watch key
        let query = k2v_client.row(INCOMING_PK, INCOMING_WATCH_SK);
        let watch_ct = match query.fetch().await {
            Err(_) => query,
            Ok(cv) => cv.to_ref(),
        };

        // Write mail to encrypted storage
        let encrypted_key =
            sodiumoxide::crypto::sealedbox::seal(self.key.as_ref(), &creds.public_key);
        let key_header = base64::encode(&encrypted_key);

        let mut  send = s3_client
            .blob(&format!("incoming/{}", gen_ident()))
            .set_value(self.encrypted_body.clone().into());
        send.set_meta(MESSAGE_KEY, &key_header);
        send.push().await?;

        // Update watch key to signal new mail
        watch_ct.set_value(gen_ident().0.to_vec()).push().await?;

        Ok(())
    }
}
