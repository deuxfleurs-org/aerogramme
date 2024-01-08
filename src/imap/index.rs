use std::num::NonZeroU32;

use anyhow::{anyhow, Context, Result};
use imap_codec::imap_types::sequence::{self, SeqOrUid, Sequence, SequenceSet};

use crate::mail::uidindex::{ImapUid, UidIndex};
use crate::mail::unique_ident::UniqueIdent;

pub struct Index<'a> {
    pub imap_index: Vec<MailIndex<'a>>,
    pub internal: &'a UidIndex,
}
impl<'a> Index<'a> {
    pub fn new(internal: &'a UidIndex) -> Result<Self> {
        let imap_index = internal
            .idx_by_uid
            .iter()
            .enumerate()
            .map(|(i_enum, (&uid, &uuid))| {
                let flags = internal
                    .table
                    .get(&uuid)
                    .ok_or(anyhow!("mail is missing from index"))?
                    .1
                    .as_ref();
                let i_int: u32 = (i_enum + 1).try_into()?;
                let i: NonZeroU32 = i_int.try_into()?;

                Ok(MailIndex {
                    i,
                    uid,
                    uuid,
                    flags,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(Self {
            imap_index,
            internal,
        })
    }

    pub fn last(&'a self) -> Option<&'a MailIndex<'a>> {
        self.imap_index.last()
    }

    /// Fetch mail descriptors based on a sequence of UID
    ///
    /// Complexity analysis:
    ///  - Sort is O(n * log n) where n is the number of uid generated by the sequence
    ///  - Finding the starting point in the index O(log m) where m is the size of the mailbox
    /// While n =< m, it's not clear if the difference is big or not.
    ///
    /// For now, the algorithm tries to be fast for small values of n,
    /// as it is what is expected by clients.
    ///
    /// So we assume for our implementation that : n << m.
    /// It's not true for full mailbox searches for example...
    pub fn fetch_on_uid(&'a self, sequence_set: &SequenceSet) -> Vec<&'a MailIndex<'a>> {
        if self.imap_index.is_empty() {
            return vec![];
        }
        let iter_strat = sequence::Strategy::Naive {
            largest: self.last().expect("imap index is not empty").uid,
        };
        let mut unroll_seq = sequence_set.iter(iter_strat).collect::<Vec<_>>();
        unroll_seq.sort();

        let start_seq = match unroll_seq.iter().next() {
            Some(elem) => elem,
            None => return vec![],
        };

        // Quickly jump to the right point in the mailbox vector O(log m) instead
        // of iterating one by one O(m). Works only because both unroll_seq & imap_index are sorted per uid.
        let mut imap_idx = {
            let start_idx = self
                .imap_index
                .partition_point(|mail_idx| &mail_idx.uid < start_seq);
            &self.imap_index[start_idx..]
        };
        println!(
            "win: {:?}",
            imap_idx.iter().map(|midx| midx.uid).collect::<Vec<_>>()
        );

        let mut acc = vec![];
        for wanted_uid in unroll_seq.iter() {
            // Slide the window forward as long as its first element is lower than our wanted uid.
            let start_idx = match imap_idx.iter().position(|midx| &midx.uid >= wanted_uid) {
                Some(v) => v,
                None => break,
            };
            imap_idx = &imap_idx[start_idx..];

            // If the beginning of our new window is the uid we want, we collect it
            if &imap_idx[0].uid == wanted_uid {
                acc.push(&imap_idx[0]);
            }
        }

        acc
    }

    pub fn fetch_on_id(&'a self, sequence_set: &SequenceSet) -> Result<Vec<&'a MailIndex<'a>>> {
        let iter_strat = sequence::Strategy::Naive {
            largest: self.last().context("The mailbox is empty")?.uid,
        };
        sequence_set
            .iter(iter_strat)
            .map(|wanted_id| {
                self.imap_index
                    .get((wanted_id.get() as usize) - 1)
                    .ok_or(anyhow!("Mail not found"))
            })
            .collect::<Result<Vec<_>>>()
    }

    pub fn fetch(
        self: &'a Index<'a>,
        sequence_set: &SequenceSet,
        by_uid: bool,
    ) -> Result<Vec<&'a MailIndex<'a>>> {
        match by_uid {
            true => Ok(self.fetch_on_uid(sequence_set)),
            _ => self.fetch_on_id(sequence_set),
        }
    }
}

#[derive(Clone, Debug)]
pub struct MailIndex<'a> {
    pub i: NonZeroU32,
    pub uid: ImapUid,
    pub uuid: UniqueIdent,
    pub flags: &'a Vec<String>,
}

impl<'a> MailIndex<'a> {
    // The following functions are used to implement the SEARCH command
    pub fn is_in_sequence_i(&self, seq: &Sequence) -> bool {
        match seq {
            Sequence::Single(SeqOrUid::Asterisk) => true,
            Sequence::Single(SeqOrUid::Value(target)) => target == &self.i,
            Sequence::Range(SeqOrUid::Asterisk, SeqOrUid::Value(x))
            | Sequence::Range(SeqOrUid::Value(x), SeqOrUid::Asterisk) => x <= &self.i,
            Sequence::Range(SeqOrUid::Value(x1), SeqOrUid::Value(x2)) => {
                if x1 < x2 {
                    x1 <= &self.i && &self.i <= x2
                } else {
                    x1 >= &self.i && &self.i >= x2
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
