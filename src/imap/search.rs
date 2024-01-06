use imap_codec::imap_types::core::NonEmptyVec;
use imap_codec::imap_types::search::SearchKey;
use imap_codec::imap_types::sequence::{SeqOrUid, Sequence, SequenceSet};
use std::num::NonZeroU32;

use crate::mail::query::{QueryScope, QueryResult};
use crate::imap::index::MailIndex;

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
                // As we perform no intersection, we don't care if we mix uid or id.
                // We only keep the smallest range, being it ID or UID, depending of
                // which one has the less items. This is an approximation as UID ranges
                // can have holes while ID ones can't.
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
    pub fn query_scope(&self) -> QueryScope {
        use SearchKey::*;
        match self.0 {
            // Combinators
            And(and_list) => and_list.as_ref().iter().fold(QueryScope::Index, |prev, sk| {
                prev.union(&Criteria(sk).query_scope())
            }),
            Not(inner) => Criteria(inner).query_scope(),
            Or(left, right) => Criteria(left).query_scope().union(&Criteria(right).query_scope()),
            All => QueryScope::Index,

            // IMF Headers
            Bcc(_) | Cc(_) | From(_) | Header(..) | SentBefore(_) | SentOn(_) | SentSince(_)
            | Subject(_) | To(_) => QueryScope::Partial,
            // Internal Date is also stored in MailMeta
            Before(_) | On(_) | Since(_) => QueryScope::Partial,
            // Message size is also stored in MailMeta
            Larger(_) | Smaller(_) => QueryScope::Partial,
            // Text and Body require that we fetch the full content!
            Text(_) | Body(_) => QueryScope::Full,

            _ => QueryScope::Index,
        }
    }

    /// Returns emails that we now for sure we want to keep
    /// but also a second list of emails we need to investigate further by 
    /// fetching some remote data
    pub fn filter_on_idx<'b>(&self, midx_list: &[MailIndex<'b>]) -> (Vec<MailIndex<'b>>, Vec<MailIndex<'b>>) {
        let (p1, p2): (Vec<_>, Vec<_>) = midx_list
            .iter()
            .map(|x| (x, self.is_keep_on_idx(x)))
            .filter(|(_midx, decision)| decision.is_keep())
            .map(|(midx, decision)| ((*midx).clone(), decision))
            .partition(|(_midx, decision)| matches!(decision, PartialDecision::Keep));

        let to_keep = p1.into_iter().map(|(v, _)| v).collect();
        let to_fetch = p2.into_iter().map(|(v, _)| v).collect();
        (to_keep, to_fetch)
    }

    pub fn filter_on_query<'b>(&self, midx_list: &[MailIndex<'b>], query_result: &Vec<QueryResult<'_>>) -> Vec<MailIndex<'b>> {
        midx_list
            .iter()
            .zip(query_result.iter())
            .filter(|(midx, qr)| self.is_keep_on_query(midx, qr))
            .map(|(midx, _qr)| midx.clone())
            .collect()
    }

    // ----
    
    /// Here we are doing a partial filtering: we do not have access 
    /// to the headers or to the body, so every time we encounter a rule
    /// based on them, we need to keep it.
    ///
    /// @TODO Could be optimized on a per-email basis by also returning the QueryScope
    /// when more information is needed!
    fn is_keep_on_idx(&self, midx: &MailIndex) -> PartialDecision {
        use SearchKey::*;
        match self.0 {
            // Combinator logic
            And(expr_list) => expr_list
                .as_ref()
                .iter()
                .fold(PartialDecision::Keep, |acc, cur| acc.and(&Criteria(cur).is_keep_on_idx(midx))),
            Or(left, right) => {
                let left_decision = Criteria(left).is_keep_on_idx(midx);
                let right_decision = Criteria(right).is_keep_on_idx(midx);
                left_decision.or(&right_decision)
            }
            Not(expr) => Criteria(expr).is_keep_on_idx(midx).not(),
            All => PartialDecision::Keep,

            // Sequence logic
            maybe_seq if is_sk_seq(maybe_seq) => is_keep_seq(maybe_seq, midx).into(),
            maybe_flag if is_sk_flag(maybe_flag) => is_keep_flag(maybe_flag, midx).into(),
            
            // All the stuff we can't evaluate yet
            Bcc(_) | Cc(_) | From(_) | Header(..) | SentBefore(_) | SentOn(_) | SentSince(_)
            | Subject(_) | To(_) | Before(_) | On(_) | Since(_) | Larger(_) | Smaller(_)
            | Text(_) | Body(_) => PartialDecision::Postpone,

            _ => unreachable!(),
        }
    }

    fn is_keep_on_query(&self, midx: &MailIndex, qr: &QueryResult) -> bool {
        use SearchKey::*;
        match self.0 {
            // Combinator logic
            And(expr_list) => expr_list
                .as_ref()
                .iter()
                .any(|cur| Criteria(cur).is_keep_on_query(midx, qr)),
            Or(left, right) => {
                Criteria(left).is_keep_on_query(midx, qr) || Criteria(right).is_keep_on_query(midx, qr)
            }
            Not(expr) => !Criteria(expr).is_keep_on_query(midx, qr),
            All => true,
            _ => unimplemented!(),
        }
    }
}

// ---- Sequence things ----
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

// --- Partial decision things ----

enum PartialDecision {
    Keep,
    Discard,
    Postpone,
}
impl From<bool>  for PartialDecision {
    fn from(x: bool) -> Self {
        match x {
            true => PartialDecision::Keep,
            _ => PartialDecision::Discard,
        }
    }
}
impl PartialDecision {
    fn not(&self) -> Self {
        match self {
            Self::Keep => Self::Discard,
            Self::Discard => Self::Keep,
            Self::Postpone => Self::Postpone,
        }
    }

    fn or(&self, other: &Self) -> Self {
        match (self, other) {
            (Self::Postpone, _) | (_, Self::Postpone) => Self::Postpone,
            (Self::Keep, _) | (_, Self::Keep) => Self::Keep,
            (Self::Discard, Self::Discard) => Self::Discard,
        }
    }

    fn and(&self, other: &Self) -> Self {
        match (self, other) {
            (Self::Postpone, _) | (_, Self::Postpone) => Self::Postpone,
            (Self::Discard, _) | (_, Self::Discard) => Self::Discard,
            (Self::Keep, Self::Keep) => Self::Keep,
        }
    }

    fn is_keep(&self) -> bool {
        !matches!(self, Self::Discard)
    }
}

// ----- Search Key things ---
fn is_sk_flag(sk: &SearchKey) -> bool {
    use SearchKey::*;
    match sk {
        Answered | Deleted | Draft | Flagged | Keyword(..) | New | Old
            | Recent | Seen | Unanswered | Undeleted | Undraft 
            | Unflagged | Unkeyword(..) | Unseen => true,
        _ => false,
    }
}

fn is_keep_flag(sk: &SearchKey, midx: &MailIndex) -> bool {
    use SearchKey::*;
    match sk {
        Answered => midx.is_flag_set("\\Answered"),
        Deleted => midx.is_flag_set("\\Deleted"),
        Draft => midx.is_flag_set("\\Draft"),
        Flagged => midx.is_flag_set("\\Flagged"),
        Keyword(kw) => midx.is_flag_set(kw.inner()),
        New => {
            let is_recent = midx.is_flag_set("\\Recent");
            let is_seen = midx.is_flag_set("\\Seen");
            is_recent && !is_seen
        },
        Old => {
            let is_recent = midx.is_flag_set("\\Recent");
            !is_recent
        },
        Recent =>  midx.is_flag_set("\\Recent"),
        Seen =>  midx.is_flag_set("\\Seen"),
        Unanswered =>  {
            let is_answered = midx.is_flag_set("\\Recent");
            !is_answered
        },
        Undeleted => {
            let is_deleted = midx.is_flag_set("\\Deleted");
            !is_deleted
        },
        Undraft => {
            let is_draft = midx.is_flag_set("\\Draft");
            !is_draft
        },
        Unflagged => {
            let is_flagged = midx.is_flag_set("\\Flagged");
            !is_flagged
        },
        Unkeyword(kw) => {
            let is_keyword_set = midx.is_flag_set(kw.inner());
            !is_keyword_set
        },
        Unseen => {
            let is_seen = midx.is_flag_set("\\Seen");
            !is_seen
        },

        // Not flag logic
        _ => unreachable!(),
    }
}

fn is_sk_seq(sk: &SearchKey) -> bool {
    use SearchKey::*;
    match sk {
        SequenceSet(..) | Uid(..) => true,
        _ => false,
    }
}
fn is_keep_seq(sk: &SearchKey, midx: &MailIndex) -> bool {
    use SearchKey::*;
    match sk {
        SequenceSet(seq_set) => seq_set.0.as_ref().iter().any(|seq| midx.is_in_sequence_i(seq)),
        Uid(seq_set) => seq_set.0.as_ref().iter().any(|seq| midx.is_in_sequence_uid(seq)),
        _ => unreachable!(),
    }
}
