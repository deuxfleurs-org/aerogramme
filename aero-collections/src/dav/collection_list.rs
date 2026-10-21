use anyhow::{bail, Result};

use aero_bayou::timestamp::now_msec;

use crate::crdt::Crdt;
use crate::unique_ident::{gen_ident, UniqueIdent};

mod v06 {
    use crate::unique_ident::UniqueIdent;
    use aero_user::migrate::InitialFormat;
    use serde::{Deserialize, Serialize};
    use std::collections::BTreeMap;

    /// A list of DAV collection names, designed to be serialized as a single K2V entry.
    #[derive(Debug, Default, Serialize, Deserialize)]
    pub(crate) struct CollectionList(pub(super) BTreeMap<String, CollectionListEntry>);

    #[derive(Serialize, Deserialize, Clone, Copy, Debug)]
    pub(crate) struct CollectionListEntry {
        pub(super) id_lww: (u64, Option<UniqueIdent>),
    }

    impl InitialFormat for CollectionList {
        const VERSION_MARKER: &'static [u8] = b"A06cols";
    }
}

pub(crate) use v06::*;

impl CollectionListEntry {
    fn merge(&mut self, other: &Self) {
        // Simple CRDT merge rule
        if other.id_lww.0 > self.id_lww.0
            || (other.id_lww.0 == self.id_lww.0 && other.id_lww.1 > self.id_lww.1)
        {
            self.id_lww = other.id_lww;
        }
    }
}

impl Crdt for CollectionList {
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

impl CollectionList {
    /// Get a list of all ident names.
    pub fn names(&self) -> Vec<String> {
        self.0
            .iter()
            .filter(|(_, v)| v.id_lww.1.is_some())
            .map(|(k, _)| k.to_string())
            .collect()
    }

    pub fn has(&self, name: &str) -> bool {
        self.0.get(name).is_some()
    }

    pub fn get(&self, name: &str) -> Option<UniqueIdent> {
        self.0.get(name).and_then(|entry| entry.id_lww.1)
    }

    pub fn delete(&mut self, name: &str) {
        self.set(name, None);
    }

    /// Ensures name `name` maps to ident `id`.
    /// If it already mapped to that, returns false.
    /// If a change had to be done, returns true.
    fn set(&mut self, name: &str, id: Option<UniqueIdent>) -> bool {
        let (ts, id) = match self.0.get_mut(name) {
            None => {
                // The entry does not exist.
                if id.is_none() {
                    // The user wants to delete the entry (`id` is `None`). Nothing to do.
                    return false;
                } else {
                    // The user wants to set the entry (`id` is `Some`). Initialize it.
                    (now_msec(), id)
                }
            }
            Some(CollectionListEntry { id_lww }) => {
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
            .insert(name.into(), CollectionListEntry { id_lww: (ts, id) });
        true
    }

    pub fn create(&mut self, name: &str) -> CreateResult {
        if let Some(_id) = self.get(name) {
            return CreateResult::Existed;
        }

        let id = gen_ident();
        self.set(name, Some(id));
        CreateResult::Created
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

            self.set(old_name, None);
            self.set(new_name, Some(mbid));
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

pub(crate) enum CreateResult {
    Created,
    Existed,
}
