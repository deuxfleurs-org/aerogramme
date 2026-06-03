use std::collections::HashMap;
use std::sync::{Arc, Weak};

use anyhow::Result;
use lazy_static::lazy_static;

use aero_user::login::Credentials;
use aero_user::storage;

use crate::calendar::namespace::CalendarNs;
use crate::mail::namespace::MailboxNs;

//@FIXME User should be run in a LocalSet
// to remove most - if not all - synchronizations types.
// Especially RwLock & co.

pub struct User {
    pub username: String,
    pub creds: Credentials,
    pub storage: storage::Store,
    pub mailboxes: MailboxNs,
    pub calendars: CalendarNs,
}

impl User {
    pub async fn new(username: String, creds: Credentials) -> Result<Arc<Self>> {
        let cache_key = (username.clone(), creds.storage.unique());

        {
            let cache = USER_CACHE.lock().unwrap();
            if let Some(u) = cache.get(&cache_key).and_then(Weak::upgrade) {
                return Ok(u);
            }
        }

        let user = Self::open(username, creds).await?;

        let mut cache = USER_CACHE.lock().unwrap();
        if let Some(concurrent_user) = cache.get(&cache_key).and_then(Weak::upgrade) {
            drop(user);
            Ok(concurrent_user)
        } else {
            cache.insert(cache_key, Arc::downgrade(&user));
            Ok(user)
        }
    }

    // ---- Internal user & mailbox management ----

    async fn open(username: String, creds: Credentials) -> Result<Arc<Self>> {
        let storage = creds.storage.clone();

        let user = Arc::new(Self {
            username,
            creds: creds.clone(),
            storage,
            mailboxes: MailboxNs::new(creds.clone()).await?,
            calendars: CalendarNs::new(),
        });

        Ok(user)
    }
}

// ---- User cache ----

lazy_static! {
    static ref USER_CACHE: std::sync::Mutex<HashMap<(String, storage::UnicityBuffer), Weak<User>>> =
        std::sync::Mutex::new(HashMap::new());
}
