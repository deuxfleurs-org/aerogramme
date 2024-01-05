use std::num::NonZeroU32;
use std::sync::Arc;

use anyhow::{anyhow, bail, Error, Result};

use futures::stream::{FuturesOrdered, StreamExt};

use imap_codec::imap_types::core::Charset;
use imap_codec::imap_types::fetch::{MacroOrMessageDataItemNames, MessageDataItem};
use imap_codec::imap_types::flag::{Flag, FlagFetch, FlagPerm, StoreResponse, StoreType};
use imap_codec::imap_types::response::{Code, Data, Status};
use imap_codec::imap_types::search::SearchKey;
use imap_codec::imap_types::sequence::{self, SequenceSet};

use crate::imap::attributes::AttributesProxy;
use crate::imap::flags;
use crate::imap::mail_view::SeenFlag;
use crate::imap::response::Body;
use crate::imap::search;
use crate::imap::selectors::MailSelectionBuilder;
use crate::mail::mailbox::Mailbox;
use crate::mail::uidindex::{ImapUid, ImapUidvalidity, UidIndex};
use crate::mail::unique_ident::UniqueIdent;

const DEFAULT_FLAGS: [Flag; 5] = [
    Flag::Seen,
    Flag::Answered,
    Flag::Flagged,
    Flag::Deleted,
    Flag::Draft,
];

/// A MailboxView is responsible for giving the client the information
/// it needs about a mailbox, such as an initial summary of the mailbox's
/// content and continuous updates indicating when the content
/// of the mailbox has been changed.
/// To do this, it keeps a variable `known_state` that corresponds to
/// what the client knows, and produces IMAP messages to be sent to the
/// client that go along updates to `known_state`.
pub struct MailboxView {
    pub(crate) mailbox: Arc<Mailbox>,
    known_state: UidIndex,
}

impl MailboxView {
    /// Creates a new IMAP view into a mailbox.
    pub async fn new(mailbox: Arc<Mailbox>) -> Self {
        let state = mailbox.current_uid_index().await;

        Self {
            mailbox,
            known_state: state,
        }
    }

    /// Create an updated view, useful to make a diff
    /// between what the client knows and new stuff
    /// Produces a set of IMAP responses describing the change between
    /// what the client knows and what is actually in the mailbox.
    /// This does NOT trigger a sync, it bases itself on what is currently
    /// loaded in RAM by Bayou.
    pub async fn update(&mut self) -> Result<Vec<Body<'static>>> {
        let old_view: &mut Self = self;
        let new_view = Self {
            mailbox: old_view.mailbox.clone(),
            known_state: old_view.mailbox.current_uid_index().await,
        };

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
        for (i, (_uid, uuid)) in old_view.known_state.idx_by_uid.iter().enumerate() {
            if !new_view.known_state.table.contains_key(uuid) {
                data.push(Body::Data(Data::Expunge(
                    NonZeroU32::try_from((i + 1 - n_expunge) as u32).unwrap(),
                )));
                n_expunge += 1;
            }
        }

        // - if new mails arrived, notify client of number of existing mails
        if new_view.known_state.table.len() != old_view.known_state.table.len() - n_expunge
            || new_view.known_state.uidvalidity != old_view.known_state.uidvalidity
        {
            data.push(new_view.exists_status()?);
        }

        if new_view.known_state.uidvalidity != old_view.known_state.uidvalidity {
            // TODO: do we want to push less/more info than this?
            data.push(new_view.uidvalidity_status()?);
            data.push(new_view.uidnext_status()?);
        } else {
            // - if flags changed for existing mails, tell client
            for (i, (_uid, uuid)) in new_view.known_state.idx_by_uid.iter().enumerate() {
                let old_mail = old_view.known_state.table.get(uuid);
                let new_mail = new_view.known_state.table.get(uuid);
                if old_mail.is_some() && old_mail != new_mail {
                    if let Some((uid, flags)) = new_mail {
                        data.push(Body::Data(Data::Fetch {
                            seq: NonZeroU32::try_from((i + 1) as u32).unwrap(),
                            items: vec![
                                MessageDataItem::Uid(*uid),
                                MessageDataItem::Flags(
                                    flags.iter().filter_map(|f| flags::from_str(f)).collect(),
                                ),
                            ]
                            .try_into()?,
                        }));
                    }
                }
            }
        }
        *old_view = new_view;
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

        Ok(data)
    }

    pub async fn store<'a>(
        &mut self,
        sequence_set: &SequenceSet,
        kind: &StoreType,
        _response: &StoreResponse,
        flags: &[Flag<'a>],
        is_uid_store: &bool,
    ) -> Result<Vec<Body<'static>>> {
        self.mailbox.opportunistic_sync().await?;

        let flags = flags.iter().map(|x| x.to_string()).collect::<Vec<_>>();

        let mails = self.get_mail_ids(sequence_set, *is_uid_store)?;
        for mi in mails.iter() {
            match kind {
                StoreType::Add => {
                    self.mailbox.add_flags(mi.uuid, &flags[..]).await?;
                }
                StoreType::Remove => {
                    self.mailbox.del_flags(mi.uuid, &flags[..]).await?;
                }
                StoreType::Replace => {
                    self.mailbox.set_flags(mi.uuid, &flags[..]).await?;
                }
            }
        }

        // @TODO: handle _response
        self.update().await
    }

    pub async fn expunge(&mut self) -> Result<Vec<Body<'static>>> {
        self.mailbox.opportunistic_sync().await?;

        let deleted_flag = Flag::Deleted.to_string();
        let state = self.mailbox.current_uid_index().await;
        let msgs = state
            .table
            .iter()
            .filter(|(_uuid, (_uid, flags))| flags.iter().any(|x| *x == deleted_flag))
            .map(|(uuid, _)| *uuid);

        for msg in msgs {
            self.mailbox.delete(msg).await?;
        }

        self.update().await
    }

    pub async fn copy(
        &self,
        sequence_set: &SequenceSet,
        to: Arc<Mailbox>,
        is_uid_copy: &bool,
    ) -> Result<(ImapUidvalidity, Vec<(ImapUid, ImapUid)>)> {
        let mails = self.get_mail_ids(sequence_set, *is_uid_copy)?;

        let mut new_uuids = vec![];
        for mi in mails.iter() {
            new_uuids.push(to.copy_from(&self.mailbox, mi.uuid).await?);
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
        let mails = self.get_mail_ids(sequence_set, *is_uid_copy)?;

        let mut new_uuids = vec![];
        for mi in mails.iter() {
            let copy_action = to.copy_from(&self.mailbox, mi.uuid).await?;
            new_uuids.push(copy_action);
            self.mailbox.delete(mi.uuid).await?
        }

        let mut ret = vec![];
        let to_state = to.current_uid_index().await;
        for (mi, new_uuid) in mails.iter().zip(new_uuids.iter()) {
            let dest_uid = to_state
                .table
                .get(new_uuid)
                .ok_or(anyhow!("moved mail not in destination mailbox"))?
                .0;
            ret.push((mi.uid, dest_uid));
        }

        let update = self.update().await?;

        Ok((to_state.uidvalidity, ret, update))
    }

    /// Looks up state changes in the mailbox and produces a set of IMAP
    /// responses describing the new state.
    pub async fn fetch<'b>(
        &self,
        sequence_set: &SequenceSet,
        attributes: &'b MacroOrMessageDataItemNames<'static>,
        is_uid_fetch: &bool,
    ) -> Result<Vec<Body<'static>>> {
        let ap = AttributesProxy::new(attributes, *is_uid_fetch);

        // Prepare data
        let mids = MailIdentifiersList(self.get_mail_ids(sequence_set, *is_uid_fetch)?);
        let mail_count = mids.0.len();
        let uuids = mids.uuids();
        let meta = self.mailbox.fetch_meta(&uuids).await?;
        let flags = uuids
            .iter()
            .map(|uuid| {
                self.known_state
                    .table
                    .get(uuid)
                    .map(|(_uuid, f)| f)
                    .ok_or(anyhow!("missing email from the flag table"))
            })
            .collect::<Result<Vec<_>, _>>()?;

        // Start filling data to build the view
        let mut selection = MailSelectionBuilder::new(ap.need_body(), mail_count);
        selection
            .with_mail_identifiers(&mids.0)
            .with_metadata(&meta)
            .with_flags(&flags);

        // Asynchronously fetch full bodies (if needed)
        let btc = selection.bodies_to_collect();
        let future_bodies = btc
            .iter()
            .map(|bi| async move {
                let body = self.mailbox.fetch_full(*bi.msg_uuid, bi.msg_key).await?;
                Ok::<_, anyhow::Error>(body)
            })
            .collect::<FuturesOrdered<_>>();
        let bodies = future_bodies
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;

        // Add bodies
        selection.with_bodies(bodies.as_slice());

        // Build mail selection views
        let views = selection.build()?;

        // Filter views to build the result
        // Also identify what must be put as seen
        let filtered_view = views
            .iter()
            .filter_map(|mv| mv.filter(&ap).ok().map(|(body, seen)| (mv, body, seen)))
            .collect::<Vec<_>>();

        // Register seen flags
        let future_flags = filtered_view
            .iter()
            .filter(|(_mv, _body, seen)| matches!(seen, SeenFlag::MustAdd))
            .map(|(mv, _body, _seen)| async move {
                let seen_flag = Flag::Seen.to_string();
                self.mailbox.add_flags(mv.ids.uuid, &[seen_flag]).await?;
                Ok::<_, anyhow::Error>(())
            })
            .collect::<FuturesOrdered<_>>();

        future_flags
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<_, _>>()?;

        let command_body = filtered_view
            .into_iter()
            .map(|(_mv, body, _seen)| body)
            .collect::<Vec<_>>();

        Ok(command_body)
    }

    /// A very naive search implementation...
    pub async fn search<'a>(
        &self,
        _charset: &Option<Charset<'a>>,
        search_key: &SearchKey<'a>,
        uid: bool,
    ) -> Result<Vec<Body<'static>>> {
        let (seq_set, seq_type) = search::Criteria(search_key).to_sequence_set();
        let mailids = MailIdentifiersList(self.get_mail_ids(&seq_set, seq_type.is_uid())?);
        let mail_u32 = match uid {
            true => mailids.uids(),
            _ => mailids.ids(),
        };
        Ok(vec![Body::Data(Data::Search(mail_u32))])
    }

    // ----

    // Gets the IMAP ID, the IMAP UIDs and, the Aerogramme UUIDs of mails identified by a SequenceSet of
    // sequence numbers (~ IMAP selector)
    fn get_mail_ids(
        &self,
        sequence_set: &SequenceSet,
        by_uid: bool,
    ) -> Result<Vec<MailIdentifiers>> {
        let mail_vec = self
            .known_state
            .idx_by_uid
            .iter()
            .map(|(uid, uuid)| (*uid, *uuid))
            .collect::<Vec<_>>();

        let mut mails = vec![];

        if by_uid {
            if mail_vec.is_empty() {
                return Ok(vec![]);
            }
            let iter_strat = sequence::Strategy::Naive {
                largest: mail_vec.last().unwrap().0,
            };

            let mut i = 0;
            for uid in sequence_set.iter(iter_strat) {
                while mail_vec.get(i).map(|mail| mail.0 < uid).unwrap_or(false) {
                    i += 1;
                }
                if let Some(mail) = mail_vec.get(i) {
                    if mail.0 == uid {
                        mails.push(MailIdentifiers {
                            i: NonZeroU32::try_from(i as u32 + 1).unwrap(),
                            uid: mail.0,
                            uuid: mail.1,
                        });
                    }
                } else {
                    break;
                }
            }
        } else {
            if mail_vec.is_empty() {
                bail!("No such message (mailbox is empty)");
            }

            let iter_strat = sequence::Strategy::Naive {
                largest: NonZeroU32::try_from((mail_vec.len()) as u32).unwrap(),
            };

            for i in sequence_set.iter(iter_strat) {
                if let Some(mail) = mail_vec.get(i.get() as usize - 1) {
                    mails.push(MailIdentifiers {
                        i,
                        uid: mail.0,
                        uuid: mail.1,
                    });
                } else {
                    bail!("No such mail: {}", i);
                }
            }
        }

        Ok(mails)
    }

    // ----

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
        self.known_state.uidvalidity
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
        self.known_state.uidnext
    }

    /// Produce an EXISTS message corresponding to the number of mails
    /// in `known_state`
    fn exists_status(&self) -> Result<Body<'static>> {
        Ok(Body::Data(Data::Exists(self.exists()?)))
    }

    pub(crate) fn exists(&self) -> Result<u32> {
        Ok(u32::try_from(self.known_state.idx_by_uid.len())?)
    }

    /// Produce a RECENT message corresponding to the number of
    /// recent mails in `known_state`
    fn recent_status(&self) -> Result<Body<'static>> {
        Ok(Body::Data(Data::Recent(self.recent()?)))
    }

    pub(crate) fn recent(&self) -> Result<u32> {
        let recent = self
            .known_state
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
            .known_state
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
        let total = self.known_state.table.len();
        let seen = self
            .known_state
            .idx_by_flag
            .get(&Flag::Seen.to_string())
            .map(|x| x.len())
            .unwrap_or(0);
        total - seen
    }
}

pub struct MailIdentifiers {
    pub i: NonZeroU32,
    pub uid: ImapUid,
    pub uuid: UniqueIdent,
}
pub struct MailIdentifiersList(Vec<MailIdentifiers>);

impl MailIdentifiersList {
    fn ids(&self) -> Vec<NonZeroU32> {
        self.0.iter().map(|mi| mi.i).collect()
    }
    fn uids(&self) -> Vec<ImapUid> {
        self.0.iter().map(|mi| mi.uid).collect()
    }
    fn uuids(&self) -> Vec<UniqueIdent> {
        self.0.iter().map(|mi| mi.uuid).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use imap_codec::encode::Encoder;
    use imap_codec::imap_types::core::NonEmptyVec;
    use imap_codec::imap_types::fetch::Section;
    use imap_codec::imap_types::fetch::{MacroOrMessageDataItemNames, MessageDataItemName};
    use imap_codec::imap_types::response::Response;
    use imap_codec::ResponseCodec;
    use std::fs;

    use crate::cryptoblob;
    use crate::imap::mail_view::{FetchedMail, MailView};
    use crate::imap::mime_view;
    use crate::mail::mailbox::MailMeta;
    use crate::mail::unique_ident;

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
            false,
        );

        let flags = vec![];
        let key = cryptoblob::gen_key();
        let meta = MailMeta {
            internaldate: 0u64,
            headers: vec![],
            message_key: key,
            rfc822_size: 8usize,
        };
        let ids = MailIdentifiers {
            i: NonZeroU32::MIN,
            uid: NonZeroU32::MIN,
            uuid: unique_ident::gen_ident(),
        };
        let rfc822 = b"Subject: hello\r\nFrom: a@a.a\r\nTo: b@b.b\r\nDate: Thu, 12 Oct 2023 08:45:28 +0000\r\n\r\nhello world";
        let content = FetchedMail::new_from_message(eml_codec::parse_message(rfc822)?.1);

        let mv = MailView {
            ids: &ids,
            content,
            meta: &meta,
            flags: &flags,
        };
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
            let txt = fs::read(format!("{}.eml", pref))?;
            let oracle = fs::read(format!("{}.dovecot.body", pref))?;
            let message = eml_codec::parse_message(&txt).unwrap().1;

            let test_repr = Response::Data(Data::Fetch {
                seq: NonZeroU32::new(1).unwrap(),
                items: NonEmptyVec::from(MessageDataItem::Body(mime_view::bodystructure(
                    &message.child,
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
