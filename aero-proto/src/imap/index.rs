use std::num::{NonZeroU32, NonZeroU64};

use imap_codec::imap_types::sequence::{SeqOrUid, Sequence, SequenceSet};

use aero_collections::mail::uidindex::{ImapSeqid, ImapUid, ModSeq, UidIndex};
use aero_collections::unique_ident::UniqueIdent;

// Helper functions to query email indexes of UidIndex.
//
// UidIndex maintains the relevant indexes, but it does not depend on IMAP crates.
// These helpers allow querying UidIndex based on IMAP types like SequenceSet.

// Extension trait that adds extra methods to UidIndex.
pub trait UidIndexForImap {
    fn fetch_by_uid(&self, sequence_set: &SequenceSet) -> Vec<MailIndex>;
    fn fetch_by_seqid(&self, sequence_set: &SequenceSet) -> Vec<MailIndex>;

    fn fetch(&self, sequence_set: &SequenceSet, by_uid: bool) -> Vec<MailIndex> {
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
        let largest_uuid = match self.idx_by_seqid.largest() {
            Some((_, uuid)) => uuid,
            None => return vec![],
        };
        let &(largest_uid, _, _) = self.table.get(largest_uuid).unwrap();
        let largest_seqid = match self.idx_by_seqid.largest() {
            Some((seqid, _)) => seqid,
            None => return vec![],
        };
        // NOTE: sequence_seq may describe an arbitrarily large range of
        // integers, so we must not iterate over all of it...
        sequence_seq
            .iter(largest_uid)
            // TODO: could this be done automatically by SequenceSet::iter?
            .take_while(|uid| *uid <= largest_uid)
            .filter_map(|uid| {
                let &uuid = self.idx_by_uid.get(&uid)?;
                let &(uid, modseq, ref flags) = self.table.get(&uuid)?;
                let &seqid = self.idx_seqid_of_uuid.get(&uuid)?;
                Some(MailIndex {
                    seqid,
                    uid,
                    uuid,
                    modseq,
                    flags: flags.clone(),
                    largest_seqid,
                    largest_uid,
                })
            })
            .collect()
    }

    fn fetch_by_seqid(&self, sequence_seq: &SequenceSet) -> Vec<MailIndex> {
        let largest_uuid = match self.idx_by_seqid.largest() {
            Some((_, uuid)) => uuid,
            None => return vec![],
        };
        let &(largest_uid, _, _) = self.table.get(largest_uuid).unwrap();
        let largest_seqid = match self.idx_by_seqid.largest() {
            Some((seqid, _)) => seqid,
            None => return vec![],
        };
        sequence_seq
            .iter(largest_seqid)
            .take_while(|seqid| *seqid <= largest_seqid)
            .filter_map(|seqid| {
                let &uuid = self.idx_by_seqid.get(seqid)?;
                let &(uid, modseq, ref flags) = self.table.get(&uuid)?;
                Some(MailIndex {
                    seqid,
                    uid,
                    uuid,
                    modseq,
                    flags: flags.clone(),
                    largest_seqid,
                    largest_uid,
                })
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
    // the `largest_*` fields are required to compare a MailIndex against '*'
    // (SeqOrUid::Asterisk), which refers to "the largest seqid/uid currently in
    // use"
    pub largest_seqid: ImapSeqid,
    pub largest_uid: ImapUid,
}

impl MailIndex {
    // The following functions are used to implement the SEARCH command
    pub fn is_in_sequence_seqid(&self, seq: &Sequence) -> bool {
        let (lo, hi) = range_of_sequence(seq, self.largest_seqid);
        lo <= self.seqid && self.seqid <= hi
    }

    pub fn is_in_sequence_uid(&self, seq: &Sequence) -> bool {
        let (lo, hi) = range_of_sequence(seq, self.largest_uid);
        lo <= self.uid && self.uid <= hi
    }

    pub fn is_flag_set(&self, flag: &str) -> bool {
        self.flags
            .iter()
            .any(|candidate| candidate.as_str() == flag)
    }
}

// Converts a Sequence into an equivalent inclusive range [x;y]
fn range_of_sequence(seq: &Sequence, largest: NonZeroU32) -> (NonZeroU32, NonZeroU32) {
    let (x, y) = match seq {
        Sequence::Single(s) => (num_of_seqoruid(*s, largest), num_of_seqoruid(*s, largest)),
        Sequence::Range(x, y) => (num_of_seqoruid(*x, largest), num_of_seqoruid(*y, largest)),
    };
    if x <= y {
        (x, y)
    } else {
        (y, x)
    }
}

fn num_of_seqoruid(s: SeqOrUid, largest: NonZeroU32) -> NonZeroU32 {
    match s {
        SeqOrUid::Asterisk => largest,
        SeqOrUid::Value(n) => n,
    }
}
