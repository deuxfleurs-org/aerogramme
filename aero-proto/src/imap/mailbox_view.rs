use std::collections::HashSet;
use std::num::{NonZeroU32, NonZeroU64};
use std::sync::Arc;

use anyhow::{anyhow, Error, Result};

use futures::stream::{StreamExt, TryStreamExt};

use imap_codec::imap_types::core::Charset;
use imap_codec::imap_types::fetch::MessageDataItem;
use imap_codec::imap_types::flag::{Flag, FlagFetch, FlagPerm, StoreResponse, StoreType};
use imap_codec::imap_types::response::{Code, CodeOther, Data, Status};
use imap_codec::imap_types::search::SearchKey;
use imap_codec::imap_types::sequence::SequenceSet;

use aero_collections::mail::mailbox::Mailbox;
use aero_collections::mail::query::QueryScope;
use aero_collections::mail::snapshot::FrozenMailbox;
use aero_collections::mail::uidindex::{ImapUid, ImapUidvalidity, ModSeq};
use aero_collections::unique_ident::UniqueIdent;

use crate::imap::attributes::AttributesProxy;
use crate::imap::flags;
use crate::imap::index::Index;
use crate::imap::mail_view::{MailView, SeenFlag};
use crate::imap::response::Body;
use crate::imap::search;

const DEFAULT_FLAGS: [Flag; 5] = [
    Flag::Seen,
    Flag::Answered,
    Flag::Flagged,
    Flag::Deleted,
    Flag::Draft,
];

pub struct UpdateParameters {
    pub silence: HashSet<UniqueIdent>,
    pub with_modseq: bool,
    pub with_uid: bool,
}
impl Default for UpdateParameters {
    fn default() -> Self {
        Self {
            silence: HashSet::new(),
            with_modseq: false,
            with_uid: false,
        }
    }
}

/// A MailboxView is responsible for giving the client the information
/// it needs about a mailbox, such as an initial summary of the mailbox's
/// content and continuous updates indicating when the content
/// of the mailbox has been changed.
/// To do this, it keeps a variable `known_state` that corresponds to
/// what the client knows, and produces IMAP messages to be sent to the
/// client that go along updates to `known_state`.
pub struct MailboxView {
    pub internal: FrozenMailbox,
    pub is_condstore: bool,
}

impl MailboxView {
    /// Creates a new IMAP view into a mailbox.
    pub async fn new(mailbox: Arc<Mailbox>, is_cond: bool) -> Self {
        Self {
            internal: mailbox.frozen().await,
            is_condstore: is_cond,
        }
    }

    /// Create an updated view, useful to make a diff
    /// between what the client knows and new stuff
    /// Produces a set of IMAP responses describing the change between
    /// what the client knows and what is actually in the mailbox.
    /// This does NOT trigger a sync, it bases itself on what is currently
    /// loaded in RAM by Bayou.
    pub async fn update(&mut self, params: UpdateParameters) -> Result<Vec<Body<'static>>> {
        let old_snapshot = self.internal.update().await;
        let new_snapshot = &self.internal.snapshot;

        let mut data = Vec::<Body>::new();

        // Calculate diff between two mailbox states
        // See example in IMAP RFC in section on NOOP command:
        // we want to produce something like this:
        // C: a047 NOOP
        // S: * 22 EXPUNGE
        // S: * 23 EXISTS
        // S: * 14 FETCH (UID 1305 FLAGS (\Seen \Deleted))
        // S: a047 OK Noop completed
        // In other words:
        // - notify client of expunged mails
        // - if new mails arrived, notify client of number of existing mails
        // - if flags changed for existing mails, tell client
        //   (for this last step: if uidvalidity changed, do nothing,
        //   just notify of new uidvalidity and they will resync)

        // - notify client of expunged mails
        let mut n_expunge = 0;
        for (i, (_uid, uuid)) in old_snapshot.idx_by_uid.iter().enumerate() {
            if !new_snapshot.table.contains_key(uuid) {
                data.push(Body::Data(Data::Expunge(
                    NonZeroU32::try_from((i + 1 - n_expunge) as u32).unwrap(),
                )));
                n_expunge += 1;
            }
        }

        // - if new mails arrived, notify client of number of existing mails
        if new_snapshot.table.len() != old_snapshot.table.len() - n_expunge
            || new_snapshot.uidvalidity != old_snapshot.uidvalidity
        {
            data.push(self.exists_status()?);
        }

        if new_snapshot.uidvalidity != old_snapshot.uidvalidity {
            // TODO: do we want to push less/more info than this?
            data.push(self.uidvalidity_status()?);
            data.push(self.uidnext_status()?);
        } else {
            // - if flags changed for existing mails, tell client
            for (i, (_uid, uuid)) in new_snapshot.idx_by_uid.iter().enumerate() {
                if params.silence.contains(uuid) {
                    continue;
                }

                let old_mail = old_snapshot.table.get(uuid);
                let new_mail = new_snapshot.table.get(uuid);
                if old_mail.is_some() && old_mail != new_mail {
                    if let Some((uid, modseq, flags)) = new_mail {
                        let mut items = vec![MessageDataItem::Flags(
                            flags.iter().filter_map(|f| flags::from_str(f)).collect(),
                        )];

                        if params.with_uid {
                            items.push(MessageDataItem::Uid(*uid));
                        }

                        if params.with_modseq {
                            items.push(MessageDataItem::ModSeq(*modseq));
                        }

                        data.push(Body::Data(Data::Fetch {
                            seq: NonZeroU32::try_from((i + 1) as u32).unwrap(),
                            items: items.try_into()?,
                        }));
                    }
                }
            }
        }
        Ok(data)
    }

    /// Generates the necessary IMAP messages so that the client
    /// has a satisfactory summary of the current mailbox's state.
    /// These are the messages that are sent in response to a SELECT command.
    pub fn summary(&self) -> Result<Vec<Body<'static>>> {
        let mut data = Vec::<Body>::new();
        data.push(self.exists_status()?);
        data.push(self.recent_status()?);
        data.extend(self.flags_status()?.into_iter());
        data.push(self.uidvalidity_status()?);
        data.push(self.uidnext_status()?);
        if self.is_condstore {
            data.push(self.highestmodseq_status()?);
        }
        /*self.unseen_first_status()?
        .map(|unseen_status| data.push(unseen_status));*/

        Ok(data)
    }

    pub async fn store<'a>(
        &mut self,
        sequence_set: &SequenceSet,
        kind: &StoreType,
        response: &StoreResponse,
        flags: &[Flag<'a>],
        unchanged_since: Option<NonZeroU64>,
        is_uid_store: &bool,
    ) -> Result<(Vec<Body<'static>>, Vec<NonZeroU32>)> {
        self.internal.sync().await?;

        let flags = flags.iter().map(|x| x.to_string()).collect::<Vec<_>>();

        let idx = self.index()?;
        let (editable, in_conflict) =
            idx.fetch_unchanged_since(sequence_set, unchanged_since, *is_uid_store)?;

        for mi in editable.iter() {
            match kind {
                StoreType::Add => {
                    self.internal.mailbox.add_flags(mi.uuid, &flags[..]).await?;
                }
                StoreType::Remove => {
                    self.internal.mailbox.del_flags(mi.uuid, &flags[..]).await?;
                }
                StoreType::Replace => {
                    self.internal.mailbox.set_flags(mi.uuid, &flags[..]).await?;
                }
            }
        }

        let silence = match response {
            StoreResponse::Answer => HashSet::new(),
            StoreResponse::Silent => editable.iter().map(|midx| midx.uuid).collect(),
        };

        let conflict_id_or_uid = match is_uid_store {
            true => in_conflict.into_iter().map(|midx| midx.uid).collect(),
            _ => in_conflict.into_iter().map(|midx| midx.i).collect(),
        };

        let summary = self
            .update(UpdateParameters {
                with_uid: *is_uid_store,
                with_modseq: unchanged_since.is_some(),
                silence,
            })
            .await?;

        Ok((summary, conflict_id_or_uid))
    }

    pub async fn idle_sync(&mut self) -> Result<Vec<Body<'static>>> {
        self.internal
            .mailbox
            .notify()
            .await
            .upgrade()
            .ok_or(anyhow!("test"))?
            .notified()
            .await;
        self.internal.mailbox.opportunistic_sync().await?;
        self.update(UpdateParameters::default()).await
    }

    pub async fn expunge(
        &mut self,
        maybe_seq_set: &Option<SequenceSet>,
    ) -> Result<Vec<Body<'static>>> {
        // Get a recent view to apply our change
        self.internal.sync().await?;
        let state = self.internal.peek().await;
        let idx = Index::new(&state)?;

        // Build a default sequence set for the default case
        use imap_codec::imap_types::sequence::{SeqOrUid, Sequence};
        let seq = match maybe_seq_set {
            Some(s) => s.clone(),
            None => SequenceSet(
                vec![Sequence::Range(
                    SeqOrUid::Value(NonZeroU32::MIN),
                    SeqOrUid::Asterisk,
                )]
                .try_into()
                .unwrap(),
            ),
        };

        let deleted_flag = Flag::Deleted.to_string();
        let msgs = idx
            .fetch_on_uid(&seq)
            .into_iter()
            .filter(|midx| midx.flags.iter().any(|x| *x == deleted_flag))
            .map(|midx| midx.uuid);

        for msg in msgs {
            self.internal.mailbox.delete(msg).await?;
        }

        self.update(UpdateParameters::default()).await
    }

    pub async fn copy(
        &self,
        sequence_set: &SequenceSet,
        to: Arc<Mailbox>,
        is_uid_copy: &bool,
    ) -> Result<(ImapUidvalidity, Vec<(ImapUid, ImapUid)>)> {
        let idx = self.index()?;
        let mails = idx.fetch(sequence_set, *is_uid_copy)?;

        let mut new_uuids = vec![];
        for mi in mails.iter() {
            new_uuids.push(to.copy_from(&self.internal.mailbox, mi.uuid).await?);
        }

        let mut ret = vec![];
        let to_state = to.current_uid_index().await;
        for (mi, new_uuid) in mails.iter().zip(new_uuids.iter()) {
            let dest_uid = to_state
                .table
                .get(new_uuid)
                .ok_or(anyhow!("copied mail not in destination mailbox"))?
                .0;
            ret.push((mi.uid, dest_uid));
        }

        Ok((to_state.uidvalidity, ret))
    }

    pub async fn r#move(
        &mut self,
        sequence_set: &SequenceSet,
        to: Arc<Mailbox>,
        is_uid_copy: &bool,
    ) -> Result<(ImapUidvalidity, Vec<(ImapUid, ImapUid)>, Vec<Body<'static>>)> {
        let idx = self.index()?;
        let mails = idx.fetch(sequence_set, *is_uid_copy)?;

        for mi in mails.iter() {
            to.move_from(&self.internal.mailbox, mi.uuid).await?;
        }

        let mut ret = vec![];
        let to_state = to.current_uid_index().await;
        for mi in mails.iter() {
            let dest_uid = to_state
                .table
                .get(&mi.uuid)
                .ok_or(anyhow!("moved mail not in destination mailbox"))?
                .0;
            ret.push((mi.uid, dest_uid));
        }

        let update = self
            .update(UpdateParameters {
                with_uid: *is_uid_copy,
                ..UpdateParameters::default()
            })
            .await?;

        Ok((to_state.uidvalidity, ret, update))
    }

    /// Looks up state changes in the mailbox and produces a set of IMAP
    /// responses describing the new state.
    pub async fn fetch<'b>(
        &self,
        sequence_set: &SequenceSet,
        ap: &AttributesProxy,
        changed_since: Option<NonZeroU64>,
        is_uid_fetch: &bool,
    ) -> Result<Vec<Body<'static>>> {
        // [1/6] Pre-compute data
        //  a. what are the uuids of the emails we want?
        //  b. do we need to fetch the full body?
        //let ap = AttributesProxy::new(attributes, *is_uid_fetch);
        let query_scope = match ap.need_body() {
            true => QueryScope::Full,
            _ => QueryScope::Partial,
        };
        tracing::debug!("Query scope {:?}", query_scope);
        let idx = self.index()?;
        let mail_idx_list = idx.fetch_changed_since(sequence_set, changed_since, *is_uid_fetch)?;

        // [2/6] Fetch the emails
        let uuids = mail_idx_list
            .iter()
            .map(|midx| midx.uuid)
            .collect::<Vec<_>>();

        let query = self.internal.query(&uuids, query_scope);
        //let query_result = self.internal.query(&uuids, query_scope).fetch().await?;

        let query_stream = query
            .fetch()
            .zip(futures::stream::iter(mail_idx_list))
            // [3/6] Derive an IMAP-specific view from the results, apply the filters
            .map(|(maybe_qr, midx)| match maybe_qr {
                Ok(qr) => Ok((MailView::new(&qr, midx)?.filter(&ap)?, midx)),
                Err(e) => Err(e),
            })
            // [4/6] Apply the IMAP transformation
            .then(|maybe_ret| async move {
                let ((body, seen), midx) = maybe_ret?;

                // [5/6] Register the \Seen flags
                if matches!(seen, SeenFlag::MustAdd) {
                    let seen_flag = Flag::Seen.to_string();
                    self.internal
                        .mailbox
                        .add_flags(midx.uuid, &[seen_flag])
                        .await?;
                }

                Ok::<_, anyhow::Error>(body)
            });

        // [6/6] Build the final result that will be sent to the client.
        query_stream.try_collect().await
    }

    /// A naive search implementation...
    pub async fn search<'a>(
        &self,
        _charset: &Option<Charset<'a>>,
        search_key: &SearchKey<'a>,
        uid: bool,
    ) -> Result<(Vec<Body<'static>>, bool)> {
        // 1. Compute the subset of sequence identifiers we need to fetch
        // based on the search query
        let crit = search::Criteria(search_key);
        let (seq_set, seq_type) = crit.to_sequence_set();

        // 2. Get the selection
        let idx = self.index()?;
        let selection = idx.fetch(&seq_set, seq_type.is_uid())?;

        // 3. Filter the selection based on the ID / UID / Flags
        let (kept_idx, to_fetch) = crit.filter_on_idx(&selection);

        // 4.a Fetch additional info about the emails
        let query_scope = crit.query_scope();
        let uuids = to_fetch.iter().map(|midx| midx.uuid).collect::<Vec<_>>();
        let query = self.internal.query(&uuids, query_scope);

        // 4.b We don't want to keep all data in memory, so we do the computing in a stream
        let query_stream = query
            .fetch()
            .zip(futures::stream::iter(&to_fetch))
            // 5.a Build a mailview with the body, might fail with an error
            // 5.b If needed, filter the selection based on the body, but keep the errors
            // 6. Drop the query+mailbox, keep only the mail index
            // Here we release a lot of memory, this is the most important part ^^
            .filter_map(|(maybe_qr, midx)| {
                let r = match maybe_qr {
                    Ok(qr) => match MailView::new(&qr, midx).map(|mv| crit.is_keep_on_query(&mv)) {
                        Ok(true) => Some(Ok(*midx)),
                        Ok(_) => None,
                        Err(e) => Some(Err(e)),
                    },
                    Err(e) => Some(Err(e)),
                };
                futures::future::ready(r)
            });

        // 7. Chain both streams (part resolved from index, part resolved from metadata+body)
        let main_stream = futures::stream::iter(kept_idx)
            .map(Ok)
            .chain(query_stream)
            .map_ok(|idx| match uid {
                true => (idx.uid, idx.modseq),
                _ => (idx.i, idx.modseq),
            });

        // 8. Do the actual computation
        let internal_result: Vec<_> = main_stream.try_collect().await?;
        let (selection, modseqs): (Vec<_>, Vec<_>) = internal_result.into_iter().unzip();

        // 9. Aggregate the maximum modseq value
        let maybe_modseq = match crit.is_modseq() {
            true => modseqs.into_iter().max(),
            _ => None,
        };

        // 10. Return the final result
        Ok((
            vec![Body::Data(Data::Search(selection, maybe_modseq))],
            maybe_modseq.is_some(),
        ))
    }

    // ----
    /// @FIXME index should be stored for longer than a single request
    /// Instead they should be tied to the FrozenMailbox refresh
    /// It's not trivial to refactor the code to do that, so we are doing
    /// some useless computation for now...
    fn index<'a>(&'a self) -> Result<Index<'a>> {
        Index::new(&self.internal.snapshot)
    }

    /// Produce an OK [UIDVALIDITY _] message corresponding to `known_state`
    fn uidvalidity_status(&self) -> Result<Body<'static>> {
        let uid_validity = Status::ok(
            None,
            Some(Code::UidValidity(self.uidvalidity())),
            "UIDs valid",
        )
        .map_err(Error::msg)?;
        Ok(Body::Status(uid_validity))
    }

    pub(crate) fn uidvalidity(&self) -> ImapUidvalidity {
        self.internal.snapshot.uidvalidity
    }

    /// Produce an OK [UIDNEXT _] message corresponding to `known_state`
    fn uidnext_status(&self) -> Result<Body<'static>> {
        let next_uid = Status::ok(
            None,
            Some(Code::UidNext(self.uidnext())),
            "Predict next UID",
        )
        .map_err(Error::msg)?;
        Ok(Body::Status(next_uid))
    }

    pub(crate) fn uidnext(&self) -> ImapUid {
        self.internal.snapshot.uidnext
    }

    pub(crate) fn highestmodseq_status(&self) -> Result<Body<'static>> {
        Ok(Body::Status(Status::ok(
            None,
            Some(Code::Other(CodeOther::unvalidated(
                format!("HIGHESTMODSEQ {}", self.highestmodseq()).into_bytes(),
            ))),
            "Highest",
        )?))
    }

    pub(crate) fn highestmodseq(&self) -> ModSeq {
        self.internal.snapshot.highestmodseq
    }

    /// Produce an EXISTS message corresponding to the number of mails
    /// in `known_state`
    fn exists_status(&self) -> Result<Body<'static>> {
        Ok(Body::Data(Data::Exists(self.exists()?)))
    }

    pub(crate) fn exists(&self) -> Result<u32> {
        Ok(u32::try_from(self.internal.snapshot.idx_by_uid.len())?)
    }

    /// Produce a RECENT message corresponding to the number of
    /// recent mails in `known_state`
    fn recent_status(&self) -> Result<Body<'static>> {
        Ok(Body::Data(Data::Recent(self.recent()?)))
    }

    #[allow(dead_code)]
    fn unseen_first_status(&self) -> Result<Option<Body<'static>>> {
        Ok(self
            .unseen_first()?
            .map(|unseen_id| {
                Status::ok(None, Some(Code::Unseen(unseen_id)), "First unseen.").map(Body::Status)
            })
            .transpose()?)
    }

    #[allow(dead_code)]
    fn unseen_first(&self) -> Result<Option<NonZeroU32>> {
        Ok(self
            .internal
            .snapshot
            .table
            .values()
            .enumerate()
            .find(|(_i, (_imap_uid, _modseq, flags))| !flags.contains(&"\\Seen".to_string()))
            .map(|(i, _)| NonZeroU32::try_from(i as u32 + 1))
            .transpose()?)
    }

    pub(crate) fn recent(&self) -> Result<u32> {
        let recent = self
            .internal
            .snapshot
            .idx_by_flag
            .get(&"\\Recent".to_string())
            .map(|os| os.len())
            .unwrap_or(0);
        Ok(u32::try_from(recent)?)
    }

    /// Produce a FLAGS and a PERMANENTFLAGS message that indicates
    /// the flags that are in `known_state` + default flags
    fn flags_status(&self) -> Result<Vec<Body<'static>>> {
        let mut body = vec![];

        // 1. Collecting all the possible flags in the mailbox
        // 1.a Fetch them from our index
        let mut known_flags: Vec<Flag> = self
            .internal
            .snapshot
            .idx_by_flag
            .flags()
            .filter_map(|f| match flags::from_str(f) {
                Some(FlagFetch::Flag(fl)) => Some(fl),
                _ => None,
            })
            .collect();
        // 1.b Merge it with our default flags list
        for f in DEFAULT_FLAGS.iter() {
            if !known_flags.contains(f) {
                known_flags.push(f.clone());
            }
        }
        // 1.c Create the IMAP message
        body.push(Body::Data(Data::Flags(known_flags.clone())));

        // 2. Returning flags that are persisted
        // 2.a Always advertise our default flags
        let mut permanent = DEFAULT_FLAGS
            .iter()
            .map(|f| FlagPerm::Flag(f.clone()))
            .collect::<Vec<_>>();
        // 2.b Say that we support any keyword flag
        permanent.push(FlagPerm::Asterisk);
        // 2.c Create the IMAP message
        let permanent_flags = Status::ok(
            None,
            Some(Code::PermanentFlags(permanent)),
            "Flags permitted",
        )
        .map_err(Error::msg)?;
        body.push(Body::Status(permanent_flags));

        // Done!
        Ok(body)
    }

    pub(crate) fn unseen_count(&self) -> usize {
        let total = self.internal.snapshot.table.len();
        let seen = self
            .internal
            .snapshot
            .idx_by_flag
            .get(&Flag::Seen.to_string())
            .map(|x| x.len())
            .unwrap_or(0);
        total - seen
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use imap_codec::encode::Encoder;
    use imap_codec::imap_types::core::Vec1;
    use imap_codec::imap_types::fetch::Section;
    use imap_codec::imap_types::fetch::{MacroOrMessageDataItemNames, MessageDataItemName};
    use imap_codec::imap_types::response::Response;
    use imap_codec::ResponseCodec;
    use std::fs;

    use aero_collections::mail::mailbox::MailMeta;
    use aero_collections::mail::query::QueryResult;
    use aero_collections::unique_ident;
    use aero_user::cryptoblob;

    use crate::imap::index::MailIndex;
    use crate::imap::mime_view;

    #[test]
    fn mailview_body_ext() -> Result<()> {
        let ap = AttributesProxy::new(
            &MacroOrMessageDataItemNames::MessageDataItemNames(vec![
                MessageDataItemName::BodyExt {
                    section: Some(Section::Header(None)),
                    partial: None,
                    peek: false,
                },
            ]),
            &[],
            false,
        );

        let key = cryptoblob::gen_key();
        let meta = MailMeta {
            internaldate: 0u64,
            headers: vec![],
            message_key: key,
            rfc822_size: 8usize,
        };

        let index_entry = (NonZeroU32::MIN, NonZeroU64::MIN, vec![]);
        let mail_in_idx = MailIndex {
            i: NonZeroU32::MIN,
            uid: index_entry.0,
            modseq: index_entry.1,
            uuid: unique_ident::gen_ident(),
            flags: &index_entry.2,
        };
        let rfc822 = b"Subject: hello\r\nFrom: a@a.a\r\nTo: b@b.b\r\nDate: Thu, 12 Oct 2023 08:45:28 +0000\r\n\r\nhello world";
        let qr = QueryResult::FullResult {
            uuid: mail_in_idx.uuid.clone(),
            metadata: meta,
            content: rfc822.to_vec(),
        };

        let mv = MailView::new(&qr, &mail_in_idx)?;
        let (res_body, _seen) = mv.filter(&ap)?;

        let fattr = match res_body {
            Body::Data(Data::Fetch {
                seq: _seq,
                items: attr,
            }) => Ok(attr),
            _ => Err(anyhow!("Not a fetch body")),
        }?;

        assert_eq!(fattr.as_ref().len(), 1);

        let (sec, _orig, _data) = match &fattr.as_ref()[0] {
            MessageDataItem::BodyExt {
                section,
                origin,
                data,
            } => Ok((section, origin, data)),
            _ => Err(anyhow!("not a body ext message attribute")),
        }?;

        assert_eq!(sec.as_ref().unwrap(), &Section::Header(None));

        Ok(())
    }

    /// Future automated test. We use lossy utf8 conversion + lowercase everything,
    /// so this test might allow invalid results. But at least it allows us to quickly test a
    /// large variety of emails.
    /// Keep in mind that special cases must still be tested manually!
    #[test]
    fn fetch_body() -> Result<()> {
        let prefixes = [
            /* *** MY OWN DATASET *** */
            "tests/emails/dxflrs/0001_simple",
            "tests/emails/dxflrs/0002_mime",
            "tests/emails/dxflrs/0003_mime-in-mime",
            "tests/emails/dxflrs/0004_msg-in-msg",
            // eml_codec do not support continuation for the moment
            //"tests/emails/dxflrs/0005_mail-parser-readme",
            "tests/emails/dxflrs/0006_single-mime",
            "tests/emails/dxflrs/0007_raw_msg_in_rfc822",
            /* *** (STRANGE) RFC *** */
            //"tests/emails/rfc/000", // must return text/enriched, we return text/plain
            //"tests/emails/rfc/001", // does not recognize the multipart/external-body, breaks the
                                      // whole parsing
            //"tests/emails/rfc/002", // wrong date in email

            //"tests/emails/rfc/003", // dovecot fixes \r\r: the bytes number is wrong + text/enriched

            /* *** THIRD PARTY *** */
            //"tests/emails/thirdparty/000", // dovecot fixes \r\r: the bytes number is wrong
            //"tests/emails/thirdparty/001", // same
            "tests/emails/thirdparty/002", // same

                                           /* *** LEGACY *** */
                                           //"tests/emails/legacy/000", // same issue with \r\r
        ];

        for pref in prefixes.iter() {
            println!("{}", pref);
            let txt = fs::read(format!("../{}.eml", pref))?;
            let oracle = fs::read(format!("../{}.dovecot.body", pref))?;
            let message = eml_codec::parse_message(&txt).unwrap().1;

            let test_repr = Response::Data(Data::Fetch {
                seq: NonZeroU32::new(1).unwrap(),
                items: Vec1::from(MessageDataItem::Body(mime_view::bodystructure(
                    &message.child,
                    false,
                )?)),
            });
            let test_bytes = ResponseCodec::new().encode(&test_repr).dump();
            let test_str = String::from_utf8_lossy(&test_bytes).to_lowercase();

            let oracle_str =
                format!("* 1 FETCH {}\r\n", String::from_utf8_lossy(&oracle)).to_lowercase();

            println!("aerogramme: {}\n\ndovecot:    {}\n\n", test_str, oracle_str);
            //println!("\n\n {} \n\n", String::from_utf8_lossy(&resp));
            assert_eq!(test_str, oracle_str);
        }

        Ok(())
    }
}
