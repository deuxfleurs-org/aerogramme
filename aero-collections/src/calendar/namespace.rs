use anyhow::{bail, Result};
use std::collections::HashMap;
use std::sync::Arc;

use aero_user::login::Credentials;
use aero_user::storage;

use crate::dav::collection::{Collection, CollectionWeak};
use crate::ident_list::{CreatedResult, IdentList};
use crate::unique_ident::UniqueIdent;

pub(crate) const CAL_LIST_PK: &str = "calendars";
pub(crate) const CAL_LIST_SK: &str = "list";
pub(crate) const MAIN_CAL: &str = "Personal";
pub(crate) const MAX_CALNAME_CHARS: usize = 32;

#[derive(Clone)]
pub struct CalendarNs {
    creds: Credentials,
    calendars: Arc<std::sync::Mutex<HashMap<UniqueIdent, CollectionWeak>>>,
}

impl CalendarNs {
    /// Create a new calendar namespace
    pub fn new(creds: Credentials) -> Self {
        Self {
            creds,
            calendars: Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }

    /// Open a calendar by name
    pub async fn open(&self, name: &str) -> Result<Option<Collection>> {
        let (list, _ct) = self.load_calendar_list().await?;

        match list.get(name) {
            None => Ok(None),
            Some(ident) => Ok(Some(self.open_by_id(ident).await?)),
        }
    }

    /// Open a calendar by unique id
    /// Check mail::namespace::open_mailbox_by_id to understand this function
    pub async fn open_by_id(&self, id: UniqueIdent) -> Result<Collection> {
        {
            let cache = self.calendars.lock().unwrap();
            if let Some(cal_weak) = cache.get(&id) {
                if let Some(cal) = cal_weak.upgrade() {
                    return Ok(cal);
                }
            }
        }

        let cal = Collection::open(&self.creds, "calendar", id).await?;

        let mut cache = self.calendars.lock().unwrap();
        if let Some(concurrent_cal_weak) = cache.get(&id) {
            if let Some(concurrent_cal) = concurrent_cal_weak.upgrade() {
                drop(cal); // we worked for nothing but at least we didn't starve someone else
                return Ok(concurrent_cal);
            }
        }

        cache.insert(id, cal.downgrade());
        Ok(cal)
    }

    /// List calendars
    pub async fn list(&self) -> Result<Vec<String>> {
        self.load_calendar_list()
            .await
            .map(|(list, _)| list.names())
    }

    /// Delete a calendar from the index
    pub async fn delete(&self, name: &str) -> Result<()> {
        // We currently assume that main cal is a bit specific
        if name == MAIN_CAL {
            bail!("Cannot delete main calendar");
        }

        let (mut list, ct) = self.load_calendar_list().await?;
        if list.has(name) {
            //@TODO: actually delete calendar content
            list.set(name, None);
            self.save_calendar_list(&list, ct).await?;
            Ok(())
        } else {
            bail!("Calendar {} does not exist", name);
        }
    }

    /// Rename a calendar in the index
    pub async fn rename(&self, old: &str, new: &str) -> Result<()> {
        if old == MAIN_CAL {
            bail!("Renaming main calendar is not supported currently");
        }
        if !new.chars().all(char::is_alphanumeric) {
            bail!("Unsupported characters in new calendar name, only alphanumeric characters are allowed currently");
        }
        if new.len() > MAX_CALNAME_CHARS {
            bail!("Calendar name can't contain more than 32 characters");
        }

        let (mut list, ct) = self.load_calendar_list().await?;
        list.rename(old, new)?;
        self.save_calendar_list(&list, ct).await?;

        Ok(())
    }

    /// Create calendar
    pub async fn create(&self, name: &str) -> Result<()> {
        if name == MAIN_CAL {
            bail!("Main calendar is automatically created, can't create it manually");
        }
        if !name.chars().all(char::is_alphanumeric) {
            bail!("Unsupported characters in new calendar name, only alphanumeric characters are allowed");
        }
        if name.len() > MAX_CALNAME_CHARS {
            bail!("Calendar name can't contain more than 32 characters");
        }

        let (mut list, ct) = self.load_calendar_list().await?;
        match list.create(name) {
            CreatedResult::Existed(_) => bail!("Calendar {} already exists", name),
            CreatedResult::Created(_) => (),
        }
        self.save_calendar_list(&list, ct).await?;

        Ok(())
    }

    /// Has calendar
    pub async fn has(&self, name: &str) -> Result<bool> {
        self.load_calendar_list()
            .await
            .map(|(list, _)| list.has(name))
    }

    // --- internal calendar list management ----

    /// Load from storage
    async fn load_calendar_list(&self) -> Result<(IdentList, Option<storage::RowRef>)> {
        let (mut list, row) = IdentList::load_from_storage(&self.creds, CAL_LIST_PK, CAL_LIST_SK).await?;

        // Create default calendars (currently only one calendar is created)
        let is_default_cal_missing = [MAIN_CAL]
            .iter()
            .map(|calname| list.create(calname))
            .fold(false, |acc, r| {
                acc || matches!(r, CreatedResult::Created(..))
            });

        // Save the index if we created a new calendar
        if is_default_cal_missing {
            self.save_calendar_list(&list, row.clone()).await?;
        }

        Ok((list, row))
    }

    /// Save an updated index
    async fn save_calendar_list(
        &self,
        list: &IdentList,
        ct: Option<storage::RowRef>,
    ) -> Result<()> {
        list.store_to_storage(&self.creds, CAL_LIST_PK, CAL_LIST_SK, ct).await
    }
}
