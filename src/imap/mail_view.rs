use std::num::NonZeroU32;

use anyhow::{anyhow, bail, Result};
use chrono::{naive::NaiveDate, DateTime as ChronoDateTime, Local, Offset, TimeZone, Utc};

use imap_codec::imap_types::core::NString;
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

use crate::mail::query::QueryResult;

use crate::imap::attributes::AttributesProxy;
use crate::imap::flags;
use crate::imap::imf_view::ImfView;
use crate::imap::index::MailIndex;
use crate::imap::mime_view;
use crate::imap::response::Body;

pub struct MailView<'a> {
    pub in_idx: MailIndex<'a>,
    pub query_result: &'a QueryResult<'a>,
    pub content: FetchedMail<'a>,
}

impl<'a> MailView<'a> {
    pub fn new(query_result: &'a QueryResult<'a>, in_idx: MailIndex<'a>) -> Result<MailView<'a>> {
        Ok(Self {
            in_idx,
            query_result,
            content: match query_result {
                QueryResult::FullResult { content, .. } => {
                    let (_, parsed) =
                        eml_codec::parse_message(&content).or(Err(anyhow!("Invalid mail body")))?;
                    FetchedMail::full_from_message(parsed)
                }
                QueryResult::PartialResult { metadata, .. } => {
                    let (_, parsed) = eml_codec::parse_message(&metadata.headers)
                        .or(Err(anyhow!("unable to parse email headers")))?;
                    FetchedMail::partial_from_message(parsed)
                }
                QueryResult::IndexResult { .. } => FetchedMail::IndexOnly,
            },
        })
    }

    pub fn imf(&self) -> Option<ImfView> {
        self.content.as_imf().map(ImfView)
    }

    pub fn selected_mime(&'a self) -> Option<mime_view::SelectedMime<'a>> {
        self.content.as_anypart().ok().map(mime_view::SelectedMime)
    }

    pub fn filter(&self, ap: &AttributesProxy) -> Result<(Body<'static>, SeenFlag)> {
        let mut seen = SeenFlag::DoNothing;
        let res_attrs = ap
            .attrs
            .iter()
            .map(|attr| match attr {
                MessageDataItemName::Uid => Ok(self.uid()),
                MessageDataItemName::Flags => Ok(self.flags()),
                MessageDataItemName::Rfc822Size => self.rfc_822_size(),
                MessageDataItemName::Rfc822Header => self.rfc_822_header(),
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
                seq: self.in_idx.i,
                items: res_attrs.try_into()?,
            }),
            seen,
        ))
    }

    pub fn stored_naive_date(&self) -> Result<NaiveDate> {
        let mail_meta = self.query_result.metadata().expect("metadata were fetched");
        let mail_ts: i64 = mail_meta.internaldate.try_into()?;
        let msg_date: ChronoDateTime<Local> = ChronoDateTime::from_timestamp(mail_ts, 0)
            .ok_or(anyhow!("unable to parse timestamp"))?
            .with_timezone(&Local);

        Ok(msg_date.date_naive())
    }

    pub fn is_header_contains_pattern(&self, hdr: &[u8], pattern: &[u8]) -> bool {
        let mime = match self.selected_mime() {
            None => return false,
            Some(x) => x,
        };

        let val = match mime.header_value(hdr) {
            None => return false,
            Some(x) => x,
        };

        val.windows(pattern.len()).any(|win| win == pattern)
    }

    // Private function, mainly for filter!
    fn uid(&self) -> MessageDataItem<'static> {
        MessageDataItem::Uid(self.in_idx.uid.clone())
    }

    fn flags(&self) -> MessageDataItem<'static> {
        MessageDataItem::Flags(
            self.in_idx
                .flags
                .iter()
                .filter_map(|f| flags::from_str(f))
                .collect(),
        )
    }

    fn rfc_822_size(&self) -> Result<MessageDataItem<'static>> {
        let sz = self
            .query_result
            .metadata()
            .ok_or(anyhow!("mail metadata are required"))?
            .rfc822_size;
        Ok(MessageDataItem::Rfc822Size(sz as u32))
    }

    fn rfc_822_header(&self) -> Result<MessageDataItem<'static>> {
        let hdrs: NString = self
            .query_result
            .metadata()
            .ok_or(anyhow!("mail metadata are required"))?
            .headers
            .to_vec()
            .try_into()?;
        Ok(MessageDataItem::Rfc822Header(hdrs))
    }

    fn rfc_822_text(&self) -> Result<MessageDataItem<'static>> {
        let txt: NString = self.content.as_msg()?.raw_body.to_vec().try_into()?;
        Ok(MessageDataItem::Rfc822Text(txt))
    }

    fn rfc822(&self) -> Result<MessageDataItem<'static>> {
        let full: NString = self.content.as_msg()?.raw_part.to_vec().try_into()?;
        Ok(MessageDataItem::Rfc822(full))
    }

    fn envelope(&self) -> MessageDataItem<'static> {
        MessageDataItem::Envelope(
            self.imf()
                .expect("an imf object is derivable from fetchedmail")
                .message_envelope(),
        )
    }

    fn body(&self) -> Result<MessageDataItem<'static>> {
        Ok(MessageDataItem::Body(mime_view::bodystructure(
            self.content.as_msg()?.child.as_ref(),
        )?))
    }

    fn body_structure(&self) -> Result<MessageDataItem<'static>> {
        Ok(MessageDataItem::Body(mime_view::bodystructure(
            self.content.as_msg()?.child.as_ref(),
        )?))
    }

    /// maps to BODY[<section>]<<partial>> and BODY.PEEK[<section>]<<partial>>
    /// peek does not implicitly set the \Seen flag
    /// eg. BODY[HEADER.FIELDS (DATE FROM)]
    /// eg. BODY[]<0.2048>
    fn body_ext(
        &self,
        section: &Option<FetchSection<'static>>,
        partial: &Option<(u32, NonZeroU32)>,
        peek: &bool,
    ) -> Result<(MessageDataItem<'static>, SeenFlag)> {
        // Manage Seen flag
        let mut seen = SeenFlag::DoNothing;
        let seen_flag = Flag::Seen.to_string();
        if !peek && !self.in_idx.flags.iter().any(|x| *x == seen_flag) {
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

        let data: NString = text.to_vec().try_into()?;

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
            .timestamp_opt(
                i64::try_from(
                    self.query_result
                        .metadata()
                        .ok_or(anyhow!("mail metadata were not fetched"))?
                        .internaldate
                        / 1000,
                )?,
                0,
            )
            .earliest()
            .ok_or(anyhow!("Unable to parse internal date"))?;
        Ok(MessageDataItem::InternalDate(DateTime::unvalidated(dt)))
    }
}

pub enum SeenFlag {
    DoNothing,
    MustAdd,
}

// -------------------

pub enum FetchedMail<'a> {
    IndexOnly,
    Partial(AnyPart<'a>),
    Full(AnyPart<'a>),
}
impl<'a> FetchedMail<'a> {
    pub fn full_from_message(msg: Message<'a>) -> Self {
        Self::Full(AnyPart::Msg(msg))
    }

    pub fn partial_from_message(msg: Message<'a>) -> Self {
        Self::Partial(AnyPart::Msg(msg))
    }

    pub fn as_anypart(&self) -> Result<&AnyPart<'a>> {
        match self {
            FetchedMail::Full(x) => Ok(&x),
            FetchedMail::Partial(x) => Ok(&x),
            _ => bail!("The full message must be fetched, not only its headers"),
        }
    }

    pub fn as_msg(&self) -> Result<&Message<'a>> {
        match self {
            FetchedMail::Full(AnyPart::Msg(x)) => Ok(&x),
            FetchedMail::Partial(AnyPart::Msg(x)) => Ok(&x),
            _ => bail!("The full message must be fetched, not only its headers AND it must be an AnyPart::Msg."),
        }
    }

    pub fn as_imf(&self) -> Option<&imf::Imf<'a>> {
        match self {
            FetchedMail::Full(AnyPart::Msg(x)) => Some(&x.imf),
            FetchedMail::Partial(AnyPart::Msg(x)) => Some(&x.imf),
            _ => None,
        }
    }
}
