use anyhow::Result;
use std::collections::{HashMap, BTreeMap};
use std::sync::{Weak, Arc};

use serde::{Deserialize, Serialize};

use aero_user::storage;

use crate::unique_ident::UniqueIdent;
use crate::user::User;
use super::Calendar;

pub(crate) const CAL_LIST_PK: &str = "calendars";
pub(crate) const CAL_LIST_SK: &str = "list";

pub(crate) struct CalendarNs(std::sync::Mutex<HashMap<UniqueIdent, Weak<Calendar>>>);
impl CalendarNs {
    pub fn new() -> Self {
        Self(std::sync::Mutex::new(HashMap::new()))
    }

    pub fn list(&self) {
        todo!();
    }
}

#[derive(Serialize, Deserialize)]
pub(crate) struct CalendarList(BTreeMap<String, CalendarListEntry>);

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub(crate) struct CalendarListEntry {
    id_lww: (u64, Option<UniqueIdent>),
}

impl CalendarList {
    pub(crate) async fn load(user: &Arc<User>) -> Result<(Self, Option<storage::RowRef>)> {
        todo!();
    }

    pub(crate) async fn save(user: &Arc<User>, ct: Option<storage::RowRef>) -> Result<()> {
        todo!();
    }

    pub(crate) fn new() -> Self {
        Self(BTreeMap::new())
    }
}
