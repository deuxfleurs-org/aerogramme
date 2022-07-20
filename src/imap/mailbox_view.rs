use std::borrow::Cow;

use std::num::NonZeroU32;
use std::sync::Arc;

use anyhow::{anyhow, bail, Error, Result};
use boitalettres::proto::res::body::Data as Body;
use chrono::{Offset, TimeZone, Utc};
use futures::stream::{FuturesOrdered, StreamExt};
use imap_codec::types::address::Address;
use imap_codec::types::body::{BasicFields, Body as FetchBody, BodyStructure, SpecificFields};
use imap_codec::types::core::{AString, Atom, IString, NString};
use imap_codec::types::datetime::MyDateTime;
use imap_codec::types::envelope::Envelope;
use imap_codec::types::fetch_attributes::{
    FetchAttribute, MacroOrFetchAttributes, Section as FetchSection,
};
use imap_codec::types::flag::{Flag, StoreResponse, StoreType};
use imap_codec::types::response::{Code, Data, MessageAttribute, Status};
use imap_codec::types::sequence::{self, SequenceSet};
use mail_parser::*;

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
    /// Generates the necessary IMAP messages so that the client
    /// has a satisfactory summary of the current mailbox's state.
    /// These are the messages that are sent in response to a SELECT command.
    pub async fn new(mailbox: Arc<Mailbox>) -> Result<(Self, Vec<Body>)> {
        let state = mailbox.current_uid_index().await;

        let new_view = Self {
            mailbox,
            known_state: state,
        };

        let mut data = Vec::<Body>::new();
        data.push(new_view.exists_status()?);
        data.push(new_view.recent_status()?);
        data.extend(new_view.flags_status()?.into_iter());
        data.push(new_view.uidvalidity_status()?);
        data.push(new_view.uidnext_status()?);

        Ok((new_view, data))
    }

    /// Produces a set of IMAP responses describing the change between
    /// what the client knows and what is actually in the mailbox.
    /// This does NOT trigger a sync, it bases itself on what is currently
    /// loaded in RAM by Bayou.
    pub async fn update(&mut self) -> Result<Vec<Body>> {
        let new_view = MailboxView {
            mailbox: self.mailbox.clone(),
            known_state: self.mailbox.current_uid_index().await,
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
        for (i, (_uid, uuid)) in self.known_state.idx_by_uid.iter().enumerate() {
            if !new_view.known_state.table.contains_key(uuid) {
                data.push(Body::Data(Data::Expunge(
                    NonZeroU32::try_from((i + 1 - n_expunge) as u32).unwrap(),
                )));
                n_expunge += 1;
            }
        }

        // - if new mails arrived, notify client of number of existing mails
        if new_view.known_state.table.len() != self.known_state.table.len() - n_expunge
            || new_view.known_state.uidvalidity != self.known_state.uidvalidity
        {
            data.push(new_view.exists_status()?);
        }

        if new_view.known_state.uidvalidity != self.known_state.uidvalidity {
            // TODO: do we want to push less/more info than this?
            data.push(new_view.uidvalidity_status()?);
            data.push(new_view.uidnext_status()?);
        } else {
            // - if flags changed for existing mails, tell client
            for (i, (_uid, uuid)) in new_view.known_state.idx_by_uid.iter().enumerate() {
                let old_mail = self.known_state.table.get(uuid);
                let new_mail = new_view.known_state.table.get(uuid);
                if old_mail.is_some() && old_mail != new_mail {
                    if let Some((uid, flags)) = new_mail {
                        data.push(Body::Data(Data::Fetch {
                            seq_or_uid: NonZeroU32::try_from((i + 1) as u32).unwrap(),
                            attributes: vec![
                                MessageAttribute::Uid((*uid).try_into().unwrap()),
                                MessageAttribute::Flags(
                                    flags.iter().filter_map(|f| string_to_flag(f)).collect(),
                                ),
                            ],
                        }));
                    }
                }
            }
        }

        *self = new_view;
        Ok(data)
    }

    pub async fn store(
        &mut self,
        sequence_set: &SequenceSet,
        kind: &StoreType,
        _response: &StoreResponse,
        flags: &[Flag],
        is_uid_store: &bool,
    ) -> Result<Vec<Body>> {
        self.mailbox.opportunistic_sync().await?;

        let flags = flags.iter().map(|x| x.to_string()).collect::<Vec<_>>();

        let mails = self.get_mail_ids(sequence_set, *is_uid_store)?;
        for (_i, _uid, uuid) in mails.iter() {
            match kind {
                StoreType::Add => {
                    self.mailbox.add_flags(*uuid, &flags[..]).await?;
                }
                StoreType::Remove => {
                    self.mailbox.del_flags(*uuid, &flags[..]).await?;
                }
                StoreType::Replace => {
                    self.mailbox.set_flags(*uuid, &flags[..]).await?;
                }
            }
        }

        self.update().await
    }

    pub async fn expunge(&mut self) -> Result<Vec<Body>> {
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

    /// Looks up state changes in the mailbox and produces a set of IMAP
    /// responses describing the new state.
    pub async fn fetch(
        &self,
        sequence_set: &SequenceSet,
        attributes: &MacroOrFetchAttributes,
        is_uid_fetch: &bool,
    ) -> Result<Vec<Body>> {
        let mails = self.get_mail_ids(sequence_set, *is_uid_fetch)?;

        let mails_uuid = mails
            .iter()
            .map(|(_i, _uid, uuid)| *uuid)
            .collect::<Vec<_>>();
        let mails_meta = self.mailbox.fetch_meta(&mails_uuid).await?;

        let mut fetch_attrs = match attributes {
            MacroOrFetchAttributes::Macro(m) => m.expand(),
            MacroOrFetchAttributes::FetchAttributes(a) => a.clone(),
        };
        if *is_uid_fetch && !fetch_attrs.contains(&FetchAttribute::Uid) {
            fetch_attrs.push(FetchAttribute::Uid);
        }
        let need_body = fetch_attrs.iter().any(|x| {
            matches!(
                x,
                FetchAttribute::Body
                    | FetchAttribute::BodyExt { .. }
                    | FetchAttribute::Rfc822
                    | FetchAttribute::Rfc822Text
                    | FetchAttribute::BodyStructure
            )
        });

        let mails = if need_body {
            let mut iter = mails
                .into_iter()
                .zip(mails_meta.into_iter())
                .map(|((i, uid, uuid), meta)| async move {
                    let body = self.mailbox.fetch_full(uuid, &meta.message_key).await?;
                    Ok::<_, anyhow::Error>((i, uid, uuid, meta, Some(body)))
                })
                .collect::<FuturesOrdered<_>>();
            let mut mails = vec![];
            while let Some(m) = iter.next().await {
                mails.push(m?);
            }
            mails
        } else {
            mails
                .into_iter()
                .zip(mails_meta.into_iter())
                .map(|((i, uid, uuid), meta)| (i, uid, uuid, meta, None))
                .collect::<Vec<_>>()
        };

        let mut ret = vec![];
        for (i, uid, uuid, meta, body) in mails {
            let mut attributes = vec![];

            let (_uid2, flags) = self
                .known_state
                .table
                .get(&uuid)
                .ok_or_else(|| anyhow!("Mail not in uidindex table: {}", uuid))?;

            let parsed = match &body {
                Some(m) => {
                    mail_parser::Message::parse(m).ok_or_else(|| anyhow!("Invalid mail body"))?
                }
                None => mail_parser::Message::parse(&meta.headers)
                    .ok_or_else(|| anyhow!("Invalid mail headers"))?,
            };

            for attr in fetch_attrs.iter() {
                match attr {
                    FetchAttribute::Uid => attributes.push(MessageAttribute::Uid(uid)),
                    FetchAttribute::Flags => {
                        attributes.push(MessageAttribute::Flags(
                            flags.iter().filter_map(|f| string_to_flag(f)).collect(),
                        ));
                    }
                    FetchAttribute::Rfc822Size => {
                        attributes.push(MessageAttribute::Rfc822Size(meta.rfc822_size as u32))
                    }
                    FetchAttribute::Rfc822Header => {
                        attributes.push(MessageAttribute::Rfc822Header(NString(
                            meta.headers.to_vec().try_into().ok().map(IString::Literal),
                        )))
                    }
                    FetchAttribute::Rfc822Text => {
                        let r = parsed
                            .raw_message.get(parsed.offset_body..parsed.offset_end)
                            .ok_or(Error::msg("Unable to extract email body, cursors out of bound. This is a bug."))?;

                        attributes.push(MessageAttribute::Rfc822Text(NString(
                            r.try_into().ok().map(IString::Literal),
                        )));
                    }
                    FetchAttribute::Rfc822 => attributes.push(MessageAttribute::Rfc822(NString(
                        body.as_ref()
                            .unwrap()
                            .clone()
                            .try_into()
                            .ok()
                            .map(IString::Literal),
                    ))),
                    FetchAttribute::Envelope => {
                        attributes.push(MessageAttribute::Envelope(message_envelope(&parsed)))
                    }
                    FetchAttribute::Body => attributes.push(MessageAttribute::Body(
                        build_imap_email_struct(&parsed, &parsed.structure)?,
                    )),
                    FetchAttribute::BodyStructure => attributes.push(MessageAttribute::Body(
                        build_imap_email_struct(&parsed, &parsed.structure)?,
                    )),
                    FetchAttribute::BodyExt {
                        section,
                        partial,
                        peek,
                    } => {
                        // @TODO Add missing section specifiers
                        match get_message_section(&parsed, section) {
                            Ok(text) => {
                                let seen_flag = Flag::Seen.to_string();
                                if !peek && !flags.iter().any(|x| *x == seen_flag) {
                                    // Add \Seen flag
                                    self.mailbox.add_flags(uuid, &[seen_flag]).await?;
                                }

                                let (text, origin) = match partial {
                                    Some((begin, len)) => {
                                        if *begin as usize > text.len() {
                                            (&[][..], Some(*begin))
                                        } else if (*begin + len.get()) as usize >= text.len() {
                                            (&text[*begin as usize..], Some(*begin))
                                        } else {
                                            (
                                                &text[*begin as usize
                                                    ..(*begin + len.get()) as usize],
                                                Some(*begin),
                                            )
                                        }
                                    }
                                    None => (&text[..], None),
                                };

                                let data =
                                    NString(text.to_vec().try_into().ok().map(IString::Literal));
                                attributes.push(MessageAttribute::BodyExt {
                                    section: section.clone(),
                                    origin,
                                    data,
                                })
                            }
                            Err(e) => {
                                tracing::error!(
                                    "Could not get section {:?} of message {}: {}",
                                    section,
                                    uuid,
                                    e
                                );
                            }
                        }
                    }
                    FetchAttribute::InternalDate => {
                        attributes.push(MessageAttribute::InternalDate(MyDateTime(
                            Utc.fix()
                                .timestamp(i64::try_from(meta.internaldate / 1000)?, 0),
                        )));
                    }
                }
            }

            ret.push(Body::Data(Data::Fetch {
                seq_or_uid: i,
                attributes,
            }));
        }

        Ok(ret)
    }

    // ----

    // Gets the UIDs and UUIDs of mails identified by a SequenceSet of
    // sequence numbers
    fn get_mail_ids(
        &self,
        sequence_set: &SequenceSet,
        by_uid: bool,
    ) -> Result<Vec<(NonZeroU32, ImapUid, UniqueIdent)>> {
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
                        mails.push((NonZeroU32::try_from(i as u32 + 1).unwrap(), mail.0, mail.1));
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
                    mails.push((i, mail.0, mail.1));
                } else {
                    bail!("No such mail: {}", i);
                }
            }
        }

        Ok(mails)
    }

    // ----

    /// Produce an OK [UIDVALIDITY _] message corresponding to `known_state`
    fn uidvalidity_status(&self) -> Result<Body> {
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
    fn uidnext_status(&self) -> Result<Body> {
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
    fn exists_status(&self) -> Result<Body> {
        Ok(Body::Data(Data::Exists(self.exists()?)))
    }

    pub(crate) fn exists(&self) -> Result<u32> {
        Ok(u32::try_from(self.known_state.idx_by_uid.len())?)
    }

    /// Produce a RECENT message corresponding to the number of
    /// recent mails in `known_state`
    fn recent_status(&self) -> Result<Body> {
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
    fn flags_status(&self) -> Result<Vec<Body>> {
        let mut flags: Vec<Flag> = self
            .known_state
            .idx_by_flag
            .flags()
            .map(|f| string_to_flag(f))
            .flatten()
            .collect();
        for f in DEFAULT_FLAGS.iter() {
            if !flags.contains(f) {
                flags.push(f.clone());
            }
        }
        let mut ret = vec![Body::Data(Data::Flags(flags.clone()))];

        flags.push(Flag::Permanent);
        let permanent_flags =
            Status::ok(None, Some(Code::PermanentFlags(flags)), "Flags permitted")
                .map_err(Error::msg)?;
        ret.push(Body::Status(permanent_flags));

        Ok(ret)
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

fn string_to_flag(f: &str) -> Option<Flag> {
    match f.chars().next() {
        Some('\\') => match f {
            "\\Seen" => Some(Flag::Seen),
            "\\Answered" => Some(Flag::Answered),
            "\\Flagged" => Some(Flag::Flagged),
            "\\Deleted" => Some(Flag::Deleted),
            "\\Draft" => Some(Flag::Draft),
            "\\Recent" => Some(Flag::Recent),
            _ => match Atom::try_from(f.strip_prefix('\\').unwrap().clone()) {
                Err(_) => {
                    tracing::error!(flag=%f, "Unable to encode flag as IMAP atom");
                    None
                }
                Ok(a) => Some(Flag::Extension(a)),
            },
        },
        Some(_) => match Atom::try_from(f.clone()) {
            Err(_) => {
                tracing::error!(flag=%f, "Unable to encode flag as IMAP atom");
                None
            }
            Ok(a) => Some(Flag::Keyword(a)),
        },
        None => None,
    }
}

/// Envelope rules are defined in RFC 3501, section 7.4.2
/// https://datatracker.ietf.org/doc/html/rfc3501#section-7.4.2
///
/// Some important notes:
///
/// If the Sender or Reply-To lines are absent in the [RFC-2822]
/// header, or are present but empty, the server sets the
/// corresponding member of the envelope to be the same value as
/// the from member (the client is not expected to know to do
/// this). Note: [RFC-2822] requires that all messages have a valid
/// From header.  Therefore, the from, sender, and reply-to
/// members in the envelope can not be NIL.
///
/// If the Date, Subject, In-Reply-To, and Message-ID header lines
/// are absent in the [RFC-2822] header, the corresponding member
/// of the envelope is NIL; if these header lines are present but
/// empty the corresponding member of the envelope is the empty
/// string.

//@FIXME return an error if the envelope is invalid instead of panicking
//@FIXME some fields must be defaulted if there are not set.
fn message_envelope(msg: &mail_parser::Message<'_>) -> Envelope {
    let from = convert_addresses(msg.get_from()).unwrap_or(vec![]);

    Envelope {
        date: NString(
            msg.get_date()
                .map(|d| IString::try_from(d.to_iso8601()).unwrap()),
        ),
        subject: NString(
            msg.get_subject()
                .map(|d| IString::try_from(d.to_string()).unwrap()),
        ),
        from: from.clone(),
        sender: convert_addresses(msg.get_sender()).unwrap_or(from.clone()),
        reply_to: convert_addresses(msg.get_reply_to()).unwrap_or(from.clone()),
        to: convert_addresses(msg.get_to()).unwrap_or(vec![]),
        cc: convert_addresses(msg.get_cc()).unwrap_or(vec![]),
        bcc: convert_addresses(msg.get_bcc()).unwrap_or(vec![]),
        in_reply_to: NString(None), // @TODO
        message_id: NString(
            msg.get_message_id()
                .map(|d| IString::try_from(d.to_string()).unwrap()),
        ),
    }
}

fn convert_addresses(a: &mail_parser::HeaderValue<'_>) -> Option<Vec<Address>> {
    match a {
        mail_parser::HeaderValue::Address(a) => Some(vec![convert_address(a)]),
        mail_parser::HeaderValue::AddressList(l) => {
            Some(l.iter().map(|a| convert_address(a)).collect())
        }
        mail_parser::HeaderValue::Empty => None,
        mail_parser::HeaderValue::Collection(c) => Some(
            c.iter()
                .map(|l| convert_addresses(l).unwrap_or(vec![]))
                .flatten()
                .collect(),
        ),
        _ => {
            tracing::warn!("Invalid address header");
            None
        }
    }
}

//@FIXME Remove unwrap
fn convert_address(a: &mail_parser::Addr<'_>) -> Address {
    let (user, host) = match &a.address {
        None => (None, None),
        Some(x) => match x.split_once('@') {
            Some((u, h)) => (Some(u.to_string()), Some(h.to_string())),
            None => (Some(x.to_string()), None),
        },
    };

    Address::new(
        NString(
            a.name
                .as_ref()
                .map(|x| IString::try_from(x.to_string()).unwrap()),
        ),
        // SMTP at-domain-list (source route) seems obsolete since at least 1991
        // https://www.mhonarc.org/archive/html/ietf-822/1991-06/msg00060.html
        NString(None),
        NString(user.map(|x| IString::try_from(x).unwrap())),
        NString(host.map(|x| IString::try_from(x).unwrap())),
    )
}

/*
--CAPTURE--
b fetch 29878:29879 (BODY)
* 29878 FETCH (BODY (("text" "plain" ("charset" "utf-8") NIL NIL "quoted-printable" 3264 82)("text" "html" ("charset" "utf-8") NIL NIL "quoted-printable" 31834 643) "alternative"))
* 29879 FETCH (BODY ("text" "html" ("charset" "us-ascii") NIL NIL "7bit" 4107 131))
                                   ^^^^^^^^^^^^^^^^^^^^^^ ^^^ ^^^ ^^^^^^ ^^^^ ^^^
                                   |                      |   |   |      |    | number of lines
                                   |                      |   |   |      | size
                                   |                      |   |   | content transfer encoding
                                   |                      |   | description
                                   |                      | id
                                   | parameter list
b OK Fetch completed (0.001 + 0.000 secs).
*/
fn build_imap_email_struct<'a>(
    msg: &Message<'a>,
    node: &MessageStructure,
) -> Result<BodyStructure> {
    match node {
        MessageStructure::Part(id) => {
            let part = msg.parts.get(*id).ok_or(anyhow!(
                "Email part referenced in email structure is missing"
            ))?;
            match part {
                MessagePart::Multipart(_) => {
                    unreachable!("A multipart entry can not be found here.")
                }
                MessagePart::Text(bp) | MessagePart::Html(bp) => {
                    let (attrs, mut basic) = headers_to_basic_fields(bp, bp.body.len())?;

                    // If the charset is not defined, set it to "us-ascii"
                    if attrs.charset.is_none() {
                        basic
                            .parameter_list
                            .push((unchecked_istring("charset"), unchecked_istring("us-ascii")));
                    }

                    // If the subtype is not defined, set it to "plain". MIME (RFC2045) says that subtype
                    // MUST be defined and hence has no default. But mail-parser does not make any
                    // difference between MIME and raw emails, hence raw emails have no subtypes.
                    let subtype = bp
                        .get_content_type()
                        .map(|h| h.c_subtype.as_ref())
                        .flatten()
                        .map(|st| IString::try_from(st.to_string()).ok())
                        .flatten()
                        .unwrap_or(unchecked_istring("plain"));

                    Ok(BodyStructure::Single {
                        body: FetchBody {
                            basic,
                            specific: SpecificFields::Text {
                                subtype,
                                number_of_lines: u32::try_from(
                                    // We do not count the number of lines but the number of line
                                    // feeds to have the same behavior as Dovecot and Cyrus.
                                    // 2 lines = 1 line feed.
                                    // @FIXME+BUG: if the body is base64-encoded, this returns the
                                    // number of lines in the decoded body, however we should
                                    // instead return the number of raw base64 lines
                                    bp.body.as_ref().chars().filter(|&c| c == '\n').count(),
                                )?,
                            },
                        },
                        extension: None,
                    })
                }
                MessagePart::Binary(bp) | MessagePart::InlineBinary(bp) => {
                    let (_, basic) = headers_to_basic_fields(bp, bp.body.len())?;

                    let ct = bp
                        .get_content_type()
                        .ok_or(anyhow!("Content-Type is missing but required here."))?;

                    let type_ =
                        IString::try_from(ct.c_type.as_ref().to_string()).map_err(|_| {
                            anyhow!("Unable to build IString from given Content-Type type given")
                        })?;

                    let subtype = IString::try_from(
                        ct.c_subtype
                            .as_ref()
                            .ok_or(anyhow!("Content-Type invalid, missing subtype"))?
                            .to_string(),
                    )
                    .map_err(|_| {
                        anyhow!("Unable to build IString from given Content-Type subtype given")
                    })?;

                    Ok(BodyStructure::Single {
                        body: FetchBody {
                            basic,
                            specific: SpecificFields::Basic { type_, subtype },
                        },
                        extension: None,
                    })
                }
                MessagePart::Message(bp) => {
                    // @NOTE in some cases mail-parser does not parse the MessageAttachment but
                    // provide it as raw body. By looking quickly at the code, it seems that the
                    // attachment is not parsed when mail-parser encounters some encoding problems.
                    match &bp.body {
                        MessageAttachment::Parsed(inner) => {
                            // @FIXME+BUG mail-parser does not handle ways when a MIME message contains
                            // a raw email and wrongly take its delimiter. The size and number of
                            // lines returned in that case are wrong. A patch to mail-parser is
                            // needed to fix this.
                            let (_, basic) = headers_to_basic_fields(bp, inner.raw_message.len())?;

                            // We do not count the number of lines but the number of line
                            // feeds to have the same behavior as Dovecot and Cyrus.
                            // 2 lines = 1 line feed.
                            let nol = inner.raw_message.iter().filter(|&c| c == &b'\n').count();

                            Ok(BodyStructure::Single {
                                body: FetchBody {
                                    basic,
                                    specific: SpecificFields::Message {
                                        envelope: message_envelope(inner),
                                        body_structure: Box::new(build_imap_email_struct(
                                            inner,
                                            &inner.structure,
                                        )?),

                                        // @FIXME This solution is bad for 2 reasons:
                                        // - RFC2045 says line endings are CRLF but we accept LF alone with
                                        // this method. It could be a feature (be liberal in what you
                                        // accept) but we must be sure that we don't break things.
                                        // - It should be done during parsing, we are iterating twice on
                                        // the same data which results in some wastes.
                                        number_of_lines: u32::try_from(nol)?,
                                    },
                                },
                                extension: None,
                            })
                        }
                        MessageAttachment::Raw(raw_msg) => {
                            let (_, basic) = headers_to_basic_fields(bp, raw_msg.len())?;

                            let ct = bp
                                .get_content_type()
                                .ok_or(anyhow!("Content-Type is missing but required here."))?;

                            let type_ =
                                IString::try_from(ct.c_type.as_ref().to_string()).map_err(|_| {
                                    anyhow!("Unable to build IString from given Content-Type type given")
                                })?;

                            let subtype = IString::try_from(
                                ct.c_subtype
                                    .as_ref()
                                    .ok_or(anyhow!("Content-Type invalid, missing subtype"))?
                                    .to_string(),
                            )
                            .map_err(|_| {
                                anyhow!(
                                    "Unable to build IString from given Content-Type subtype given"
                                )
                            })?;

                            Ok(BodyStructure::Single {
                                body: FetchBody {
                                    basic,
                                    specific: SpecificFields::Basic { type_, subtype },
                                },
                                extension: None,
                            })
                        }
                    }
                }
            }
        }
        MessageStructure::List(lp) => {
            let subtype = IString::try_from(
                msg.get_content_type()
                    .ok_or(anyhow!("Content-Type is missing but required here."))?
                    .c_subtype
                    .as_ref()
                    .ok_or(anyhow!("Content-Type invalid, missing subtype"))?
                    .to_string(),
            )
            .map_err(|_| {
                anyhow!("Unable to build IString from given Content-Type subtype given")
            })?;

            // @NOTE we should use try_collect() but it is unstable as of 2022-07-05
            Ok(BodyStructure::Multi {
                bodies: lp
                    .iter()
                    .map(|inner_node| build_imap_email_struct(msg, inner_node))
                    .fold(Ok(vec![]), try_collect_shime)?,
                subtype,
                extension_data: None,
            })
        }
        MessageStructure::MultiPart((id, lp)) => {
            let part = msg
                .parts
                .get(*id)
                .map(|p| match p {
                    MessagePart::Multipart(mp) => Some(mp),
                    _ => None,
                })
                .flatten()
                .ok_or(anyhow!(
                    "Email part referenced in email structure is missing"
                ))?;

            let subtype = IString::try_from(
                part.headers_rfc
                    .get(&RfcHeader::ContentType)
                    .ok_or(anyhow!("Content-Type is missing but required here."))?
                    .get_content_type()
                    .c_subtype
                    .as_ref()
                    .ok_or(anyhow!("Content-Type invalid, missing subtype"))?
                    .to_string(),
            )
            .map_err(|_| {
                anyhow!("Unable to build IString from given Content-Type subtype given")
            })?;

            Ok(BodyStructure::Multi {
                bodies: lp
                    .iter()
                    .map(|inner_node| build_imap_email_struct(msg, inner_node))
                    .fold(Ok(vec![]), try_collect_shime)?,
                subtype,
                extension_data: None,
                /*Some(MultipartExtensionData {
                    parameter_list: vec![],
                    disposition: None,
                    language: None,
                    location: None,
                    extension: vec![],
                })*/
            })
        }
    }
}

fn try_collect_shime<T>(acc: Result<Vec<T>>, elem: Result<T>) -> Result<Vec<T>> {
    match (acc, elem) {
        (Err(e), _) | (_, Err(e)) => Err(e),
        (Ok(mut ac), Ok(el)) => {
            ac.push(el);
            Ok(ac)
        }
    }
}

/// s is set to static to ensure that only compile time values
/// checked by developpers are passed.
fn unchecked_istring(s: &'static str) -> IString {
    IString::try_from(s).expect("this value is expected to be a valid imap-codec::IString")
}

#[derive(Default)]
struct SpecialAttrs<'a> {
    charset: Option<&'a Cow<'a, str>>,
    boundary: Option<&'a Cow<'a, str>>,
}

/// Takes mail-parser Content-Type attributes, build imap-codec BasicFields.parameter_list and
/// identify some specific attributes (charset and boundary).
fn attrs_to_params<'a>(bp: &impl MimeHeaders<'a>) -> (SpecialAttrs, Vec<(IString, IString)>) {
    // Try to extract Content-Type attributes from headers
    let attrs = match bp
        .get_content_type()
        .map(|c| c.attributes.as_ref())
        .flatten()
    {
        Some(v) => v,
        _ => return (SpecialAttrs::default(), vec![]),
    };

    // Transform the Content-Type attributes into IMAP's parameter list
    // Also collect some special attributes that might be used elsewhere
    attrs.iter().fold(
        (SpecialAttrs::default(), vec![]),
        |(mut sa, mut param_list), (k, v)| {
            let nk = k.to_lowercase();
            match (IString::try_from(k.as_ref()), IString::try_from(v.as_ref())) {
                (Ok(ik), Ok(iv)) => param_list.push((ik, iv)),
                _ => return (sa, param_list),
            };

            match nk.as_str() {
                "charset" => {
                    sa.charset = Some(v);
                }
                "boundary" => {
                    sa.boundary = Some(v);
                }
                _ => (),
            };

            (sa, param_list)
        },
    )
}

/// Takes mail-parser headers and build imap-codec BasicFields
/// Return some special informations too
fn headers_to_basic_fields<'a, T>(
    bp: &'a Part<T>,
    size: usize,
) -> Result<(SpecialAttrs<'a>, BasicFields)> {
    let (attrs, parameter_list) = attrs_to_params(bp);

    let bf = BasicFields {
        parameter_list,

        id: NString(
            bp.get_content_id()
                .map(|ci| IString::try_from(ci.to_string()).ok())
                .flatten(),
        ),

        description: NString(
            bp.get_content_description()
                .map(|cd| IString::try_from(cd.to_string()).ok())
                .flatten(),
        ),

        /*
         * RFC2045 - section 6.1
         * "Content-Transfer-Encoding: 7BIT" is assumed if the
         * Content-Transfer-Encoding header field is not present.
         */
        content_transfer_encoding: bp
            .get_content_transfer_encoding()
            .map(|h| IString::try_from(h.to_string()).ok())
            .flatten()
            .unwrap_or(unchecked_istring("7bit")),

        size: u32::try_from(size)?,
    };

    Ok((attrs, bf))
}

fn get_message_section<'a>(
    parsed: &'a Message<'a>,
    section: &Option<FetchSection>,
) -> Result<Cow<'a, [u8]>> {
    match section {
        Some(FetchSection::Text(None)) => Ok(parsed
            .raw_message
            .get(parsed.offset_body..parsed.offset_end)
            .ok_or(Error::msg(
                "Unable to extract email body, cursors out of bound. This is a bug.",
            ))?
            .into()),
        Some(FetchSection::Text(Some(part))) => {
            map_subpart_msg(parsed, part.0.as_slice(), |part_msg| {
                Ok(part_msg
                    .raw_message
                    .get(part_msg.offset_body..parsed.offset_end)
                    .ok_or(Error::msg(
                        "Unable to extract email body, cursors out of bound. This is a bug.",
                    ))?
                    .to_vec()
                    .into())
            })
        }
        Some(FetchSection::Header(part)) => map_subpart_msg(
            parsed,
            part.as_ref().map(|p| p.0.as_slice()).unwrap_or(&[]),
            |part_msg| {
                Ok(part_msg
                    .raw_message
                    .get(..part_msg.offset_body)
                    .ok_or(Error::msg(
                        "Unable to extract email header, cursors out of bound. This is a bug.",
                    ))?
                    .to_vec()
                    .into())
            },
        ),
        Some(
            FetchSection::HeaderFields(part, fields) | FetchSection::HeaderFieldsNot(part, fields),
        ) => {
            let invert = matches!(section, Some(FetchSection::HeaderFieldsNot(_, _)));
            let fields = fields
                .iter()
                .map(|x| match x {
                    AString::Atom(a) => a.as_bytes(),
                    AString::String(IString::Literal(l)) => l.as_slice(),
                    AString::String(IString::Quoted(q)) => q.as_bytes(),
                })
                .collect::<Vec<_>>();

            map_subpart_msg(
                parsed,
                part.as_ref().map(|p| p.0.as_slice()).unwrap_or(&[]),
                |part_msg| {
                    let mut ret = vec![];
                    for (hn, hv) in part_msg.get_raw_headers() {
                        if fields
                            .as_slice()
                            .iter()
                            .any(|x| (*x == hn.as_str().as_bytes()) ^ invert)
                        {
                            ret.extend(hn.as_str().as_bytes());
                            ret.extend(b": ");
                            ret.extend(hv.as_bytes());
                        }
                    }
                    ret.extend(b"\r\n");
                    Ok(ret.into())
                },
            )
        }
        Some(FetchSection::Part(part)) => map_subpart(parsed, part.0.as_slice(), |_msg, part| {
            let bytes = match part {
                MessagePart::Text(p) | MessagePart::Html(p) => p.body.as_bytes().to_vec(),
                MessagePart::Binary(p) | MessagePart::InlineBinary(p) => p.body.to_vec(),
                MessagePart::Message(Part {
                    body: MessageAttachment::Raw(r),
                    ..
                }) => r.to_vec(),
                MessagePart::Message(Part {
                    body: MessageAttachment::Parsed(p),
                    ..
                }) => p.raw_message.to_vec(),
                MessagePart::Multipart(_) => bail!("Multipart part has no body"),
            };
            Ok(bytes.into())
        }),
        Some(FetchSection::Mime(part)) => map_subpart(parsed, part.0.as_slice(), |msg, part| {
            let raw_headers = match part {
                MessagePart::Text(p) | MessagePart::Html(p) => &p.headers_raw,
                MessagePart::Binary(p) | MessagePart::InlineBinary(p) => &p.headers_raw,
                MessagePart::Message(p) => &p.headers_raw,
                MessagePart::Multipart(m) => &m.headers_raw,
            };
            let mut ret = vec![];
            for (name, body) in raw_headers {
                ret.extend(name.as_str().as_bytes());
                ret.extend(b": ");
                ret.extend(&msg.raw_message[body.start..body.end]);
            }
            ret.extend(b"\r\n");
            Ok(ret.into())
        }),
        None => Ok(parsed.raw_message.clone()),
    }
}

fn map_subpart_msg<'a, F, R>(msg: &Message<'a>, path: &[NonZeroU32], f: F) -> Result<R>
where
    F: FnOnce(&Message<'_>) -> Result<R>,
{
    if path.is_empty() {
        f(msg)
    } else {
        let part = msg
            .parts
            .get(path[0].get() as usize - 1)
            .ok_or(anyhow!("No such subpart: {}", path[0]))?;
        if matches!(part, MessagePart::Message(_)) {
            let part_msg = part
                .parse_message()
                .ok_or(anyhow!("Cannot parse subpart: {}", path[0]))?;
            map_subpart_msg(&part_msg, &path[1..], f)
        } else {
            bail!("Subpart is not a message: {}", path[0]);
        }
    }
}

fn map_subpart<'a, F, R>(msg: &Message<'a>, path: &[NonZeroU32], f: F) -> Result<R>
where
    F: FnOnce(&Message<'_>, &MessagePart<'_>) -> Result<R>,
{
    if path.is_empty() {
        bail!("Unexpected empty path");
    } else {
        let part = msg
            .parts
            .get(path[0].get() as usize - 1)
            .ok_or(anyhow!("No such subpart: {}", path[0]))?;
        if path.len() == 1 {
            f(msg, part)
        } else {
            if matches!(part, MessagePart::Message(_)) {
                let part_msg = part
                    .parse_message()
                    .ok_or(anyhow!("Cannot parse subpart: {}", path[0]))?;
                map_subpart(&part_msg, &path[1..], f)
            } else {
                bail!("Subpart is not a message: {}", path[0]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use imap_codec::codec::Encode;
    use std::fs;

    /// Future automated test. We use lossy utf8 conversion + lowercase everything,
    /// so this test might allow invalid results. But at least it allows us to quickly test a
    /// large variety of emails.
    /// Keep in mind that special cases must still be tested manually!
    #[test]
    fn fetch_body() -> Result<()> {
        let prefixes = [
            "tests/emails/dxflrs/0001_simple",
            "tests/emails/dxflrs/0002_mime",
            "tests/emails/dxflrs/0003_mime-in-mime",
            "tests/emails/dxflrs/0004_msg-in-msg",
            "tests/emails/dxflrs/0005_mail-parser-readme",
            //"tests/emails/dxflrs/0006_single-mime",
            //"tests/emails/dxflrs/0007_raw_msg_in_rfc822",

            //"tests/emails/rfc/000", // broken
            //  "tests/emails/rfc/001", // broken
            //  "tests/emails/rfc/002", // broken: dovecot adds \r when it is missing and count is as
            // a character. Difference on how lines are counted too.
            /*"tests/emails/rfc/003", // broken for the same reason
               "tests/emails/thirdparty/000",
               "tests/emails/thirdparty/001",
               "tests/emails/thirdparty/002",
            */
        ];

        for pref in prefixes.iter() {
            println!("{}", pref);
            let txt = fs::read(format!("{}.eml", pref))?;
            let exp = fs::read(format!("{}.dovecot.body", pref))?;
            let message = Message::parse(&txt).unwrap();

            let mut resp = Vec::new();
            MessageAttribute::Body(build_imap_email_struct(&message, &message.structure)?)
                .encode(&mut resp);

            let resp_str = String::from_utf8_lossy(&resp).to_lowercase();

            let exp_no_parenthesis = &exp[1..exp.len() - 1];
            let exp_str = String::from_utf8_lossy(exp_no_parenthesis).to_lowercase();

            println!("aerogramme: {}\ndovecot:    {}", resp_str, exp_str);
            //println!("\n\n {} \n\n", String::from_utf8_lossy(&resp));
            assert_eq!(resp_str, exp_str);
        }

        Ok(())
    }
}
