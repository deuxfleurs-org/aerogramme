use std::borrow::Cow;
use std::iter::zip;
use std::num::NonZeroU32;
use std::sync::Arc;

use anyhow::{anyhow, bail, Error, Result};
use chrono::{Offset, TimeZone, Utc};

use futures::stream::{FuturesOrdered, StreamExt};

use imap_codec::imap_types::body::{BasicFields, Body as FetchBody, BodyStructure, SpecificFields};
use imap_codec::imap_types::core::{AString, Atom, IString, NString, NonEmptyVec};
use imap_codec::imap_types::datetime::DateTime;
use imap_codec::imap_types::envelope::{Address, Envelope};
use imap_codec::imap_types::fetch::{
    MacroOrMessageDataItemNames, MessageDataItem, MessageDataItemName, Section as FetchSection,
};
use imap_codec::imap_types::flag::{Flag, FlagFetch, FlagPerm, StoreResponse, StoreType};
use imap_codec::imap_types::response::{Code, Data, Status};
use imap_codec::imap_types::sequence::{self, SequenceSet};

use eml_codec::{
    header, imf, mime,
    mime::r#type::Deductible,
    part::{composite::Message, AnyPart},
};

use crate::cryptoblob::Key;
use crate::imap::response::Body;
use crate::mail::mailbox::{MailMeta, Mailbox};
use crate::mail::uidindex::{ImapUid, ImapUidvalidity, UidIndex};
use crate::mail::unique_ident::UniqueIdent;




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
pub fn message_envelope(msg: &imf::Imf) -> Envelope<'static> {
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

pub fn convert_addresses(addrlist: &Vec<imf::address::AddressRef>) -> Vec<Address<'static>> {
    let mut acc = vec![];
    for item in addrlist {
        match item {
            imf::address::AddressRef::Single(a) => acc.push(convert_mbx(a)),
            imf::address::AddressRef::Many(l) => acc.extend(l.participants.iter().map(convert_mbx)),
        }
    }
    return acc;
}

pub fn convert_mbx(addr: &imf::mailbox::MailboxRef) -> Address<'static> {
    Address {
        name: NString(
            addr.name
                .as_ref()
                .map(|x| IString::try_from(x.to_string()).unwrap()),
        ),
        // SMTP at-domain-list (source route) seems obsolete since at least 1991
        // https://www.mhonarc.org/archive/html/ietf-822/1991-06/msg00060.html
        adl: NString(None),
        mailbox: NString(Some(
            IString::try_from(addr.addrspec.local_part.to_string()).unwrap(),
        )),
        host: NString(Some(
            IString::try_from(addr.addrspec.domain.to_string()).unwrap(),
        )),
    }
}

