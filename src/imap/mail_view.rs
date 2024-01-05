use std::num::NonZeroU32;

use anyhow::{anyhow, bail, Result};
use chrono::{Offset, TimeZone, Utc};

use imap_codec::imap_types::core::{IString, NString};
use imap_codec::imap_types::datetime::DateTime;
use imap_codec::imap_types::fetch::{
    MessageDataItem, MessageDataItemName, Section as FetchSection,
};
use imap_codec::imap_types::flag::Flag;
use imap_codec::imap_types::response::Data;

use eml_codec::{
    imf,
    part::{composite::Message, AnyPart},
};

use crate::imap::attributes::AttributesProxy;
use crate::imap::flags;
use crate::imap::imf_view::message_envelope;
use crate::imap::mailbox_view::MailIdentifiers;
use crate::imap::mime_view;
use crate::imap::response::Body;
use crate::mail::mailbox::MailMeta;

pub struct MailView<'a> {
    pub ids: &'a MailIdentifiers,
    pub meta: &'a MailMeta,
    pub flags: &'a Vec<String>,
    pub content: FetchedMail<'a>,
}

impl<'a> MailView<'a> {
    fn uid(&self) -> MessageDataItem<'static> {
        MessageDataItem::Uid(self.ids.uid.clone())
    }

    fn flags(&self) -> MessageDataItem<'static> {
        MessageDataItem::Flags(
            self.flags
                .iter()
                .filter_map(|f| flags::from_str(f))
                .collect(),
        )
    }

    fn rfc_822_size(&self) -> MessageDataItem<'static> {
        MessageDataItem::Rfc822Size(self.meta.rfc822_size as u32)
    }

    fn rfc_822_header(&self) -> MessageDataItem<'static> {
        MessageDataItem::Rfc822Header(NString(
            self.meta
                .headers
                .to_vec()
                .try_into()
                .ok()
                .map(IString::Literal),
        ))
    }

    fn rfc_822_text(&self) -> Result<MessageDataItem<'static>> {
        Ok(MessageDataItem::Rfc822Text(NString(
            self.content
                .as_full()?
                .raw_body
                .to_vec()
                .try_into()
                .ok()
                .map(IString::Literal),
        )))
    }

    fn rfc822(&self) -> Result<MessageDataItem<'static>> {
        Ok(MessageDataItem::Rfc822(NString(
            self.content
                .as_full()?
                .raw_part
                .to_vec()
                .try_into()
                .ok()
                .map(IString::Literal),
        )))
    }

    fn envelope(&self) -> MessageDataItem<'static> {
        MessageDataItem::Envelope(message_envelope(self.content.imf().clone()))
    }

    fn body(&self) -> Result<MessageDataItem<'static>> {
        Ok(MessageDataItem::Body(mime_view::bodystructure(
            self.content.as_full()?.child.as_ref(),
        )?))
    }

    fn body_structure(&self) -> Result<MessageDataItem<'static>> {
        Ok(MessageDataItem::Body(mime_view::bodystructure(
            self.content.as_full()?.child.as_ref(),
        )?))
    }

    /// maps to BODY[<section>]<<partial>> and BODY.PEEK[<section>]<<partial>>
    /// peek does not implicitly set the \Seen flag
    /// eg. BODY[HEADER.FIELDS (DATE FROM)]
    /// eg. BODY[]<0.2048>
    fn body_ext<'b>(
        &self,
        section: &Option<FetchSection<'b>>,
        partial: &Option<(u32, NonZeroU32)>,
        peek: &bool,
    ) -> Result<(MessageDataItem<'b>, SeenFlag)> {
        // Manage Seen flag
        let mut seen = SeenFlag::DoNothing;
        let seen_flag = Flag::Seen.to_string();
        if !peek && !self.flags.iter().any(|x| *x == seen_flag) {
            // Add \Seen flag
            //self.mailbox.add_flags(uuid, &[seen_flag]).await?;
            seen = SeenFlag::MustAdd;
        }

        // Process message
        let (text, origin) =
            match mime_view::body_ext(self.content.as_anypart()?, section, partial)? {
                mime_view::BodySection::Full(body) => (body, None),
                mime_view::BodySection::Slice { body, origin_octet } => (body, Some(origin_octet)),
            };

        let data = NString(text.to_vec().try_into().ok().map(IString::Literal));

        return Ok((
            MessageDataItem::BodyExt {
                section: section.as_ref().map(|fs| fs.clone()),
                origin,
                data,
            },
            seen,
        ));
    }

    fn internal_date(&self) -> Result<MessageDataItem<'static>> {
        let dt = Utc
            .fix()
            .timestamp_opt(i64::try_from(self.meta.internaldate / 1000)?, 0)
            .earliest()
            .ok_or(anyhow!("Unable to parse internal date"))?;
        Ok(MessageDataItem::InternalDate(DateTime::unvalidated(dt)))
    }

    pub fn filter<'b>(&self, ap: &AttributesProxy) -> Result<(Body<'static>, SeenFlag)> {
        let mut seen = SeenFlag::DoNothing;
        let res_attrs = ap
            .attrs
            .iter()
            .map(|attr| match attr {
                MessageDataItemName::Uid => Ok(self.uid()),
                MessageDataItemName::Flags => Ok(self.flags()),
                MessageDataItemName::Rfc822Size => Ok(self.rfc_822_size()),
                MessageDataItemName::Rfc822Header => Ok(self.rfc_822_header()),
                MessageDataItemName::Rfc822Text => self.rfc_822_text(),
                MessageDataItemName::Rfc822 => self.rfc822(),
                MessageDataItemName::Envelope => Ok(self.envelope()),
                MessageDataItemName::Body => self.body(),
                MessageDataItemName::BodyStructure => self.body_structure(),
                MessageDataItemName::BodyExt {
                    section,
                    partial,
                    peek,
                } => {
                    let (body, has_seen) = self.body_ext(section, partial, peek)?;
                    seen = has_seen;
                    Ok(body)
                }
                MessageDataItemName::InternalDate => self.internal_date(),
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok((
            Body::Data(Data::Fetch {
                seq: self.ids.i,
                items: res_attrs.try_into()?,
            }),
            seen,
        ))
    }
}

pub enum SeenFlag {
    DoNothing,
    MustAdd,
}

// -------------------

pub enum FetchedMail<'a> {
    Partial(imf::Imf<'a>),
    Full(AnyPart<'a>),
}
impl<'a> FetchedMail<'a> {
    pub fn new_from_message(msg: Message<'a>) -> Self {
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
