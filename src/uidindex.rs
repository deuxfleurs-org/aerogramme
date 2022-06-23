use std::num::NonZeroU32;

use im::{HashMap, HashSet, OrdMap, OrdSet};
use serde::{de::Error, Deserialize, Deserializer, Serialize, Serializer};

use crate::bayou::*;
use crate::mail_ident::MailIdent;

pub type ImapUid = NonZeroU32;
pub type ImapUidvalidity = NonZeroU32;
pub type Flag = String;

#[derive(Clone)]
/// A UidIndex handles the mutable part of a mailbox
/// It is built by running the event log on it
/// Each applied log generates a new UidIndex by cloning the previous one
/// and applying the event. This is why we use immutable datastructures:
/// they are cheap to clone.
pub struct UidIndex {
    // Source of trust
    pub table: OrdMap<MailIdent, (ImapUid, Vec<Flag>)>,

    // Indexes optimized for queries
    pub idx_by_uid: OrdMap<ImapUid, MailIdent>,
    pub idx_by_flag: FlagIndex,

    // Counters
    pub uidvalidity: ImapUidvalidity,
    pub uidnext: ImapUid,
    pub internalseq: ImapUid,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum UidIndexOp {
    MailAdd(MailIdent, ImapUid, Vec<Flag>),
    MailDel(MailIdent),
    FlagAdd(MailIdent, Vec<Flag>),
    FlagDel(MailIdent, Vec<Flag>),
}

impl UidIndex {
    #[must_use]
    pub fn op_mail_add(&self, ident: MailIdent, flags: Vec<Flag>) -> UidIndexOp {
        UidIndexOp::MailAdd(ident, self.internalseq, flags)
    }

    #[must_use]
    pub fn op_mail_del(&self, ident: MailIdent) -> UidIndexOp {
        UidIndexOp::MailDel(ident)
    }

    #[must_use]
    pub fn op_flag_add(&self, ident: MailIdent, flags: Vec<Flag>) -> UidIndexOp {
        UidIndexOp::FlagAdd(ident, flags)
    }

    #[must_use]
    pub fn op_flag_del(&self, ident: MailIdent, flags: Vec<Flag>) -> UidIndexOp {
        UidIndexOp::FlagDel(ident, flags)
    }

    // INTERNAL functions to keep state consistent

    fn reg_email(&mut self, ident: MailIdent, uid: ImapUid, flags: &Vec<Flag>) {
        // Insert the email in our table
        self.table.insert(ident, (uid, flags.clone()));

        // Update the indexes/caches
        self.idx_by_uid.insert(uid, ident);
        self.idx_by_flag.insert(uid, flags);
    }

    fn unreg_email(&mut self, ident: &MailIdent) {
        // We do nothing if the mail does not exist
        let (uid, flags) = match self.table.get(ident) {
            Some(v) => v,
            None => return,
        };

        // Delete all cache entries
        self.idx_by_uid.remove(uid);
        self.idx_by_flag.remove(*uid, flags);

        // Remove from source of trust
        self.table.remove(ident);
    }
}

impl Default for UidIndex {
    fn default() -> Self {
        Self {
            table: OrdMap::new(),
            idx_by_uid: OrdMap::new(),
            idx_by_flag: FlagIndex::new(),
            uidvalidity: NonZeroU32::new(1).unwrap(),
            uidnext: NonZeroU32::new(1).unwrap(),
            internalseq: NonZeroU32::new(1).unwrap(),
        }
    }
}

impl BayouState for UidIndex {
    type Op = UidIndexOp;

    fn apply(&self, op: &UidIndexOp) -> Self {
        let mut new = self.clone();
        match op {
            UidIndexOp::MailAdd(ident, uid, flags) => {
                // Change UIDValidity if there is a conflict
                if *uid < new.internalseq {
                    new.uidvalidity =
                        NonZeroU32::new(new.uidvalidity.get() + new.internalseq.get() - uid.get())
                            .unwrap();
                }

                // Assign the real uid of the email
                let new_uid = new.internalseq;

                // Delete the previous entry if any.
                // Our proof has no assumption on `ident` uniqueness,
                // so we must handle this case even it is very unlikely
                // In this case, we overwrite the email.
                // Note: assigning a new UID is mandatory.
                new.unreg_email(ident);

                // We record our email and update ou caches
                new.reg_email(*ident, new_uid, flags);

                // Update counters
                new.internalseq = NonZeroU32::new(new.internalseq.get() + 1).unwrap();
                new.uidnext = new.internalseq;
            }
            UidIndexOp::MailDel(ident) => {
                // If the email is known locally, we remove its references in all our indexes
                new.unreg_email(ident);

                // We update the counter
                new.internalseq = NonZeroU32::new(new.internalseq.get() + 1).unwrap();
            }
            UidIndexOp::FlagAdd(ident, new_flags) => {
                if let Some((uid, existing_flags)) = new.table.get_mut(ident) {
                    // Add flags to the source of trust and the cache
                    let mut to_add: Vec<Flag> = new_flags
                        .iter()
                        .filter(|f| !existing_flags.contains(f))
                        .cloned()
                        .collect();
                    new.idx_by_flag.insert(*uid, &to_add);
                    existing_flags.append(&mut to_add);
                }
            }
            UidIndexOp::FlagDel(ident, rm_flags) => {
                if let Some((uid, existing_flags)) = new.table.get_mut(ident) {
                    // Remove flags from the source of trust and the cache
                    existing_flags.retain(|x| !rm_flags.contains(x));
                    new.idx_by_flag.remove(*uid, rm_flags);
                }
            }
        }
        new
    }
}

// ---- FlagIndex implementation ----
#[derive(Clone)]
pub struct FlagIndex(HashMap<Flag, OrdSet<ImapUid>>);

impl FlagIndex {
    fn new() -> Self {
        Self(HashMap::new())
    }
    fn insert(&mut self, uid: ImapUid, flags: &Vec<Flag>) {
        flags.iter().for_each(|flag| {
            self.0
                .entry(flag.clone())
                .or_insert(OrdSet::new())
                .insert(uid);
        });
    }
    fn remove(&mut self, uid: ImapUid, flags: &Vec<Flag>) -> () {
        flags.iter().for_each(|flag| {
            self.0.get_mut(flag).and_then(|set| set.remove(&uid));
        });
    }
}

// ---- CUSTOM SERIALIZATION AND DESERIALIZATION ----

#[derive(Serialize, Deserialize)]
struct UidIndexSerializedRepr {
    mails: Vec<(ImapUid, MailIdent, Vec<Flag>)>,
    uidvalidity: ImapUidvalidity,
    uidnext: ImapUid,
    internalseq: ImapUid,
}

impl<'de> Deserialize<'de> for UidIndex {
    fn deserialize<D>(d: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let val: UidIndexSerializedRepr = UidIndexSerializedRepr::deserialize(d)?;

        let mut uidindex = UidIndex {
            table: OrdMap::new(),
            idx_by_uid: OrdMap::new(),
            idx_by_flag: FlagIndex::new(),
            uidvalidity: val.uidvalidity,
            uidnext: val.uidnext,
            internalseq: val.internalseq,
        };

        val.mails
            .iter()
            .for_each(|(u, i, f)| uidindex.reg_email(*i, *u, f));

        Ok(uidindex)
    }
}

impl Serialize for UidIndex {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut mails = vec![];
        for (ident, (uid, flags)) in self.table.iter() {
            mails.push((*uid, *ident, flags.clone()));
        }

        let val = UidIndexSerializedRepr {
            mails,
            uidvalidity: self.uidvalidity,
            uidnext: self.uidnext,
            internalseq: self.internalseq,
        };

        val.serialize(serializer)
    }
}

// ---- TESTS ----

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uidindex() {
        let mut state = UidIndex::default();

        // Add message 1
        {
            let m = MailIdent([0x01; 24]);
            let f = vec!["\\Recent".to_string(), "\\Archive".to_string()];
            let ev = state.op_mail_add(m, f);
            state = state.apply(&ev);

            // Early checks
            assert_eq!(state.table.len(), 1);
            let (uid, flags) = state.table.get(&m).unwrap();
            assert_eq!(*uid, 1);
            assert_eq!(flags.len(), 2);
            let ident = state.idx_by_uid.get(&1).unwrap();
            assert_eq!(&m, ident);
            let recent = state.idx_by_flag.0.get("\\Recent").unwrap();
            assert_eq!(recent.len(), 1);
            assert_eq!(recent.iter().next().unwrap(), &1);
            assert_eq!(state.uidnext, 2);
            assert_eq!(state.uidvalidity, 1);
        }

        // Add message 2
        {
            let m = MailIdent([0x02; 24]);
            let f = vec!["\\Seen".to_string(), "\\Archive".to_string()];
            let ev = state.op_mail_add(m, f);
            state = state.apply(&ev);

            let archive = state.idx_by_flag.0.get("\\Archive").unwrap();
            assert_eq!(archive.len(), 2);
        }

        // Add flags to message 1
        {
            let m = MailIdent([0x01; 24]);
            let f = vec!["Important".to_string(), "$cl_1".to_string()];
            let ev = state.op_flag_add(m, f);
            state = state.apply(&ev);
        }

        // Delete flags from message 1
        {
            let m = MailIdent([0x01; 24]);
            let f = vec!["\\Recent".to_string()];
            let ev = state.op_flag_del(m, f);
            state = state.apply(&ev);

            let archive = state.idx_by_flag.0.get("\\Archive").unwrap();
            assert_eq!(archive.len(), 2);
        }

        // Delete message 2
        {
            let m = MailIdent([0x02; 24]);
            let ev = state.op_mail_del(m);
            state = state.apply(&ev);

            let archive = state.idx_by_flag.0.get("\\Archive").unwrap();
            assert_eq!(archive.len(), 1);
        }

        // Add a message 3 concurrent to message 1 (trigger a uid validity change)
        {
            let m = MailIdent([0x03; 24]);
            let f = vec!["\\Archive".to_string(), "\\Recent".to_string()];
            let ev = UidIndexOp::MailAdd(m, 1, f);
            state = state.apply(&ev);
        }

        // Checks
        {
            assert_eq!(state.table.len(), 2);
            assert!(state.uidvalidity > 1);

            let (last_uid, ident) = state.idx_by_uid.get_max().unwrap();
            assert_eq!(ident, &MailIdent([0x03; 24]));

            let archive = state.idx_by_flag.0.get("\\Archive").unwrap();
            assert_eq!(archive.len(), 2);
            let mut iter = archive.iter();
            assert_eq!(iter.next().unwrap(), &1);
            assert_eq!(iter.next().unwrap(), last_uid);
        }
    }
}
