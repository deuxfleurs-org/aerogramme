use std::num::NonZeroU64;

use imap_codec::imap_types::sequence::{SeqOrUid, Sequence, SequenceSet};

use aero_collections::mail::uidindex::{ImapUid, ImapSeqid, ModSeq, UidIndex};
use aero_collections::unique_ident::UniqueIdent;

// Helper functions to query email indexes of UidIndex.
//
// UidIndex maintains the relevant indexes, but it does not depend on IMAP crates.
// These helpers allow querying UidIndex based on IMAP types like SequenceSet.

// Extension trait that adds extra methods to UidIndex.
pub trait UidIndexForImap {
    fn fetch_by_uid(&self, sequence_set: &SequenceSet) -> Vec<MailIndex>;
    fn fetch_by_seqid(&self, sequence_set: &SequenceSet) -> Vec<MailIndex>;
    
    fn fetch(
        &self,
        sequence_set: &SequenceSet,
        by_uid: bool,
    ) -> Vec<MailIndex> {
        match by_uid {
            true => self.fetch_by_uid(sequence_set),
            false => self.fetch_by_seqid(sequence_set),
        }
    }

    fn fetch_changed_since(
        &self,
        sequence_set: &SequenceSet,
        maybe_modseq: Option<NonZeroU64>,
        by_uid: bool,
    ) -> Vec<MailIndex> {
        let raw = self.fetch(sequence_set, by_uid);
        match maybe_modseq {
            Some(pit) => raw.into_iter().filter(|midx| midx.modseq > pit).collect(),
            None => raw,
        }
    }

    fn fetch_unchanged_since(
        &self,
        sequence_set: &SequenceSet,
        maybe_modseq: Option<NonZeroU64>,
        by_uid: bool,
    ) -> (Vec<MailIndex>, Vec<MailIndex>) {
        let raw = self.fetch(sequence_set, by_uid);
        match maybe_modseq {
            Some(pit) => raw.into_iter().partition(|midx| midx.modseq <= pit),
            None => (raw, vec![]),
        }
    }
}

impl UidIndexForImap for UidIndex {
    fn fetch_by_uid(&self, sequence_seq: &SequenceSet) -> Vec<MailIndex> {
        let uuid_largest = match self.idx_by_seqid.largest() {
            Some((_, uuid)) => uuid,
            None => return vec![],
        };
        let (uid_largest, _, _) = self.table.get(uuid_largest).unwrap();
        sequence_seq
            .iter(*uid_largest)
            .filter_map(|uid| {
                let &uuid = self.idx_by_uid.get(&uid)?;
                let &(uid, modseq, ref flags) = self.table.get(&uuid)?;
                let &seqid = self.idx_seqid_of_uuid.get(&uuid)?;
                Some(MailIndex { seqid, uid, uuid, modseq, flags: flags.clone() })
            })
            .collect()
    }

    fn fetch_by_seqid(&self, sequence_seq: &SequenceSet) -> Vec<MailIndex> {
        let seqid_largest = match self.idx_by_seqid.largest() {
            Some((seqid, _)) => seqid,
            None => return vec![],
        };
        sequence_seq
            .iter(seqid_largest)
            .filter_map(|seqid| {
                let &uuid = self.idx_by_seqid.get(seqid)?;
                let &(uid, modseq, ref flags) = self.table.get(&uuid)?;
                Some(MailIndex { seqid, uid, uuid, modseq, flags: flags.clone() })
            })
            .collect()
    }
}

// @FIXME this could be a MailIndex<'a> with flags: &'a Vec<String>
#[derive(Clone, Debug)]
pub struct MailIndex {
    pub seqid: ImapSeqid,
    pub uid: ImapUid,
    pub uuid: UniqueIdent,
    pub modseq: ModSeq,
    pub flags: Vec<String>,
}

impl MailIndex {
    // The following functions are used to implement the SEARCH command
    pub fn is_in_sequence_seqid(&self, seq: &Sequence) -> bool {
        match seq {
            Sequence::Single(SeqOrUid::Asterisk) => true,
            Sequence::Single(SeqOrUid::Value(target)) => target == &self.seqid,
            Sequence::Range(SeqOrUid::Asterisk, SeqOrUid::Value(x))
            | Sequence::Range(SeqOrUid::Value(x), SeqOrUid::Asterisk) => x <= &self.seqid,
            Sequence::Range(SeqOrUid::Value(x1), SeqOrUid::Value(x2)) => {
                if x1 < x2 {
                    x1 <= &self.seqid && &self.seqid <= x2
                } else {
                    x1 >= &self.seqid && &self.seqid >= x2
                }
            }
            Sequence::Range(SeqOrUid::Asterisk, SeqOrUid::Asterisk) => true,
        }
    }

    pub fn is_in_sequence_uid(&self, seq: &Sequence) -> bool {
        match seq {
            Sequence::Single(SeqOrUid::Asterisk) => true,
            Sequence::Single(SeqOrUid::Value(target)) => target == &self.uid,
            Sequence::Range(SeqOrUid::Asterisk, SeqOrUid::Value(x))
            | Sequence::Range(SeqOrUid::Value(x), SeqOrUid::Asterisk) => x <= &self.uid,
            Sequence::Range(SeqOrUid::Value(x1), SeqOrUid::Value(x2)) => {
                if x1 < x2 {
                    x1 <= &self.uid && &self.uid <= x2
                } else {
                    x1 >= &self.uid && &self.uid >= x2
                }
            }
            Sequence::Range(SeqOrUid::Asterisk, SeqOrUid::Asterisk) => true,
        }
    }

    pub fn is_flag_set(&self, flag: &str) -> bool {
        self.flags
            .iter()
            .any(|candidate| candidate.as_str() == flag)
    }
}
