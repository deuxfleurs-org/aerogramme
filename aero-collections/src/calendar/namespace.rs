use anyhow::{bail, Result};
use std::collections::{HashMap, BTreeMap};
use std::sync::{Weak, Arc};

use serde::{Deserialize, Serialize};

use aero_bayou::timestamp::now_msec;
use aero_user::storage;
use aero_user::cryptoblob::{open_deserialize, seal_serialize};

use crate::unique_ident::{gen_ident, UniqueIdent};
use crate::user::User;
use super::Calendar;

pub(crate) const CAL_LIST_PK: &str = "calendars";
pub(crate) const CAL_LIST_SK: &str = "list";
pub(crate) const MAIN_CAL: &str = "Personal";
pub(crate) const MAX_CALNAME_CHARS: usize = 32;

pub struct CalendarNs(std::sync::Mutex<HashMap<UniqueIdent, Weak<Calendar>>>);

impl CalendarNs {
    /// Create a new calendar namespace
    pub fn new() -> Self {
        Self(std::sync::Mutex::new(HashMap::new()))
    }

    /// Open a calendar by name
    pub async fn open(&self, user: &Arc<User>, name: &str) -> Result<Option<Arc<Calendar>>> {
        let (list, _ct) = CalendarList::load(user).await?;

        match list.get(name) {
            None => Ok(None),
            Some(ident) => Ok(Some(self.open_by_id(user, ident).await?)),
        }
    }

    /// Open a calendar by unique id
    /// Check user.rs::open_mailbox_by_id to understand this function
    pub async fn open_by_id(&self, user: &Arc<User>, id: UniqueIdent) -> Result<Arc<Calendar>> {
        {
            let cache = self.0.lock().unwrap();
            if let Some(cal) = cache.get(&id).and_then(Weak::upgrade) {
                return Ok(cal);
            }
        }

        let cal = Arc::new(Calendar::open(&user.creds, id).await?);
        
        let mut cache = self.0.lock().unwrap();
        if let Some(concurrent_cal) = cache.get(&id).and_then(Weak::upgrade) {
            drop(cal); // we worked for nothing but at least we didn't starve someone else
            Ok(concurrent_cal)
        } else {
            cache.insert(id, Arc::downgrade(&cal));
            Ok(cal)
        }
    }

    /// List calendars
    pub async fn list(&self, user: &Arc<User>) -> Result<Vec<String>> {
        CalendarList::load(user).await.map(|(list, _)| list.names())
    }

    /// Delete a calendar from the index
    pub async fn delete(&self, user: &Arc<User>, name: &str) -> Result<()> {
        // We currently assume that main cal is a bit specific
        if name == MAIN_CAL {
            bail!("Cannot delete main calendar");
        }

        let (mut list, ct) = CalendarList::load(user).await?;
        if list.has(name) {
            //@TODO: actually delete calendar content
            list.bind(name, None);
            list.save(user, ct).await?;
            Ok(())
        } else {
            bail!("Calendar {} does not exist", name);
        }
    }

    /// Rename a calendar in the index
    pub async fn rename(&self, user: &Arc<User>, old: &str, new: &str) -> Result<()> {
        if old == MAIN_CAL {
            bail!("Renaming main calendar is not supported currently");
        }
        if !new.chars().all(char::is_alphanumeric) {
            bail!("Unsupported characters in new calendar name, only alphanumeric characters are allowed currently");
        }
        if new.len() > MAX_CALNAME_CHARS {
            bail!("Calendar name can't contain more than 32 characters");
        }

        let (mut list, ct) = CalendarList::load(user).await?;
        list.rename(old, new)?;
        list.save(user, ct).await?;

        Ok(())
    }

    /// Create calendar
    pub async fn create(&self, user: &Arc<User>, name: &str) -> Result<()> {
        if name == MAIN_CAL {
            bail!("Main calendar is automatically created, can't create it manually");
        }
        if !name.chars().all(char::is_alphanumeric) {
            bail!("Unsupported characters in new calendar name, only alphanumeric characters are allowed");
        }
        if name.len() > MAX_CALNAME_CHARS {
            bail!("Calendar name can't contain more than 32 characters");
        }

        let (mut list, ct) = CalendarList::load(user).await?;
        match list.create(name) {
            CalendarExists::Existed(_) => bail!("Calendar {} already exists", name),
            CalendarExists::Created(_) => (),
        }
        list.save(user, ct).await?;
        
        Ok(())
    }

    /// Has calendar
    pub async fn has(&self, user: &Arc<User>, name: &str) -> Result<bool> {
        CalendarList::load(user).await.map(|(list, _)| list.has(name))
    }
}

// ------
// ------ From this point, implementation is hidden from the rest of the crate
// ------

#[derive(Serialize, Deserialize)]
struct CalendarList(BTreeMap<String, CalendarListEntry>);

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
struct CalendarListEntry {
    id_lww: (u64, Option<UniqueIdent>),
}

impl CalendarList {
    // ---- Index persistence related functions

    /// Load from storage
    async fn load(user: &Arc<User>) -> Result<(Self, Option<storage::RowRef>)> {
        let row_ref = storage::RowRef::new(CAL_LIST_PK, CAL_LIST_SK);
        let (mut list, row) = match user
            .storage
            .row_fetch(&storage::Selector::Single(&row_ref))
            .await
        {
            Err(storage::StorageError::NotFound) => (Self::new(), None),
            Err(e) => return Err(e.into()),
            Ok(rv) => {
                let mut list = Self::new();
                let (row_ref, row_vals) = match rv.into_iter().next() {
                    Some(row_val) => (row_val.row_ref, row_val.value),
                    None => (row_ref, vec![]),
                };

                for v in row_vals {
                    if let storage::Alternative::Value(vbytes) = v {
                        let list2 = open_deserialize::<CalendarList>(&vbytes, &user.creds.keys.master)?;
                        list.merge(list2);
                    }
                }
                (list, Some(row_ref))
            }
        };

        // Create default calendars (currently only one calendar is created)
        let is_default_cal_missing = [MAIN_CAL]
            .iter()
            .map(|calname| list.create(calname))
            .fold(false, |acc, r| {
                acc || matches!(r, CalendarExists::Created(..))
            });

        // Save the index if we created a new calendar
        if is_default_cal_missing {
            list.save(user, row.clone()).await?;
        }

        Ok((list, row))
    }

    /// Save an updated index
    async fn save(&self, user: &Arc<User>, ct: Option<storage::RowRef>) -> Result<()> {
        let list_blob = seal_serialize(self, &user.creds.keys.master)?;
        let rref = ct.unwrap_or(storage::RowRef::new(CAL_LIST_PK, CAL_LIST_SK));
        let row_val = storage::RowVal::new(rref, list_blob);
        user.storage.row_insert(vec![row_val]).await?;
        Ok(())
    }

    // ----- Index manipulation functions

    /// Ensure that a given calendar exists
    /// (Don't forget to save if it returns CalendarExists::Created)
    fn create(&mut self, name: &str) -> CalendarExists {
        if let Some(CalendarListEntry {
            id_lww: (_, Some(id))
        }) = self.0.get(name)
        {
            return CalendarExists::Existed(*id);
        }

        let id = gen_ident();
        self.bind(name, Some(id)).unwrap();
        CalendarExists::Created(id)
    }

    /// Get a list of all calendar names
    fn names(&self) -> Vec<String> {
        self.0
            .iter()
            .filter(|(_, v)| v.id_lww.1.is_some())
            .map(|(k, _)| k.to_string())
            .collect()
    }

    /// For a given calendar name, get its Unique Identifier
    fn get(&self, name: &str) -> Option<UniqueIdent> {
        self.0.get(name).map(|CalendarListEntry { 
            id_lww: (_, ident),
        }| *ident).flatten()
    }

    /// Check if a given calendar name exists
    fn has(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// Rename a calendar
    fn rename(&mut self, old: &str, new: &str) -> Result<()> {
        if self.has(new) {
            bail!("Calendar {} already exists", new);
        }
        let ident = match self.get(old) {
            None => bail!("Calendar {} does not exist", old),
            Some(ident) => ident,
        };

        self.bind(old, None);
        self.bind(new, Some(ident));

        Ok(())
    }

    // ----- Internal logic

    /// New is not publicly exposed, use `load` instead
    fn new() -> Self {
        Self(BTreeMap::new())
    }

    /// Low level index updating logic (used to add/rename/delete) an entry
    fn bind(&mut self, name: &str, id: Option<UniqueIdent>) -> Option<()> {
        let (ts, id) = match self.0.get_mut(name) {
            None => {
                if id.is_none() {
                    // User wants to delete entry with given name (passed id is None)
                    // Entry does not exist (get_mut is None)
                    // Nothing to do
                    return None;
                } else {
                    // User wants entry with given name to be present (id is Some)
                    // Entry does not exist
                    // Initialize entry
                    (now_msec(), id)
                }
            }
            Some(CalendarListEntry {
                id_lww,
            }) => {
                if id_lww.1 == id {
                    // Entry is already equals to the requested id (Option<UniqueIdent)
                    // Nothing to do
                    return None;
                } else {
                    // Entry does not equal to what we know internally
                    // We update the Last Write Win CRDT here with the new id value
                    (
                        std::cmp::max(id_lww.0 + 1, now_msec()),
                        id,
                    )
                }
            }
        };

        // If we did not return here, that's because we have to update
        // something in our internal index.
        self.0.insert(
            name.into(),
            CalendarListEntry { id_lww: (ts, id) },
        );
        Some(())
    }

    // Merge 2 calendar lists by applying a LWW logic on each element
    fn merge(&mut self, list2: Self) {
        for (k, v) in list2.0.into_iter() {
            if let Some(e) = self.0.get_mut(&k) {
                e.merge(&v);
            } else {
                self.0.insert(k, v);
            }
        }
    }
}

impl CalendarListEntry {
    fn merge(&mut self, other: &Self) {
        // Simple CRDT merge rule
        if other.id_lww.0 > self.id_lww.0
            || (other.id_lww.0 == self.id_lww.0 && other.id_lww.1 > self.id_lww.1)
        {
            self.id_lww = other.id_lww;
        }
    }
}

pub(crate) enum CalendarExists {
    Created(UniqueIdent),
    Existed(UniqueIdent),
}
