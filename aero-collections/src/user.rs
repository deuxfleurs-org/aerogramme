use std::collections::HashMap;

use anyhow::Result;
use lazy_static::lazy_static;

use aero_user::login::Credentials;
use aero_user::storage;

use crate::calendar::CalendarNs;
use crate::mail::namespace::MailboxNs;

#[derive(Clone)]
pub struct User {
    pub username: String,
    pub creds: Credentials,
    pub mailboxes: MailboxNs,
    pub calendars: CalendarNs,
}

impl User {
    pub async fn new(username: String, creds: Credentials) -> Result<Self> {
        let cache_key = (username.clone(), creds.storage.unique());

        {
            let cache = USER_CACHE.lock().unwrap();
            if let Some(u) = cache.get(&cache_key) {
                return Ok(u.clone());
            }
        }

        let user = Self::open(username, creds).await?;

        let mut cache = USER_CACHE.lock().unwrap();
        if let Some(concurrent_user) = cache.get(&cache_key) {
            drop(user);
            Ok(concurrent_user.clone())
        } else {
            cache.insert(cache_key, user.clone());
            Ok(user)
        }
    }

    async fn open(username: String, creds: Credentials) -> Result<Self> {
        let user = Self {
            username,
            creds: creds.clone(),
            mailboxes: MailboxNs::new(creds.clone()).await?,
            calendars: CalendarNs::new(creds.clone()),
        };

        Ok(user)
    }
}

// ---- User cache ----

lazy_static! {
    static ref USER_CACHE: std::sync::Mutex<HashMap<(String, storage::UnicityBuffer), User>> =
        std::sync::Mutex::new(HashMap::new());
}
