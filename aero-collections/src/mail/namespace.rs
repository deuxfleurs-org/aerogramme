use std::collections::BTreeMap;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use aero_bayou::timestamp::now_msec;

use crate::mail::uidindex::ImapUidvalidity;
use crate::unique_ident::{gen_ident, UniqueIdent};

pub const MAILBOX_HIERARCHY_DELIMITER: char = '.';

/// INBOX is the only mailbox that must always exist.
/// It is created automatically when the account is created.
/// IMAP allows the user to rename INBOX to something else,
/// in this case all messages from INBOX are moved to a mailbox
/// with the new name and the INBOX mailbox still exists and is empty.
/// In our implementation, we indeed move the underlying mailbox
/// to the new name (i.e. the new name has the same id as the previous
/// INBOX), and we create a new empty mailbox for INBOX.
pub const INBOX: &str = "INBOX";

/// For convenience purpose, we also create some special mailbox
/// that are described in RFC6154 SPECIAL-USE
/// @FIXME maybe it should be a configuration parameter
/// @FIXME maybe we should have a per-mailbox flag mechanism, either an enum or a string, so we
/// track which mailbox is used for what.
/// @FIXME Junk could be useful but we don't have any antispam solution yet so...
/// @FIXME IMAP supports virtual mailbox. \All or \Flagged are intended to be virtual mailboxes.
/// \Trash might be one, or not one. I don't know what we should do there.
pub const DRAFTS: &str = "Drafts";
pub const ARCHIVE: &str = "Archive";
pub const SENT: &str = "Sent";
pub const TRASH: &str = "Trash";

pub(crate) const MAILBOX_LIST_PK: &str = "mailboxes";
pub(crate) const MAILBOX_LIST_SK: &str = "list";

// ---- User's mailbox list (serialized in K2V) ----

#[derive(Serialize, Deserialize)]
pub(crate) struct MailboxList(BTreeMap<String, MailboxListEntry>);

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub(crate) struct MailboxListEntry {
    id_lww: (u64, Option<UniqueIdent>),
    uidvalidity: ImapUidvalidity,
}

impl MailboxListEntry {
    fn merge(&mut self, other: &Self) {
        // Simple CRDT merge rule
        if other.id_lww.0 > self.id_lww.0
            || (other.id_lww.0 == self.id_lww.0 && other.id_lww.1 > self.id_lww.1)
        {
            self.id_lww = other.id_lww;
        }
        self.uidvalidity = std::cmp::max(self.uidvalidity, other.uidvalidity);
    }
}

impl MailboxList {
    pub(crate) fn new() -> Self {
        Self(BTreeMap::new())
    }

    pub(crate) fn merge(&mut self, list2: Self) {
        for (k, v) in list2.0.into_iter() {
            if let Some(e) = self.0.get_mut(&k) {
                e.merge(&v);
            } else {
                self.0.insert(k, v);
            }
        }
    }

    pub(crate) fn existing_mailbox_names(&self) -> Vec<String> {
        self.0
            .iter()
            .filter(|(_, v)| v.id_lww.1.is_some())
            .map(|(k, _)| k.to_string())
            .collect()
    }

    pub(crate) fn has_mailbox(&self, name: &str) -> bool {
        matches!(
            self.0.get(name),
            Some(MailboxListEntry {
                id_lww: (_, Some(_)),
                ..
            })
        )
    }

    pub(crate) fn get_mailbox(&self, name: &str) -> Option<(ImapUidvalidity, Option<UniqueIdent>)> {
        self.0.get(name).map(
            |MailboxListEntry {
                 id_lww: (_, mailbox_id),
                 uidvalidity,
             }| (*uidvalidity, *mailbox_id),
        )
    }

    /// Ensures mailbox `name` maps to id `id`.
    /// If it already mapped to that, returns None.
    /// If a change had to be done, returns Some(new uidvalidity in mailbox).
    pub(crate) fn set_mailbox(
        &mut self,
        name: &str,
        id: Option<UniqueIdent>,
    ) -> Option<ImapUidvalidity> {
        let (ts, id, uidvalidity) = match self.0.get_mut(name) {
            None => {
                if id.is_none() {
                    return None;
                } else {
                    (now_msec(), id, ImapUidvalidity::new(1).unwrap())
                }
            }
            Some(MailboxListEntry {
                id_lww,
                uidvalidity,
            }) => {
                if id_lww.1 == id {
                    return None;
                } else {
                    (
                        std::cmp::max(id_lww.0 + 1, now_msec()),
                        id,
                        ImapUidvalidity::new(uidvalidity.get() + 1).unwrap(),
                    )
                }
            }
        };

        self.0.insert(
            name.into(),
            MailboxListEntry {
                id_lww: (ts, id),
                uidvalidity,
            },
        );
        Some(uidvalidity)
    }

    pub(crate) fn update_uidvalidity(&mut self, name: &str, new_uidvalidity: ImapUidvalidity) {
        match self.0.get_mut(name) {
            None => {
                self.0.insert(
                    name.into(),
                    MailboxListEntry {
                        id_lww: (now_msec(), None),
                        uidvalidity: new_uidvalidity,
                    },
                );
            }
            Some(MailboxListEntry { uidvalidity, .. }) => {
                *uidvalidity = std::cmp::max(*uidvalidity, new_uidvalidity);
            }
        }
    }

    pub(crate) fn create_mailbox(&mut self, name: &str) -> CreatedMailbox {
        if let Some(MailboxListEntry {
            id_lww: (_, Some(id)),
            uidvalidity,
        }) = self.0.get(name)
        {
            return CreatedMailbox::Existed(*id, *uidvalidity);
        }

        let id = gen_ident();
        let uidvalidity = self.set_mailbox(name, Some(id)).unwrap();
        CreatedMailbox::Created(id, uidvalidity)
    }

    pub(crate) fn rename_mailbox(&mut self, old_name: &str, new_name: &str) -> Result<()> {
        if let Some((uidvalidity, Some(mbid))) = self.get_mailbox(old_name) {
            if self.has_mailbox(new_name) {
                bail!(
                    "Cannot rename {} into {}: {} already exists",
                    old_name,
                    new_name,
                    new_name
                );
            }

            self.set_mailbox(old_name, None);
            self.set_mailbox(new_name, Some(mbid));
            self.update_uidvalidity(new_name, uidvalidity);
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

pub(crate) enum CreatedMailbox {
    Created(UniqueIdent, ImapUidvalidity),
    Existed(UniqueIdent, ImapUidvalidity),
}
