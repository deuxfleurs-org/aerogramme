use imap_codec::imap_types::core::NonEmptyVec;
use imap_codec::imap_types::search::SearchKey;
use imap_codec::imap_types::sequence::{SeqOrUid, Sequence, SequenceSet};
use std::num::NonZeroU32;

pub enum SeqType {
    Undefined,
    NonUid,
    Uid,
}
impl SeqType {
    pub fn is_uid(&self) -> bool {
        matches!(self, Self::Uid)
    }
}

pub struct Criteria<'a>(pub &'a SearchKey<'a>);
impl<'a> Criteria<'a> {
    /// Returns a set of email identifiers that is greater or equal
    /// to the set of emails to return
    pub fn to_sequence_set(&self) -> (SequenceSet, SeqType) {
        match self.0 {
            SearchKey::All => (sequence_set_all(), SeqType::Undefined),
            SearchKey::SequenceSet(seq_set) => (seq_set.clone(), SeqType::NonUid),
            SearchKey::Uid(seq_set) => (seq_set.clone(), SeqType::Uid),
            SearchKey::Not(_inner) => {
                tracing::debug!(
                    "using NOT in a search request is slow: it selects all identifiers"
                );
                (sequence_set_all(), SeqType::Undefined)
            }
            SearchKey::Or(left, right) => {
                tracing::debug!("using OR in a search request is slow: no deduplication is done");
                let (base, base_seqtype) = Self(&left).to_sequence_set();
                let (ext, ext_seqtype) = Self(&right).to_sequence_set();

                // Check if we have a UID/ID conflict in fetching: now we don't know how to handle them
                match (base_seqtype, ext_seqtype) {
                    (SeqType::Uid, SeqType::NonUid) | (SeqType::NonUid, SeqType::Uid) => {
                        (sequence_set_all(), SeqType::Undefined)
                    }
                    (SeqType::Undefined, x) | (x, _) => {
                        let mut new_vec = base.0.into_inner();
                        new_vec.extend_from_slice(ext.0.as_ref());
                        let seq = SequenceSet(
                            NonEmptyVec::try_from(new_vec)
                                .expect("merging non empty vec lead to non empty vec"),
                        );
                        (seq, x)
                    }
                }
            }
            SearchKey::And(search_list) => {
                tracing::debug!(
                    "using AND in a search request is slow: no intersection is performed"
                );
                search_list
                    .as_ref()
                    .iter()
                    .map(|crit| Self(&crit).to_sequence_set())
                    .min_by(|(x, _), (y, _)| {
                        let x_size = approx_sequence_set_size(x);
                        let y_size = approx_sequence_set_size(y);
                        x_size.cmp(&y_size)
                    })
                    .unwrap_or((sequence_set_all(), SeqType::Undefined))
            }
            _ => (sequence_set_all(), SeqType::Undefined),
        }
    }

    /// Not really clever as we can have cases where we filter out
    /// the email before needing to inspect its meta.
    /// But for now we are seeking the most basic/stupid algorithm.
    pub fn need_meta(&self) -> bool {
        use SearchKey::*;
        match self.0 {
            // IMF Headers
            Bcc(_) | Cc(_) | From(_) | Header(..) | SentBefore(_) | SentOn(_) | SentSince(_)
            | Subject(_) | To(_) => true,
            // Internal Date is also stored in MailMeta
            Before(_) | On(_) | Since(_) => true,
            // Message size is also stored in MailMeta
            Larger(_) | Smaller(_) => true,
            And(and_list) => and_list.as_ref().iter().any(|sk| Criteria(sk).need_meta()),
            Not(inner) => Criteria(inner).need_meta(),
            Or(left, right) => Criteria(left).need_meta() || Criteria(right).need_meta(),
            _ => false,
        }
    }

    pub fn need_body(&self) -> bool {
        use SearchKey::*;
        match self.0 {
            Text(_) | Body(_) => true,
            And(and_list) => and_list.as_ref().iter().any(|sk| Criteria(sk).need_body()),
            Not(inner) => Criteria(inner).need_body(),
            Or(left, right) => Criteria(left).need_body() || Criteria(right).need_body(),
            _ => false,
        }
    }
}

fn sequence_set_all() -> SequenceSet {
    SequenceSet::from(Sequence::Range(
        SeqOrUid::Value(NonZeroU32::MIN),
        SeqOrUid::Asterisk,
    ))
}

// This is wrong as sequences can overlap
fn approx_sequence_set_size(seq_set: &SequenceSet) -> u64 {
    seq_set.0.as_ref().iter().fold(0u64, |acc, seq| {
        acc.saturating_add(approx_sequence_size(seq))
    })
}

// This is wrong as sequence UID can have holes,
// as we don't know the number of messages in the mailbox also
fn approx_sequence_size(seq: &Sequence) -> u64 {
    match seq {
        Sequence::Single(_) => 1,
        Sequence::Range(SeqOrUid::Asterisk, _) | Sequence::Range(_, SeqOrUid::Asterisk) => u64::MAX,
        Sequence::Range(SeqOrUid::Value(x1), SeqOrUid::Value(x2)) => {
            let x2 = x2.get() as i64;
            let x1 = x1.get() as i64;
            (x2 - x1).abs().try_into().unwrap_or(1)
        }
    }
}
