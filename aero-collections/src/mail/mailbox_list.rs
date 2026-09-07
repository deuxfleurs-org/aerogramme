use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use aero_bayou::timestamp::now_msec;

use crate::crdt::Crdt;
use crate::mail::uidindex::ImapUidvalidity;
use crate::unique_ident::UniqueIdent;

/// A list of mailbox names in a user namespace, designed to be serialized as a
/// single K2V entry.
#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct MailboxList(BTreeMap<String, MailboxListEntry>);

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
struct MailboxListEntry {
    id_lww: (u64, MailboxListVal),
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, PartialOrd)]
pub enum MailboxListVal {
    /// Deleted entry with this ID; we remember the uidvalidity value to avoid
    /// reusing it if the mailbox is later re-created with the same name
    Deleted { prev_uidvalidity: ImapUidvalidity },
    /// Active entry with this ID
    Active { id: UniqueIdent },
}

impl MailboxListEntry {
    fn merge(&mut self, other: &Self) {
        // CRDT merge rule:
        // - on Deleted, merge the uidvalidity by taking the max
        // - otherwise, use the timestamps to implement a last-write-wins policy
        use MailboxListVal::Deleted;
        match (self.id_lww, other.id_lww) {
            (
                (
                    ts1,
                    Deleted {
                        prev_uidvalidity: uidv1,
                    },
                ),
                (
                    ts2,
                    Deleted {
                        prev_uidvalidity: uidv2,
                    },
                ),
            ) => {
                self.id_lww = (
                    ts1.max(ts2),
                    Deleted {
                        prev_uidvalidity: uidv1.max(uidv2),
                    },
                );
            }
            ((self_ts, self_val), (other_ts, other_val)) => {
                if other_ts > self_ts || (other_ts == self_ts && other_val > self_val) {
                    self.id_lww = (other_ts, other_val)
                }
            }
        }
    }
}

impl Crdt for MailboxList {
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

impl MailboxList {
    /// Get a list of all ident names.
    pub fn names(&self) -> Vec<String> {
        self.0
            .iter()
            .filter(|(_, v)| matches!(v.id_lww.1, MailboxListVal::Active { .. }))
            .map(|(k, _)| k.to_string())
            .collect()
    }

    pub fn has(&self, name: &str) -> bool {
        matches!(
            self.0.get(name),
            Some(MailboxListEntry {
                id_lww: (_, MailboxListVal::Active { .. }),
                ..
            })
        )
    }

    pub fn get(&self, name: &str) -> Option<MailboxListVal> {
        self.0.get(name).map(|entry| entry.id_lww.1)
    }

    /// Ensures name `name` maps to value `v`.
    /// If it already mapped to that, returns false.
    /// If a change had to be done, returns true.
    pub fn set(&mut self, name: &str, v: MailboxListVal) -> bool {
        let (ts, id) = match self.0.get_mut(name) {
            None => {
                // The entry does not exist, initialize it.
                (now_msec(), v)
            }
            Some(MailboxListEntry { id_lww }) => {
                // The entry currently exists.
                if id_lww.1 == v {
                    // Entry has the requested `id`. Nothing to do.
                    return false;
                } else {
                    // Update the Last Writer Wins CRDT with the new `id`.
                    (std::cmp::max(id_lww.0 + 1, now_msec()), v)
                }
            }
        };

        self.0
            .insert(name.into(), MailboxListEntry { id_lww: (ts, id) });
        true
    }
}
