use std::collections::BTreeSet;
use std::num::{NonZeroU32, NonZeroU64};

use im::{HashMap, OrdMap, OrdSet, Vector};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::unique_ident::UniqueIdent;
use aero_bayou::*;

pub type ModSeq = NonZeroU64;
pub type ImapSeqid = NonZeroU32;
pub type ImapUid = NonZeroU32;
pub type ImapUidvalidity = NonZeroU32;
pub type Flag = String;
pub type Flags = BTreeSet<Flag>;
pub type IndexEntry = (ImapUid, ModSeq, Flags);
pub type InternalSeq = u32;
pub type InternalModSeq = u64;

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
    pub idx_by_seqid: SeqidMap<UniqueIdent>,
    // FIXME: can we remove this index which is somewhat expensive to maintain?
    // it may be easier after refactoring SEARCH
    pub idx_seqid_of_uuid: OrdMap<UniqueIdent, ImapSeqid>,

    // "Public" Counters
    pub uidvalidity: ImapUidvalidity,

    // "Internal" Counters

    // `internalseq` counts the number of *added emails*: it is equal to
    // count(MailAdd commands).
    //
    // This has two purposes:
    // - generate mail UIDs
    // - detect conflicts between concurrent MailAdd commands.
    //
    // NOTE: we do not count MailDel commands. This is an optimization to reduce
    // uidvalidity changes, which relies on the assumption that an email is
    // never added twice to the mailbox with the same UniqueIdent. The code in
    // `mailbox.rs` (for append, copy, move) ensures that this assumption always
    // holds.
    //
    // Reasoning: consider a MailDel operation. Bumping `internalseq` causes
    // later MailAdd operations to be replayed with different `ImapUid`s.
    // However, if we know that there is only one MailAdd possible for the same
    // `UniqueIdent`, then either:
    // - it occurs before the MailDel (and is not replayed),
    // - it occurs after the MailDel, and thus the deletion is a no-op.
    //
    // In both cases, there is no actual `ImapUid` conflict: it is safe to keep
    // `ImapUid`s as they were, and thus no need to bump `internalseq` and
    // `uidvalidity`.
    internalseq: InternalSeq,

    // `internalmodseq` counts the number of added mails and modifications to
    // mail flags in the entire mailbox: it is equal to count(MailAdd commands)
    // + count(Flag{Add,Del,Set} commands).
    //
    // This is used to implement RFC4551 (CONDSTORE). It also serves two purposes:
    // - generate MODSEQ numbers for emails
    // - detect conflicts between concurrent Flag{Add,Del,Set} commands.
    internalmodseq: InternalModSeq,
}

/// A map where keys are sequence IDs. Sequence IDs are non-zero integers.
// Internally, we store the value of sequence ID `i` at offset `i-1` in the vector.
#[derive(Clone)]
pub struct SeqidMap<T>(Vector<T>);

impl<T: Clone> SeqidMap<T> {
    pub fn new() -> Self {
        Self(Vector::new())
    }

    pub fn get(&self, seqid: NonZeroU32) -> Option<&T> {
        self.0.get(seqid.get() as usize - 1)
    }

    pub fn push(&mut self, x: T) {
        self.0.push_back(x)
    }

    pub fn remove(&mut self, seqid: NonZeroU32) {
        self.0.remove(seqid.get() as usize - 1);
    }

    pub fn next_seqid(&self) -> NonZeroU32 {
        NonZeroU32::try_from(self.0.len() as u32 + 1).unwrap()
    }

    pub fn largest(&self) -> Option<(NonZeroU32, &T)> {
        self.0.last().map(|x| {
            let id = NonZeroU32::try_from(self.0.len() as u32).unwrap();
            (id, x)
        })
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum UidIndexOp {
    MailAdd(UniqueIdent, InternalSeq, InternalModSeq, Flags),
    MailDel(UniqueIdent),
    FlagAdd(UniqueIdent, InternalModSeq, Flags),
    FlagDel(UniqueIdent, InternalModSeq, Flags),
    FlagSet(UniqueIdent, InternalModSeq, Flags),
}

impl UidIndex {
    /// It is recommended for performance (but not required for safety) to use
    /// idents that increase when adding new mails (i.e. `ident` should be
    /// higher than the ones used for earlier calls to `op_mail_add`.
    #[must_use]
    pub fn op_mail_add(&self, ident: UniqueIdent, flags: Flags) -> UidIndexOp {
        UidIndexOp::MailAdd(ident, self.internalseq, self.internalmodseq, flags)
    }

    #[must_use]
    pub fn op_mail_del(&self, ident: UniqueIdent) -> UidIndexOp {
        UidIndexOp::MailDel(ident)
    }

    #[must_use]
    pub fn op_flag_add(&self, ident: UniqueIdent, flags: Flags) -> UidIndexOp {
        UidIndexOp::FlagAdd(ident, self.internalmodseq, flags)
    }

    #[must_use]
    pub fn op_flag_del(&self, ident: UniqueIdent, flags: Flags) -> UidIndexOp {
        UidIndexOp::FlagDel(ident, self.internalmodseq, flags)
    }

    #[must_use]
    pub fn op_flag_set(&self, ident: UniqueIdent, flags: Flags) -> UidIndexOp {
        UidIndexOp::FlagSet(ident, self.internalmodseq, flags)
    }

    pub fn uidnext(&self) -> ImapUid {
        Self::uidnext_of_internal(self.internalseq)
    }

    pub fn highestmodseq(&self) -> ModSeq {
        Self::highestmodseq_of_internal(self.internalmodseq)
    }

    fn uidnext_of_internal(internalseq: u32) -> ImapUid {
        // UIDNEXT is internalseq + 1.
        //
        // Mail UIDs start at 1, and internalseq (count(MailAdd)) is 0 on an
        // empty mailbox => UIDNEXT=1 initially.
        NonZeroU32::try_from(internalseq + 1).unwrap()
    }

    fn highestmodseq_of_internal(internalmodseq: u64) -> ModSeq {
        // HIGHESTMODSEQ is internalmodseq + 1
        //
        // NOTE: the reasoning is different than for UIDNEXT. (UIDNEXT refers to
        // the *next* unallocated UID, while HIGHESTMODSEQ refers to the maximum
        // *currently* allocated ModSeq.) HIGHESTMODSEQ is defined as
        // `internalmodseq` + 1 because it needs to be >= 1 according to the RFC
        // definitions. This means that we get HIGHESTMODSEQ=1 on an empty
        // mailbox, and the first ModSeq assigned to an email is *2*. (I.e. if
        // we had a MODSEQNEXT counter, we would MODSEQNEXT=2 initially.)
        NonZeroU64::try_from(internalmodseq + 1).unwrap()
    }

    // INTERNAL functions to keep state consistent

    fn reg_email(&mut self, ident: UniqueIdent, uid: ImapUid, modseq: ModSeq, flags: &Flags) {
        // Insert the email in our table
        self.table.insert(ident, (uid, modseq, flags.clone()));

        // Update the indexes/caches
        self.idx_by_uid.insert(uid, ident);
        self.idx_by_flag.insert(uid, flags);
        self.idx_by_modseq.insert(modseq, ident);
        let next_seqid = self.idx_by_seqid.next_seqid();
        self.idx_by_seqid.push(ident);
        self.idx_seqid_of_uuid.insert(ident, next_seqid);
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
        let seqid = self.idx_seqid_of_uuid.remove(ident).unwrap();
        self.idx_by_seqid.remove(seqid);
        // we need to update all indexed seqids starting from this one in idx_seqid_of_uuid
        for id in seqid.get()..self.idx_by_seqid.next_seqid().get() {
            let id = NonZeroU32::try_from(id).unwrap();
            let uuid = self.idx_by_seqid.get(id).unwrap();
            self.idx_seqid_of_uuid.insert(*uuid, id);
        }

        // Remove from source of trust
        self.table.remove(ident);
    }

    // Can be useful to debug so we want this code
    // to be available to developers
    pub fn dump(&self) {
        println!("---- MAILBOX STATE ----");
        println!("UIDVALIDITY {}", self.uidvalidity);
        println!("INTERNALSEQ {}", self.internalseq);
        for (uid, ident) in self.idx_by_uid.iter() {
            println!(
                "{} {} {}",
                uid,
                hex::encode(ident.0),
                self.table.get(ident).cloned().unwrap().2.into_iter().collect::<Vec<_>>().join(", ")
            );
        }
        println!();
    }
}

impl Default for UidIndex {
    fn default() -> Self {
        Self {
            table: OrdMap::new(),

            idx_by_uid: OrdMap::new(),
            idx_by_modseq: OrdMap::new(),
            idx_by_flag: FlagIndex::new(),
            idx_by_seqid: SeqidMap::new(),
            idx_seqid_of_uuid: OrdMap::new(),

            uidvalidity: NonZeroU32::new(1).unwrap(),

            internalseq: 0,
            internalmodseq: 0,
        }
    }
}

impl BayouState for UidIndex {
    type Op = UidIndexOp;

    fn apply(&self, op: &UidIndexOp) -> Self {
        let mut new = self.clone();
        match op {
            UidIndexOp::MailAdd(ident, iseq, imodseq, flags) => {
                // Change UIDValidity if there is a UID conflict or a MODSEQ conflict
                // The intuition: we increase the UIDValidity by the number of possible conflicts
                // Proof: https://aerogramme.deuxfleurs.fr/documentation/internals/imap-uid/
                if *iseq < new.internalseq || *imodseq < new.internalmodseq {
                    let bump_uid = new.internalseq - iseq;
                    let bump_modseq = (new.internalmodseq - imodseq) as u32;
                    new.uidvalidity =
                        NonZeroU32::new(new.uidvalidity.get() + bump_uid + bump_modseq).unwrap();
                }

                // Assign the real uid of the email using uidnext(), then bump
                // the counter.
                let new_uid = new.uidnext();
                new.internalseq += 1;

                // Assign the real modseq of the email and its new flags.
                //
                // highestmodseq() returns the highest currently assigned
                // modseq; first bump the counter then assign the new modseq.
                new.internalmodseq += 1;
                let new_modseq = new.highestmodseq();

                // We record our email and update our caches
                new.reg_email(*ident, new_uid, new_modseq, flags);
            }
            UidIndexOp::MailDel(ident) => {
                // If the email is known locally, we remove its references in all our indexes
                new.unreg_email(ident);
            }
            UidIndexOp::FlagAdd(ident, imodseq, new_flags) => {
                if let Some((uid, email_modseq, existing_flags)) = new.table.get_mut(ident) {
                    // Bump UIDValidity if required
                    if *imodseq < new.internalmodseq {
                        let bump_modseq = (new.internalmodseq - imodseq) as u32;
                        new.uidvalidity =
                            NonZeroU32::new(new.uidvalidity.get() + bump_modseq).unwrap();
                    }

                    // Add flags to the source of trust and the cache.
                    // Bump the modseq counter first to get a new highestmodseq()
                    new.internalmodseq += 1;
                    new.idx_by_flag.insert(*uid, new_flags);
                    new.idx_by_modseq.remove(email_modseq);
                    *email_modseq = Self::highestmodseq_of_internal(new.internalmodseq);
                    new.idx_by_modseq.insert(*email_modseq, *ident);
                    existing_flags.append(&mut new_flags.clone());
                }
            }
            UidIndexOp::FlagDel(ident, imodseq, rm_flags) => {
                if let Some((uid, email_modseq, existing_flags)) = new.table.get_mut(ident) {
                    // Bump UIDValidity if required
                    if *imodseq < new.internalmodseq {
                        let bump_modseq = (new.internalmodseq - imodseq) as u32;
                        new.uidvalidity =
                            NonZeroU32::new(new.uidvalidity.get() + bump_modseq).unwrap();
                    }

                    // Remove flags from the source of trust and the cache
                    existing_flags.retain(|x| !rm_flags.contains(x));
                    new.idx_by_flag.remove(*uid, rm_flags);

                    // Register that email has been modified.
                    // Bump the modseq counter first to get a new highestmodseq()
                    new.internalmodseq += 1;
                    new.idx_by_modseq.remove(email_modseq);
                    *email_modseq = Self::highestmodseq_of_internal(new.internalmodseq);
                    new.idx_by_modseq.insert(*email_modseq, *ident);
                }
            }
            UidIndexOp::FlagSet(ident, imodseq, new_flags) => {
                if let Some((uid, email_modseq, existing_flags)) = new.table.get_mut(ident) {
                    // Bump UIDValidity if required
                    if *imodseq < new.internalmodseq {
                        let bump_modseq = (new.internalmodseq - imodseq) as u32;
                        new.uidvalidity =
                            NonZeroU32::new(new.uidvalidity.get() + bump_modseq).unwrap();
                    }

                    // Update flags from the source of trust and the cache
                    let rm_flags = existing_flags.difference(new_flags).cloned().collect();
                    *existing_flags = new_flags.clone();
                    new.idx_by_flag.remove(*uid, &rm_flags);
                    new.idx_by_flag.insert(*uid, new_flags);

                    // Register that email has been modified
                    // Bump the modseq counter first to get a new highestmodseq()
                    new.internalmodseq += 1;
                    new.idx_by_modseq.remove(email_modseq);
                    *email_modseq = Self::highestmodseq_of_internal(new.internalmodseq);
                    new.idx_by_modseq.insert(*email_modseq, *ident);
                }
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
    fn insert(&mut self, uid: ImapUid, flags: &Flags) {
        flags.iter().for_each(|flag| {
            self.0
                .entry(flag.clone())
                .or_insert(OrdSet::new())
                .insert(uid);
        });
    }
    fn remove(&mut self, uid: ImapUid, flags: &Flags) {
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

    pub fn flags(&self) -> FlagIter<'_> {
        self.0.keys()
    }
}

// ---- CUSTOM SERIALIZATION AND DESERIALIZATION ----

#[derive(Serialize, Deserialize)]
struct UidIndexSerializedRepr {
    mails: Vec<(ImapUid, ModSeq, UniqueIdent, Flags)>,

    uidvalidity: ImapUidvalidity,

    internalseq: InternalSeq,
    internalmodseq: InternalModSeq,
}

impl<'de> Deserialize<'de> for UidIndex {
    fn deserialize<D>(d: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let val: UidIndexSerializedRepr = UidIndexSerializedRepr::deserialize(d)?;
        let mut uidindex = UidIndex {
            uidvalidity: val.uidvalidity,

            internalseq: val.internalseq,
            internalmodseq: val.internalmodseq,
            ..UidIndex::default()
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
            let f = BTreeSet::from(["\\Recent".to_string(), "\\Archive".to_string()]);
            let ev = state.op_mail_add(m, f);
            state = state.apply(&ev);

            // Early checks
            assert_eq!(state.table.len(), 1);
            let (uid, modseq, flags) = state.table.get(&m).unwrap();
            assert_eq!(*uid, NonZeroU32::new(1).unwrap());
            assert_eq!(*modseq, NonZeroU64::new(2).unwrap());
            assert_eq!(flags.len(), 2);
            let ident = state.idx_by_uid.get(&NonZeroU32::new(1).unwrap()).unwrap();
            assert_eq!(&m, ident);
            let recent = state.idx_by_flag.0.get("\\Recent").unwrap();
            assert_eq!(recent.len(), 1);
            assert_eq!(recent.iter().next().unwrap(), &NonZeroU32::new(1).unwrap());
            assert_eq!(state.uidnext(), NonZeroU32::new(2).unwrap());
            assert_eq!(state.uidvalidity, NonZeroU32::new(1).unwrap());
        }

        // Add message 2
        {
            let m = UniqueIdent([0x02; 24]);
            let f = BTreeSet::from(["\\Seen".to_string(), "\\Archive".to_string()]);
            let ev = state.op_mail_add(m, f);
            state = state.apply(&ev);

            let archive = state.idx_by_flag.0.get("\\Archive").unwrap();
            assert_eq!(archive.len(), 2);
        }

        // Add flags to message 1
        {
            let m = UniqueIdent([0x01; 24]);
            let f = BTreeSet::from(["Important".to_string(), "$cl_1".to_string()]);
            let ev = state.op_flag_add(m, f);
            state = state.apply(&ev);
        }

        // Delete flags from message 1
        {
            let m = UniqueIdent([0x01; 24]);
            let f = BTreeSet::from(["\\Recent".to_string()]);
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
            let f = BTreeSet::from(["\\Archive".to_string(), "\\Recent".to_string()]);
            let ev = UidIndexOp::MailAdd(
                m,
                0,
                0,
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
