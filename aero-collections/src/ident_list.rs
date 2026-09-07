use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use aero_bayou::timestamp::now_msec;
use aero_user::cryptoblob::{open_deserialize, seal_serialize};
use aero_user::login::Credentials;
use aero_user::storage;

use crate::unique_ident::{gen_ident, UniqueIdent};

/// A list of named identifiers, designed to be serialized as a single K2V entry.
///
/// This is useful to represent the list of toplevel collections in a namespace,
/// such as mailboxes, calendars, etc.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct IdentList(BTreeMap<String, IdentListEntry>);

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
struct IdentListEntry {
    id_lww: (u64, IdentListVal),
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
enum IdentListVal {
    /// Active entry with this ID
    Active(UniqueIdent),
    /// Deleted entry with this ID; the ID will be reused if the entry is
    /// re-created
    Deleted(UniqueIdent),
    /// Empty entry
    None,
}

impl PartialOrd for IdentListVal {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        use core::cmp::Ordering;
        match (self, other) {
            (Self::None, Self::None) => Some(Ordering::Equal),
            (Self::None, _) => Some(Ordering::Less),
            (_, Self::None) => Some(Ordering::Greater),
            
            (Self::Deleted(id), Self::Deleted(id2)) |
            (Self::Active(id), Self::Active(id2)) => id.partial_cmp(id2),
            
            (Self::Deleted(_), Self::Active(_)) => Some(Ordering::Less),
            (Self::Active(_), Self::Deleted(_)) => Some(Ordering::Greater),
        }
    }
}

impl IdentListVal {
    pub fn into_active(&self) -> Option<UniqueIdent> {
        match self {
            Self::Active(id) => Some(*id),
            Self::Deleted(_) | Self::None => None,
        }
    }
}

impl IdentListEntry {
    fn merge(&mut self, other: &Self) {
        // Simple CRDT merge rule
        if other.id_lww.0 > self.id_lww.0
            || (other.id_lww.0 == self.id_lww.0 && other.id_lww.1 > self.id_lww.1)
        {
            self.id_lww = other.id_lww;
        }
    }
}

impl IdentList {
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }
    
    pub async fn load_from_storage(
        creds: &Credentials,
        pk: &str,
        sk: &str,
    ) -> Result<(Self, Option<storage::RowRef>)> {
        match creds.storage.row_fetch(pk, sk).await {
            Err(storage::StorageError::NotFound) => Ok((IdentList::new(), None)),
            Err(e) => Err(e.into()),
            Ok(rv) => {
                let mut list = IdentList::new();
                let (row_ref, row_vals) = (rv.row_ref, rv.value);

                for v in row_vals {
                    if let storage::Alternative::Value(vbytes) = v {
                        let list2 =
                            open_deserialize::<IdentList>(&vbytes, &creds.keys.master)?;
                        list.merge(list2);
                    }
                }
                Ok((list, Some(row_ref)))
            }
        }
    }

    pub async fn store_to_storage(
        &self,
        creds: &Credentials,
        pk: &str,
        sk: &str,
        ct: Option<storage::RowRef>,
    ) -> Result<()> {
        let list_blob = seal_serialize(self, &creds.keys.master)?;
        let rref = ct.unwrap_or(storage::RowRef::new(pk, sk));
        let row_val = storage::RowVal::new(rref, list_blob);
        creds.storage.row_update(vec![row_val]).await?;
        Ok(())
    }

    pub fn merge(&mut self, list2: Self) {
        for (k, v) in list2.0.into_iter() {
            if let Some(e) = self.0.get_mut(&k) {
                e.merge(&v);
            } else {
                self.0.insert(k, v);
            }
        }
    }
    
    /// Get a list of all ident names.
    pub fn names(&self) -> Vec<String> {
        self.0
            .iter()
            .filter(|(_, v)| matches!(v.id_lww.1, IdentListVal::Active(_)))
            .map(|(k, _)| k.to_string())
            .collect()
    }

    pub fn has(&self, name: &str) -> bool {
        matches!(
            self.0.get(name),
            Some(IdentListEntry {
                id_lww: (_, IdentListVal::Active(_)),
                ..
            })
        )
    }

    pub fn get(&self, name: &str) -> Option<UniqueIdent> {
        self.0
            .get(name)
            .map(|IdentListEntry { id_lww: (_, id) }| id.into_active())
            .flatten()
    }

    pub fn delete(&mut self, name: &str) {
        self.set(name, IdentListVal::None);
    }

    pub fn delete_remember_id(&mut self, name: &str) {
        if let Some(IdentListEntry {
            id_lww: (_, IdentListVal::Active(id)),
        }) = self.0.get(name) {
            self.set(name, IdentListVal::Deleted(*id));
        }
    }

    /// Ensures name `name` maps to ident `id`.
    /// If it already mapped to that, returns false.
    /// If a change had to be done, returns true.
    fn set(&mut self, name: &str, id: IdentListVal) -> bool {
        let (ts, id) = match self.0.get_mut(name) {
            None => {
                if let IdentListVal::None = id {
                    // A None entry is equivalent to no entry. Nothing to do.
                    return false;
                } else {
                    // The entry does not exist, initialize it.
                    (now_msec(), id)
                }
            }
            Some(IdentListEntry { id_lww }) => {
                // The entry currently exists.
                if id_lww.1 == id {
                    // Entry has the requested `id`. Nothing to do.
                    return false;
                } else {
                    // Update the Last Writer Wins CRDT with the new `id`.
                    (std::cmp::max(id_lww.0 + 1, now_msec()), id)
                }
            }
        };

        self.0
            .insert(name.into(), IdentListEntry { id_lww: (ts, id) });
        true
    }

    pub fn create(&mut self, name: &str) -> CreatedResult {
        match self.0.get(name).map(|ent| ent.id_lww.1) {
            Some(IdentListVal::Active(id)) => CreatedResult::Existed(id),
            Some(IdentListVal::Deleted(id)) => {
                self.set(name, IdentListVal::Active(id));
                CreatedResult::Created(id)
            },
            Some(IdentListVal::None) | None => {
                let id = gen_ident();
                self.set(name, IdentListVal::Active(id));
                CreatedResult::Created(id)
            }
        }
    }

    pub fn rename(&mut self, old_name: &str, new_name: &str) -> Result<()> {
        //@TODO: does this have reasonable semantics if merged with other
        // concurrent operations?
        if let Some(mbid) = self.get(old_name) {
            if self.has(new_name) {
                bail!(
                    "Cannot rename {} into {}: {} already exists",
                    old_name,
                    new_name,
                    new_name
                );
            }

            self.set(old_name, IdentListVal::None);
            self.set(new_name, IdentListVal::Active(mbid));
            Ok(())
        } else {
            bail!(
                "Cannot rename {} into {}: {} doesn't exist",
                old_name,
                new_name,
                old_name
            );
        }
    }
}

pub(crate) enum CreatedResult {
    Created(UniqueIdent),
    Existed(UniqueIdent),
}
