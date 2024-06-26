use std::num::{NonZeroU32, NonZeroU64};

use im::{HashMap, OrdMap, OrdSet};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::unique_ident::UniqueIdent;
use aero_bayou::*;

pub type ModSeq = NonZeroU64;
pub type ImapUid = NonZeroU32;
pub type ImapUidvalidity = NonZeroU32;
pub type Flag = String;
pub type IndexEntry = (ImapUid, ModSeq, Vec<Flag>);

/// A UidIndex handles the mutable part of a mailbox
/// It is built by running the event log on it
/// Each applied log generates a new UidIndex by cloning the previous one
/// and applying the event. This is why we use immutable datastructures:
/// they are cheap to clone.
#[derive(Clone)]
pub struct UidIndex {
    // Source of trust
    pub table: OrdMap<UniqueIdent, IndexEntry>,

    // Indexes optimized for queries
    pub idx_by_uid: OrdMap<ImapUid, UniqueIdent>,
    pub idx_by_modseq: OrdMap<ModSeq, UniqueIdent>,
    pub idx_by_flag: FlagIndex,

    // "Public" Counters
    pub uidvalidity: ImapUidvalidity,
    pub uidnext: ImapUid,
    pub highestmodseq: ModSeq,

    // "Internal" Counters
    pub internalseq: ImapUid,
    pub internalmodseq: ModSeq,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum UidIndexOp {
    MailAdd(UniqueIdent, ImapUid, ModSeq, Vec<Flag>),
    MailDel(UniqueIdent),
    FlagAdd(UniqueIdent, ModSeq, Vec<Flag>),
    FlagDel(UniqueIdent, ModSeq, Vec<Flag>),
    FlagSet(UniqueIdent, ModSeq, Vec<Flag>),
    BumpUidvalidity(u32),
}

impl UidIndex {
    #[must_use]
    pub fn op_mail_add(&self, ident: UniqueIdent, flags: Vec<Flag>) -> UidIndexOp {
        UidIndexOp::MailAdd(ident, self.internalseq, self.internalmodseq, flags)
    }

    #[must_use]
    pub fn op_mail_del(&self, ident: UniqueIdent) -> UidIndexOp {
        UidIndexOp::MailDel(ident)
    }

    #[must_use]
    pub fn op_flag_add(&self, ident: UniqueIdent, flags: Vec<Flag>) -> UidIndexOp {
        UidIndexOp::FlagAdd(ident, self.internalmodseq, flags)
    }

    #[must_use]
    pub fn op_flag_del(&self, ident: UniqueIdent, flags: Vec<Flag>) -> UidIndexOp {
        UidIndexOp::FlagDel(ident, self.internalmodseq, flags)
    }

    #[must_use]
    pub fn op_flag_set(&self, ident: UniqueIdent, flags: Vec<Flag>) -> UidIndexOp {
        UidIndexOp::FlagSet(ident, self.internalmodseq, flags)
    }

    #[must_use]
    pub fn op_bump_uidvalidity(&self, count: u32) -> UidIndexOp {
        UidIndexOp::BumpUidvalidity(count)
    }

    // INTERNAL functions to keep state consistent

    fn reg_email(&mut self, ident: UniqueIdent, uid: ImapUid, modseq: ModSeq, flags: &[Flag]) {
        // Insert the email in our table
        self.table.insert(ident, (uid, modseq, flags.to_owned()));

        // Update the indexes/caches
        self.idx_by_uid.insert(uid, ident);
        self.idx_by_flag.insert(uid, flags);
        self.idx_by_modseq.insert(modseq, ident);
    }

    fn unreg_email(&mut self, ident: &UniqueIdent) {
        // We do nothing if the mail does not exist
        let (uid, modseq, flags) = match self.table.get(ident) {
            Some(v) => v,
            None => return,
        };

        // Delete all cache entries
        self.idx_by_uid.remove(uid);
        self.idx_by_flag.remove(*uid, flags);
        self.idx_by_modseq.remove(modseq);

        // Remove from source of trust
        self.table.remove(ident);
    }
}

impl Default for UidIndex {
    fn default() -> Self {
        Self {
            table: OrdMap::new(),

            idx_by_uid: OrdMap::new(),
            idx_by_modseq: OrdMap::new(),
            idx_by_flag: FlagIndex::new(),

            uidvalidity: NonZeroU32::new(1).unwrap(),
            uidnext: NonZeroU32::new(1).unwrap(),
            highestmodseq: NonZeroU64::new(1).unwrap(),

            internalseq: NonZeroU32::new(1).unwrap(),
            internalmodseq: NonZeroU64::new(1).unwrap(),
        }
    }
}

impl BayouState for UidIndex {
    type Op = UidIndexOp;

    fn apply(&self, op: &UidIndexOp) -> Self {
        let mut new = self.clone();
        match op {
            UidIndexOp::MailAdd(ident, uid, modseq, flags) => {
                // Change UIDValidity if there is a UID conflict or a MODSEQ conflict
                // @FIXME Need to prove that summing work
                // The intuition: we increase the UIDValidity by the number of possible conflicts
                if *uid < new.internalseq || *modseq < new.internalmodseq {
                    let bump_uid = new.internalseq.get() - uid.get();
                    let bump_modseq = (new.internalmodseq.get() - modseq.get()) as u32;
                    new.uidvalidity =
                        NonZeroU32::new(new.uidvalidity.get() + bump_uid + bump_modseq).unwrap();
                }

                // Assign the real uid of the email
                let new_uid = new.internalseq;

                // Assign the real modseq of the email and its new flags
                let new_modseq = new.internalmodseq;

                // Delete the previous entry if any.
                // Our proof has no assumption on `ident` uniqueness,
                // so we must handle this case even it is very unlikely
                // In this case, we overwrite the email.
                // Note: assigning a new UID is mandatory.
                new.unreg_email(ident);

                // We record our email and update ou caches
                new.reg_email(*ident, new_uid, new_modseq, flags);

                // Update counters
                new.highestmodseq = new.internalmodseq;

                new.internalseq = NonZeroU32::new(new.internalseq.get() + 1).unwrap();
                new.internalmodseq = NonZeroU64::new(new.internalmodseq.get() + 1).unwrap();

                new.uidnext = new.internalseq;
            }
            UidIndexOp::MailDel(ident) => {
                // If the email is known locally, we remove its references in all our indexes
                new.unreg_email(ident);

                // We update the counter
                new.internalseq = NonZeroU32::new(new.internalseq.get() + 1).unwrap();
            }
            UidIndexOp::FlagAdd(ident, candidate_modseq, new_flags) => {
                if let Some((uid, email_modseq, existing_flags)) = new.table.get_mut(ident) {
                    // Bump UIDValidity if required
                    if *candidate_modseq < new.internalmodseq {
                        let bump_modseq =
                            (new.internalmodseq.get() - candidate_modseq.get()) as u32;
                        new.uidvalidity =
                            NonZeroU32::new(new.uidvalidity.get() + bump_modseq).unwrap();
                    }

                    // Add flags to the source of trust and the cache
                    let mut to_add: Vec<Flag> = new_flags
                        .iter()
                        .filter(|f| !existing_flags.contains(f))
                        .cloned()
                        .collect();
                    new.idx_by_flag.insert(*uid, &to_add);
                    *email_modseq = new.internalmodseq;
                    new.idx_by_modseq.insert(new.internalmodseq, *ident);
                    existing_flags.append(&mut to_add);

                    // Update counters
                    new.highestmodseq = new.internalmodseq;
                    new.internalmodseq = NonZeroU64::new(new.internalmodseq.get() + 1).unwrap();
                }
            }
            UidIndexOp::FlagDel(ident, candidate_modseq, rm_flags) => {
                if let Some((uid, email_modseq, existing_flags)) = new.table.get_mut(ident) {
                    // Bump UIDValidity if required
                    if *candidate_modseq < new.internalmodseq {
                        let bump_modseq =
                            (new.internalmodseq.get() - candidate_modseq.get()) as u32;
                        new.uidvalidity =
                            NonZeroU32::new(new.uidvalidity.get() + bump_modseq).unwrap();
                    }

                    // Remove flags from the source of trust and the cache
                    existing_flags.retain(|x| !rm_flags.contains(x));
                    new.idx_by_flag.remove(*uid, rm_flags);

                    // Register that email has been modified
                    new.idx_by_modseq.insert(new.internalmodseq, *ident);
                    *email_modseq = new.internalmodseq;

                    // Update counters
                    new.highestmodseq = new.internalmodseq;
                    new.internalmodseq = NonZeroU64::new(new.internalmodseq.get() + 1).unwrap();
                }
            }
            UidIndexOp::FlagSet(ident, candidate_modseq, new_flags) => {
                if let Some((uid, email_modseq, existing_flags)) = new.table.get_mut(ident) {
                    // Bump UIDValidity if required
                    if *candidate_modseq < new.internalmodseq {
                        let bump_modseq =
                            (new.internalmodseq.get() - candidate_modseq.get()) as u32;
                        new.uidvalidity =
                            NonZeroU32::new(new.uidvalidity.get() + bump_modseq).unwrap();
                    }

                    // Remove flags from the source of trust and the cache
                    let (keep_flags, rm_flags): (Vec<String>, Vec<String>) = existing_flags
                        .iter()
                        .cloned()
                        .partition(|x| new_flags.contains(x));
                    *existing_flags = keep_flags;
                    let mut to_add: Vec<Flag> = new_flags
                        .iter()
                        .filter(|f| !existing_flags.contains(f))
                        .cloned()
                        .collect();
                    existing_flags.append(&mut to_add);
                    new.idx_by_flag.remove(*uid, &rm_flags);
                    new.idx_by_flag.insert(*uid, &to_add);

                    // Register that email has been modified
                    new.idx_by_modseq.insert(new.internalmodseq, *ident);
                    *email_modseq = new.internalmodseq;

                    // Update counters
                    new.highestmodseq = new.internalmodseq;
                    new.internalmodseq = NonZeroU64::new(new.internalmodseq.get() + 1).unwrap();
                }
            }
            UidIndexOp::BumpUidvalidity(count) => {
                new.uidvalidity = ImapUidvalidity::new(new.uidvalidity.get() + *count)
                    .unwrap_or(ImapUidvalidity::new(u32::MAX).unwrap());
            }
        }
        new
    }
}

// ---- FlagIndex implementation ----

#[derive(Clone)]
pub struct FlagIndex(HashMap<Flag, OrdSet<ImapUid>>);
pub type FlagIter<'a> = im::hashmap::Keys<'a, Flag, OrdSet<ImapUid>>;

impl FlagIndex {
    fn new() -> Self {
        Self(HashMap::new())
    }
    fn insert(&mut self, uid: ImapUid, flags: &[Flag]) {
        flags.iter().for_each(|flag| {
            self.0
                .entry(flag.clone())
                .or_insert(OrdSet::new())
                .insert(uid);
        });
    }
    fn remove(&mut self, uid: ImapUid, flags: &[Flag]) {
        for flag in flags.iter() {
            if let Some(set) = self.0.get_mut(flag) {
                set.remove(&uid);
                if set.is_empty() {
                    self.0.remove(flag);
                }
            }
        }
    }

    pub fn get(&self, f: &Flag) -> Option<&OrdSet<ImapUid>> {
        self.0.get(f)
    }

    pub fn flags(&self) -> FlagIter {
        self.0.keys()
    }
}

// ---- CUSTOM SERIALIZATION AND DESERIALIZATION ----

#[derive(Serialize, Deserialize)]
struct UidIndexSerializedRepr {
    mails: Vec<(ImapUid, ModSeq, UniqueIdent, Vec<Flag>)>,

    uidvalidity: ImapUidvalidity,
    uidnext: ImapUid,
    highestmodseq: ModSeq,

    internalseq: ImapUid,
    internalmodseq: ModSeq,
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
            idx_by_modseq: OrdMap::new(),
            idx_by_flag: FlagIndex::new(),

            uidvalidity: val.uidvalidity,
            uidnext: val.uidnext,
            highestmodseq: val.highestmodseq,

            internalseq: val.internalseq,
            internalmodseq: val.internalmodseq,
        };

        val.mails
            .iter()
            .for_each(|(uid, modseq, uuid, flags)| uidindex.reg_email(*uuid, *uid, *modseq, flags));

        Ok(uidindex)
    }
}

impl Serialize for UidIndex {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut mails = vec![];
        for (ident, (uid, modseq, flags)) in self.table.iter() {
            mails.push((*uid, *modseq, *ident, flags.clone()));
        }

        let val = UidIndexSerializedRepr {
            mails,
            uidvalidity: self.uidvalidity,
            uidnext: self.uidnext,
            highestmodseq: self.highestmodseq,
            internalseq: self.internalseq,
            internalmodseq: self.internalmodseq,
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
            let m = UniqueIdent([0x01; 24]);
            let f = vec!["\\Recent".to_string(), "\\Archive".to_string()];
            let ev = state.op_mail_add(m, f);
            state = state.apply(&ev);

            // Early checks
            assert_eq!(state.table.len(), 1);
            let (uid, modseq, flags) = state.table.get(&m).unwrap();
            assert_eq!(*uid, NonZeroU32::new(1).unwrap());
            assert_eq!(*modseq, NonZeroU64::new(1).unwrap());
            assert_eq!(flags.len(), 2);
            let ident = state.idx_by_uid.get(&NonZeroU32::new(1).unwrap()).unwrap();
            assert_eq!(&m, ident);
            let recent = state.idx_by_flag.0.get("\\Recent").unwrap();
            assert_eq!(recent.len(), 1);
            assert_eq!(recent.iter().next().unwrap(), &NonZeroU32::new(1).unwrap());
            assert_eq!(state.uidnext, NonZeroU32::new(2).unwrap());
            assert_eq!(state.uidvalidity, NonZeroU32::new(1).unwrap());
        }

        // Add message 2
        {
            let m = UniqueIdent([0x02; 24]);
            let f = vec!["\\Seen".to_string(), "\\Archive".to_string()];
            let ev = state.op_mail_add(m, f);
            state = state.apply(&ev);

            let archive = state.idx_by_flag.0.get("\\Archive").unwrap();
            assert_eq!(archive.len(), 2);
        }

        // Add flags to message 1
        {
            let m = UniqueIdent([0x01; 24]);
            let f = vec!["Important".to_string(), "$cl_1".to_string()];
            let ev = state.op_flag_add(m, f);
            state = state.apply(&ev);
        }

        // Delete flags from message 1
        {
            let m = UniqueIdent([0x01; 24]);
            let f = vec!["\\Recent".to_string()];
            let ev = state.op_flag_del(m, f);
            state = state.apply(&ev);

            let archive = state.idx_by_flag.0.get("\\Archive").unwrap();
            assert_eq!(archive.len(), 2);
        }

        // Delete message 2
        {
            let m = UniqueIdent([0x02; 24]);
            let ev = state.op_mail_del(m);
            state = state.apply(&ev);

            let archive = state.idx_by_flag.0.get("\\Archive").unwrap();
            assert_eq!(archive.len(), 1);
        }

        // Add a message 3 concurrent to message 1 (trigger a uid validity change)
        {
            let m = UniqueIdent([0x03; 24]);
            let f = vec!["\\Archive".to_string(), "\\Recent".to_string()];
            let ev = UidIndexOp::MailAdd(
                m,
                NonZeroU32::new(1).unwrap(),
                NonZeroU64::new(1).unwrap(),
                f,
            );
            state = state.apply(&ev);
        }

        // Checks
        {
            assert_eq!(state.table.len(), 2);
            assert!(state.uidvalidity > NonZeroU32::new(1).unwrap());

            let (last_uid, ident) = state.idx_by_uid.get_max().unwrap();
            assert_eq!(ident, &UniqueIdent([0x03; 24]));

            let archive = state.idx_by_flag.0.get("\\Archive").unwrap();
            assert_eq!(archive.len(), 2);
            let mut iter = archive.iter();
            assert_eq!(iter.next().unwrap(), &NonZeroU32::new(1).unwrap());
            assert_eq!(iter.next().unwrap(), last_uid);
        }
    }
}
