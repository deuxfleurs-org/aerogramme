use std::borrow::Cow;
use std::iter::zip;
use std::num::NonZeroU32;
use std::sync::Arc;

use anyhow::{anyhow, bail, Error, Result};
use boitalettres::proto::res::body::Data as Body;
use chrono::{Offset, TimeZone, Utc};

use futures::stream::{FuturesOrdered, StreamExt};

use imap_codec::imap_types::address::Address;
use imap_codec::imap_types::body::{BasicFields, Body as FetchBody, BodyStructure, SpecificFields};
use imap_codec::imap_types::core::{AString, Atom, IString, NString};
use imap_codec::imap_types::datetime::MyDateTime;
use imap_codec::imap_types::envelope::Envelope;
use imap_codec::imap_types::fetch_attributes::{
    FetchAttribute, MacroOrFetchAttributes, Section as FetchSection,
};
use imap_codec::imap_types::flag::{Flag, StoreResponse, StoreType};
use imap_codec::imap_types::response::{Code, Data, MessageAttribute, Status};
use imap_codec::imap_types::sequence::{self, SequenceSet};

use eml_codec::{
    header, imf, mime,
    mime::r#type::Deductible,
    part::{composite::Message, AnyPart},
};

use crate::cryptoblob::Key;
use crate::mail::mailbox::{MailMeta, Mailbox};
use crate::mail::uidindex::{ImapUid, ImapUidvalidity, UidIndex};
use crate::mail::unique_ident::UniqueIdent;

const DEFAULT_FLAGS: [Flag; 5] = [
    Flag::Seen,
    Flag::Answered,
    Flag::Flagged,
    Flag::Deleted,
    Flag::Draft,
];

enum FetchedMail<'a> {
    Partial(imf::Imf<'a>),
    Full(AnyPart<'a>),
}
impl<'a> FetchedMail<'a> {
    fn new_from_message(msg: Message<'a>) -> Self {
        FetchedMail::Full(AnyPart::Msg(msg))
    }

    /*fn new_from_header(hdr: imf::Imf<'a>) -> Self {
        FetchedMail::Partial(hdr)
    }*/

    fn as_anypart(&self) -> Result<&AnyPart<'a>> {
        match self {
            FetchedMail::Full(x) => Ok(&x),
            _ => bail!("The full message must be fetched, not only its headers"),
        }
    }

    fn as_full(&self) -> Result<&Message<'a>> {
        match self {
            FetchedMail::Full(AnyPart::Msg(x)) => Ok(&x),
            _ => bail!("The full message must be fetched, not only its headers AND it must be an AnyPart::Msg."),
        }
    }

    fn imf(&self) -> &imf::Imf<'a> {
        match self {
            FetchedMail::Full(AnyPart::Msg(x)) => &x.imf,
            FetchedMail::Partial(x) => &x,
            _ => panic!("Can't contain AnyPart that is not a message"),
        }
    }
}

pub struct AttributesProxy {
    attrs: Vec<FetchAttribute>,
}
impl AttributesProxy {
    fn new(attrs: &MacroOrFetchAttributes, is_uid_fetch: bool) -> Self {
        // Expand macros
        let mut fetch_attrs = match attrs {
            MacroOrFetchAttributes::Macro(m) => m.expand(),
            MacroOrFetchAttributes::FetchAttributes(a) => a.clone(),
        };

        // Handle uids
        if is_uid_fetch && !fetch_attrs.contains(&FetchAttribute::Uid) {
            fetch_attrs.push(FetchAttribute::Uid);
        }

        Self { attrs: fetch_attrs }
    }

    fn need_body(&self) -> bool {
        self.attrs.iter().any(|x| {
            matches!(
                x,
                FetchAttribute::Body
                    | FetchAttribute::BodyExt { .. }
                    | FetchAttribute::Rfc822
                    | FetchAttribute::Rfc822Text
                    | FetchAttribute::BodyStructure
            )
        })
    }
}

pub struct MailIdentifiers {
    i: NonZeroU32,
    uid: ImapUid,
    uuid: UniqueIdent,
}
struct MailIdentifiersList(Vec<MailIdentifiers>);

impl MailIdentifiersList {
    fn uuids(&self) -> Vec<UniqueIdent> {
        self.0.iter().map(|mi| mi.uuid).collect()
    }
}

pub struct MailView<'a> {
    ids: &'a MailIdentifiers,
    meta: &'a MailMeta,
    flags: &'a Vec<String>,
    content: FetchedMail<'a>,
    add_seen: bool,
}

impl<'a> MailView<'a> {
    fn uid(&self) -> MessageAttribute {
        MessageAttribute::Uid(self.ids.uid)
    }

    fn flags(&self) -> MessageAttribute {
        MessageAttribute::Flags(
            self.flags
                .iter()
                .filter_map(|f| string_to_flag(f))
                .collect(),
        )
    }

    fn rfc_822_size(&self) -> MessageAttribute {
        MessageAttribute::Rfc822Size(self.meta.rfc822_size as u32)
    }

    fn rfc_822_header(&self) -> MessageAttribute {
        MessageAttribute::Rfc822Header(NString(
            self.meta
                .headers
                .to_vec()
                .try_into()
                .ok()
                .map(IString::Literal),
        ))
    }

    fn rfc_822_text(&self) -> Result<MessageAttribute> {
        Ok(MessageAttribute::Rfc822Text(NString(
            self.content
                .as_full()?
                .raw_body
                .try_into()
                .ok()
                .map(IString::Literal),
        )))
    }

    fn rfc822(&self) -> Result<MessageAttribute> {
        Ok(MessageAttribute::Rfc822(NString(
            self.content
                .as_full()?
                .raw_part
                .clone()
                .try_into()
                .ok()
                .map(IString::Literal),
        )))
    }

    fn envelope(&self) -> MessageAttribute {
        MessageAttribute::Envelope(message_envelope(self.content.imf()))
    }

    fn body(&self) -> Result<MessageAttribute> {
        Ok(MessageAttribute::Body(build_imap_email_struct(
            self.content.as_full()?.child.as_ref(),
        )?))
    }

    fn body_structure(&self) -> Result<MessageAttribute> {
        Ok(MessageAttribute::Body(build_imap_email_struct(
            self.content.as_full()?.child.as_ref(),
        )?))
    }

    /// maps to BODY[<section>]<<partial>> and BODY.PEEK[<section>]<<partial>>
    /// peek does not implicitly set the \Seen flag
    /// eg. BODY[HEADER.FIELDS (DATE FROM)]
    /// eg. BODY[]<0.2048>
    fn body_ext(
        &mut self,
        section: &Option<FetchSection>,
        partial: &Option<(u32, NonZeroU32)>,
        peek: &bool,
    ) -> Result<MessageAttribute> {
        // Extract message section
        let text = get_message_section(self.content.as_anypart()?, section)?;

        let seen_flag = Flag::Seen.to_string();
        if !peek && !self.flags.iter().any(|x| *x == seen_flag) {
            // Add \Seen flag
            //self.mailbox.add_flags(uuid, &[seen_flag]).await?;
            self.add_seen = true;
        }

        // Handle <<partial>> which cut the message bytes
        let (text, origin) = apply_partial(partial, &text);

        let data = NString(text.to_vec().try_into().ok().map(IString::Literal));

        return Ok(MessageAttribute::BodyExt {
            section: section.clone(),
            origin,
            data,
        });
    }

    fn internal_date(&self) -> Result<MessageAttribute> {
        let dt = Utc
            .fix()
            .timestamp_opt(i64::try_from(self.meta.internaldate / 1000)?, 0)
            .earliest()
            .ok_or(anyhow!("Unable to parse internal date"))?;
        Ok(MessageAttribute::InternalDate(MyDateTime(dt)))
    }

    fn filter(&mut self, ap: &AttributesProxy) -> Result<Body> {
        let res_attrs = ap
            .attrs
            .iter()
            .map(|attr| match attr {
                FetchAttribute::Uid => Ok(self.uid()),
                FetchAttribute::Flags => Ok(self.flags()),
                FetchAttribute::Rfc822Size => Ok(self.rfc_822_size()),
                FetchAttribute::Rfc822Header => Ok(self.rfc_822_header()),
                FetchAttribute::Rfc822Text => self.rfc_822_text(),
                FetchAttribute::Rfc822 => self.rfc822(),
                FetchAttribute::Envelope => Ok(self.envelope()),
                FetchAttribute::Body => self.body(),
                FetchAttribute::BodyStructure => self.body_structure(),
                FetchAttribute::BodyExt {
                    section,
                    partial,
                    peek,
                } => self.body_ext(section, partial, peek),
                FetchAttribute::InternalDate => self.internal_date(),
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Body::Data(Data::Fetch {
            seq_or_uid: self.ids.i,
            attributes: res_attrs,
        }))
    }
}

fn apply_partial<'a>(
    partial: &'_ Option<(u32, NonZeroU32)>,
    text: &'a [u8],
) -> (&'a [u8], Option<u32>) {
    match partial {
        Some((begin, len)) => {
            if *begin as usize > text.len() {
                (&[][..], Some(*begin))
            } else if (begin + len.get()) as usize >= text.len() {
                (&text[*begin as usize..], Some(*begin))
            } else {
                (
                    &text[*begin as usize..(begin + len.get()) as usize],
                    Some(*begin),
                )
            }
        }
        None => (&text[..], None),
    }
}

pub struct BodyIdentifier<'a> {
    msg_uuid: &'a UniqueIdent,
    msg_key: &'a Key,
}

#[derive(Default)]
pub struct MailSelectionBuilder<'a> {
    //attrs: AttributeProxy,
    mail_count: usize,
    need_body: bool,
    mi: &'a [MailIdentifiers],
    meta: &'a [MailMeta],
    flags: &'a [&'a Vec<String>],
    bodies: &'a [Vec<u8>],
}

impl<'a> MailSelectionBuilder<'a> {
    fn new(need_body: bool, mail_count: usize) -> Self {
        Self {
            mail_count,
            need_body,
            ..MailSelectionBuilder::default()
        }
    }

    fn with_mail_identifiers(&mut self, mi: &'a [MailIdentifiers]) -> &mut Self {
        self.mi = mi;
        self
    }

    fn with_metadata(&mut self, meta: &'a [MailMeta]) -> &mut Self {
        self.meta = meta;
        self
    }

    fn with_flags(&mut self, flags: &'a [&'a Vec<String>]) -> &mut Self {
        self.flags = flags;
        self
    }

    fn bodies_to_collect(&self) -> Vec<BodyIdentifier> {
        if !self.need_body {
            return vec![];
        }
        zip(self.mi, self.meta)
            .map(|(mi, meta)| BodyIdentifier {
                msg_uuid: &mi.uuid,
                msg_key: &meta.message_key,
            })
            .collect::<Vec<_>>()
    }

    fn with_bodies(&mut self, rbodies: &'a [Vec<u8>]) -> &mut Self {
        self.bodies = rbodies;
        self
    }

    fn build(&self) -> Result<Vec<MailView<'a>>> {
        let mut bodies = vec![];

        if !self.need_body {
            for m in self.meta.iter() {
                let (_, hdrs) =
                    eml_codec::parse_imf(&m.headers).or(Err(anyhow!("Invalid mail headers")))?;
                bodies.push(FetchedMail::Partial(hdrs));
            }
        } else {
            for rb in self.bodies.iter() {
                let (_, p) = eml_codec::parse_message(&rb).or(Err(anyhow!("Invalid mail body")))?;
                bodies.push(FetchedMail::new_from_message(p));
            }
        }

        if self.mi.len() != self.mail_count && self.meta.len() != self.mail_count
            || self.flags.len() != self.mail_count
            || bodies.len() != self.mail_count
        {
            return Err(anyhow!("Can't build a mail view selection as parts were not correctly registered into the builder."));
        }

        Ok(zip(self.mi, zip(self.meta, zip(self.flags, bodies)))
            .map(|(ids, (meta, (flags, content)))| MailView {
                ids,
                meta,
                flags,
                content,
                add_seen: false,
            })
            .collect())
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
                                MessageAttribute::Uid(*uid),
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

    /// Looks up state changes in the mailbox and produces a set of IMAP
    /// responses describing the new state.
    pub async fn fetch(
        &self,
        sequence_set: &SequenceSet,
        attributes: &MacroOrFetchAttributes,
        is_uid_fetch: &bool,
    ) -> Result<Vec<Body>> {
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
        let mut views = selection.build()?;

        // Filter views to build the result
        let ret = views
            .iter_mut()
            .filter_map(|mv| mv.filter(&ap).ok())
            .collect::<Vec<_>>();

        // Register seen flags
        let future_flags = views
            .iter()
            .filter(|mv| mv.add_seen)
            .map(|mv| async move {
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

        Ok(ret)
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
            .filter_map(|f| string_to_flag(f))
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
            _ => match Atom::try_from(f.strip_prefix('\\').unwrap().to_string()) {
                Err(_) => {
                    tracing::error!(flag=%f, "Unable to encode flag as IMAP atom");
                    None
                }
                Ok(a) => Some(Flag::Extension(a)),
            },
        },
        Some(_) => match Atom::try_from(f.to_string()) {
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
fn message_envelope(msg: &imf::Imf) -> Envelope {
    let from = msg.from.iter().map(convert_mbx).collect::<Vec<_>>();

    Envelope {
        date: NString(
            msg.date
                .as_ref()
                .map(|d| IString::try_from(d.to_rfc3339()).unwrap()),
        ),
        subject: NString(
            msg.subject
                .as_ref()
                .map(|d| IString::try_from(d.to_string()).unwrap()),
        ),
        sender: msg
            .sender
            .as_ref()
            .map(|v| vec![convert_mbx(v)])
            .unwrap_or(from.clone()),
        reply_to: if msg.reply_to.is_empty() {
            from.clone()
        } else {
            convert_addresses(&msg.reply_to)
        },
        from,
        to: convert_addresses(&msg.to),
        cc: convert_addresses(&msg.cc),
        bcc: convert_addresses(&msg.bcc),
        in_reply_to: NString(
            msg.in_reply_to
                .iter()
                .next()
                .map(|d| IString::try_from(d.to_string()).unwrap()),
        ),
        message_id: NString(
            msg.msg_id
                .as_ref()
                .map(|d| IString::try_from(d.to_string()).unwrap()),
        ),
    }
}

fn convert_addresses(addrlist: &Vec<imf::address::AddressRef>) -> Vec<Address> {
    let mut acc = vec![];
    for item in addrlist {
        match item {
            imf::address::AddressRef::Single(a) => acc.push(convert_mbx(a)),
            imf::address::AddressRef::Many(l) => acc.extend(l.participants.iter().map(convert_mbx)),
        }
    }
    return acc;
}

fn convert_mbx(addr: &imf::mailbox::MailboxRef) -> Address {
    Address::new(
        NString(
            addr.name
                .as_ref()
                .map(|x| IString::try_from(x.to_string()).unwrap()),
        ),
        // SMTP at-domain-list (source route) seems obsolete since at least 1991
        // https://www.mhonarc.org/archive/html/ietf-822/1991-06/msg00060.html
        NString(None),
        NString(Some(
            IString::try_from(addr.addrspec.local_part.to_string()).unwrap(),
        )),
        NString(Some(
            IString::try_from(addr.addrspec.domain.to_string()).unwrap(),
        )),
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

fn build_imap_email_struct<'a>(part: &AnyPart<'a>) -> Result<BodyStructure> {
    match part {
        AnyPart::Mult(x) => {
            let itype = &x.mime.interpreted_type;
            let subtype = IString::try_from(itype.subtype.to_string())
                .unwrap_or(unchecked_istring("alternative"));

            Ok(BodyStructure::Multi {
                bodies: x
                    .children
                    .iter()
                    .filter_map(|inner| build_imap_email_struct(&inner).ok())
                    .collect(),
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
        AnyPart::Txt(x) => {
            let mut basic = basic_fields(&x.mime.fields, x.body.len())?;

            // Get the interpreted content type, set it
            let itype = match &x.mime.interpreted_type {
                Deductible::Inferred(v) | Deductible::Explicit(v) => v,
            };
            let subtype =
                IString::try_from(itype.subtype.to_string()).unwrap_or(unchecked_istring("plain"));

            // Add charset to the list of parameters if we know it has been inferred as it will be
            // missing from the parsed content.
            if let Deductible::Inferred(charset) = &itype.charset {
                basic.parameter_list.push((
                    unchecked_istring("charset"),
                    IString::try_from(charset.to_string()).unwrap_or(unchecked_istring("us-ascii")),
                ));
            }

            Ok(BodyStructure::Single {
                body: FetchBody {
                    basic,
                    specific: SpecificFields::Text {
                        subtype,
                        number_of_lines: nol(x.body),
                    },
                },
                extension: None,
            })
        }
        AnyPart::Bin(x) => {
            let basic = basic_fields(&x.mime.fields, x.body.len())?;

            let default = mime::r#type::NaiveType {
                main: &b"application"[..],
                sub: &b"octet-stream"[..],
                params: vec![],
            };
            let ct = x.mime.fields.ctype.as_ref().unwrap_or(&default);

            let type_ = IString::try_from(String::from_utf8_lossy(ct.main).to_string()).or(Err(
                anyhow!("Unable to build IString from given Content-Type type given"),
            ))?;

            let subtype =
                IString::try_from(String::from_utf8_lossy(ct.sub).to_string()).or(Err(anyhow!(
                    "Unable to build IString from given Content-Type subtype given"
                )))?;

            Ok(BodyStructure::Single {
                body: FetchBody {
                    basic,
                    specific: SpecificFields::Basic { type_, subtype },
                },
                extension: None,
            })
        }
        AnyPart::Msg(x) => {
            let basic = basic_fields(&x.mime.fields, x.raw_part.len())?;

            Ok(BodyStructure::Single {
                body: FetchBody {
                    basic,
                    specific: SpecificFields::Message {
                        envelope: message_envelope(&x.imf),
                        body_structure: Box::new(build_imap_email_struct(x.child.as_ref())?),
                        number_of_lines: nol(x.raw_part),
                    },
                },
                extension: None,
            })
        }
    }
}

fn nol(input: &[u8]) -> u32 {
    input
        .iter()
        .filter(|x| **x == b'\n')
        .count()
        .try_into()
        .unwrap_or(0)
}

/// s is set to static to ensure that only compile time values
/// checked by developpers are passed.
fn unchecked_istring(s: &'static str) -> IString {
    IString::try_from(s).expect("this value is expected to be a valid imap-codec::IString")
}

fn basic_fields(m: &mime::NaiveMIME, sz: usize) -> Result<BasicFields> {
    let parameter_list = m
        .ctype
        .as_ref()
        .map(|x| {
            x.params
                .iter()
                .map(|p| {
                    (
                        IString::try_from(String::from_utf8_lossy(p.name).to_string()),
                        IString::try_from(p.value.to_string()),
                    )
                })
                .filter(|(k, v)| k.is_ok() && v.is_ok())
                .map(|(k, v)| (k.unwrap(), v.unwrap()))
                .collect()
        })
        .unwrap_or(vec![]);

    Ok(BasicFields {
        parameter_list,
        id: NString(
            m.id.as_ref()
                .and_then(|ci| IString::try_from(ci.to_string()).ok()),
        ),
        description: NString(
            m.description
                .as_ref()
                .and_then(|cd| IString::try_from(cd.to_string()).ok()),
        ),
        content_transfer_encoding: match m.transfer_encoding {
            mime::mechanism::Mechanism::_8Bit => unchecked_istring("8bit"),
            mime::mechanism::Mechanism::Binary => unchecked_istring("binary"),
            mime::mechanism::Mechanism::QuotedPrintable => unchecked_istring("quoted-printable"),
            mime::mechanism::Mechanism::Base64 => unchecked_istring("base64"),
            _ => unchecked_istring("7bit"),
        },
        // @FIXME we can't compute the size of the message currently...
        size: u32::try_from(sz)?,
    })
}

/// Extract message section for section identifier passed by the FETCH BODY[<section>]<<partial>>
/// request
///
/// Example of message sections:
///
/// ```
///    HEADER     ([RFC-2822] header of the message)
///    TEXT       ([RFC-2822] text body of the message) MULTIPART/MIXED
///    1          TEXT/PLAIN
///    2          APPLICATION/OCTET-STREAM
///    3          MESSAGE/RFC822
///    3.HEADER   ([RFC-2822] header of the message)
///    3.TEXT     ([RFC-2822] text body of the message) MULTIPART/MIXED
///    3.1        TEXT/PLAIN
///    3.2        APPLICATION/OCTET-STREAM
///    4          MULTIPART/MIXED
///    4.1        IMAGE/GIF
///    4.1.MIME   ([MIME-IMB] header for the IMAGE/GIF)
///    4.2        MESSAGE/RFC822
///    4.2.HEADER ([RFC-2822] header of the message)
///    4.2.TEXT   ([RFC-2822] text body of the message) MULTIPART/MIXED
///    4.2.1      TEXT/PLAIN
///    4.2.2      MULTIPART/ALTERNATIVE
///    4.2.2.1    TEXT/PLAIN
///    4.2.2.2    TEXT/RICHTEXT
/// ```
fn get_message_section<'a>(
    parsed: &'a AnyPart<'a>,
    section: &Option<FetchSection>,
) -> Result<Cow<'a, [u8]>> {
    let msg = parsed
        .as_message()
        .ok_or(anyhow!("Part must be a message"))?;
    match section {
        Some(FetchSection::Text(None)) => Ok(msg.raw_body.into()),
        Some(FetchSection::Text(Some(part))) => {
            map_subpart(parsed, part.0.as_slice(), |part_msg| {
                Ok(part_msg
                    .as_message()
                    .ok_or(Error::msg(
                        "Not a message/rfc822 part while expected by request (TEXT)",
                    ))?
                    .raw_body
                    .into())
            })
        }
        Some(FetchSection::Header(part)) => map_subpart(
            parsed,
            part.as_ref().map(|p| p.0.as_slice()).unwrap_or(&[]),
            |part_msg| {
                Ok(part_msg
                    .as_message()
                    .ok_or(Error::msg(
                        "Not a message/rfc822 part while expected by request (HEADER)",
                    ))?
                    .raw_headers
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

            map_subpart(
                parsed,
                part.as_ref().map(|p| p.0.as_slice()).unwrap_or(&[]),
                |part_msg| {
                    let mut ret = vec![];
                    for f in &part_msg.mime().kv {
                        let (k, v) = match f {
                            header::Field::Good(header::Kv2(k, v)) => (k, v),
                            _ => continue,
                        };
                        if fields.as_slice().iter().any(|x| (x == k) ^ invert) {
                            ret.extend(*k);
                            ret.extend(b": ");
                            ret.extend(*v);
                            ret.extend(b"\r\n");
                        }
                    }
                    ret.extend(b"\r\n");
                    Ok(ret.into())
                },
            )
        }
        Some(FetchSection::Part(part)) => map_subpart(parsed, part.0.as_slice(), |part| {
            let bytes = match &part {
                AnyPart::Txt(p) => p.body,
                AnyPart::Bin(p) => p.body,
                AnyPart::Msg(p) => p.raw_part,
                AnyPart::Mult(_) => bail!("Multipart part has no body"),
            };
            Ok(bytes.to_vec().into())
        }),
        Some(FetchSection::Mime(part)) => map_subpart(parsed, part.0.as_slice(), |part| {
            let bytes = match &part {
                AnyPart::Txt(p) => p.mime.fields.raw,
                AnyPart::Bin(p) => p.mime.fields.raw,
                AnyPart::Msg(p) => p.mime.fields.raw,
                AnyPart::Mult(p) => p.mime.fields.raw,
            };
            Ok(bytes.to_vec().into())
        }),
        None => Ok(msg.raw_part.into()),
    }
}

/// Fetch a MIME SubPart
///
/// eg. FETCH BODY[4.2.2.1] -> [4, 2, 2, 1]
fn map_subpart<'a, F, R>(part: &AnyPart<'a>, path: &[NonZeroU32], f: F) -> Result<R>
where
    F: FnOnce(&AnyPart<'a>) -> Result<R>,
{
    if path.is_empty() {
        f(part)
    } else {
        match part {
            AnyPart::Mult(x) => map_subpart(
                x.children
                    .get(path[0].get() as usize - 1)
                    .ok_or(anyhow!("Unable to resolve subpath {:?}, current multipart has only {} elements", path, x.children.len()))?,
                &path[1..],
                f),
            AnyPart::Msg(x) => map_subpart(x.child.as_ref(), path, f),
            _ => bail!("You tried to access a subpart on an atomic part (text or binary). Unresolved subpath {:?}", path),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cryptoblob;
    use crate::mail::unique_ident;
    use imap_codec::codec::Encode;
    use imap_codec::imap_types::fetch_attributes::Section;
    use std::fs;

    #[test]
    fn mailview_body_ext() -> Result<()> {
        let ap = AttributesProxy::new(
            &MacroOrFetchAttributes::FetchAttributes(vec![FetchAttribute::BodyExt {
                section: Some(Section::Header(None)),
                partial: None,
                peek: false,
            }]),
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

        let mut mv = MailView {
            ids: &ids,
            content,
            meta: &meta,
            flags: &flags,
            add_seen: false,
        };
        let res_body = mv.filter(&ap)?;

        let fattr = match res_body {
            Body::Data(Data::Fetch {
                seq_or_uid: _seq,
                attributes: attr,
            }) => Ok(attr),
            _ => Err(anyhow!("Not a fetch body")),
        }?;

        assert_eq!(fattr.len(), 1);

        let (sec, _orig, _data) = match &fattr[0] {
            MessageAttribute::BodyExt {
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
            let exp = fs::read(format!("{}.dovecot.body", pref))?;
            let message = eml_codec::parse_message(&txt).unwrap().1;

            let mut resp = Vec::new();
            MessageAttribute::Body(build_imap_email_struct(&message.child)?)
                .encode(&mut resp)
                .unwrap();

            let resp_str = String::from_utf8_lossy(&resp).to_lowercase();

            let exp_no_parenthesis = &exp[1..exp.len() - 1];
            let exp_str = String::from_utf8_lossy(exp_no_parenthesis).to_lowercase();

            println!("aerogramme: {}\n\ndovecot:    {}\n\n", resp_str, exp_str);
            //println!("\n\n {} \n\n", String::from_utf8_lossy(&resp));
            assert_eq!(resp_str, exp_str);
        }

        Ok(())
    }
}
