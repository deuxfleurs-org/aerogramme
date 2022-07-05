use std::borrow::{Borrow, Cow};
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::Arc;

use anyhow::{anyhow, bail, Error, Result};
use boitalettres::proto::res::body::Data as Body;
use chrono::{Offset, TimeZone, Utc};
use futures::stream::{FuturesOrdered, StreamExt};
use imap_codec::types::address::Address;
use imap_codec::types::body::{BasicFields, Body as FetchBody, BodyStructure, SpecificFields};
use imap_codec::types::core::{Atom, IString, NString, NonZeroBytes};
use imap_codec::types::datetime::MyDateTime;
use imap_codec::types::envelope::Envelope;
use imap_codec::types::fetch_attributes::{FetchAttribute, MacroOrFetchAttributes};
use imap_codec::types::flag::Flag;
use imap_codec::types::response::{Code, Data, MessageAttribute, Status};
use imap_codec::types::sequence::{self, SequenceSet};
use mail_parser::*;

use crate::mail::mailbox::Mailbox;
use crate::mail::uidindex::UidIndex;

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
    mailbox: Arc<Mailbox>,
    known_state: UidIndex,
}

impl MailboxView {
    /// Creates a new IMAP view into a mailbox.
    /// Generates the necessary IMAP messages so that the client
    /// has a satisfactory summary of the current mailbox's state.
    /// These are the messages that are sent in response to a SELECT command.
    pub async fn new(mailbox: Arc<Mailbox>) -> Result<(Self, Vec<Body>)> {
        // TODO THIS IS JUST A TEST REMOVE LATER
        mailbox.test().await?;

        let state = mailbox.current_uid_index().await;

        let new_view = Self {
            mailbox,
            known_state: state,
        };

        let mut data = Vec::<Body>::new();
        data.push(new_view.exists()?);
        data.push(new_view.recent()?);
        data.extend(new_view.flags()?.into_iter());
        data.push(new_view.uidvalidity()?);
        data.push(new_view.uidnext()?);
        if let Some(unseen) = new_view.unseen()? {
            data.push(unseen);
        }

        Ok((new_view, data))
    }

    /// Looks up state changes in the mailbox and produces a set of IMAP
    /// responses describing the changes.
    pub async fn sync_update(&mut self) -> Result<Vec<Body>> {
        self.mailbox.sync().await?;
        // TODO THIS IS JUST A TEST REMOVE LATER
        self.mailbox.test().await?;

        self.update().await
    }

    /// Produces a set of IMAP responses describing the change between
    /// what the client knows and what is actually in the mailbox.
    pub async fn update(&mut self) -> Result<Vec<Body>> {
        let new_view = MailboxView {
            mailbox: self.mailbox.clone(),
            known_state: self.mailbox.current_uid_index().await,
        };

        let mut data = Vec::<Body>::new();

        if new_view.known_state.uidvalidity != self.known_state.uidvalidity {
            // TODO: do we want to push less/more info than this?
            data.push(new_view.uidvalidity()?);
            data.push(new_view.exists()?);
            data.push(new_view.uidnext()?);
        } else {
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
            if new_view.known_state.table.len() != self.known_state.table.len() - n_expunge {
                data.push(new_view.exists()?);
            }

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

    /// Looks up state changes in the mailbox and produces a set of IMAP
    /// responses describing the new state.
    pub async fn fetch(
        &self,
        sequence_set: &SequenceSet,
        attributes: &MacroOrFetchAttributes,
        uid: &bool,
    ) -> Result<Vec<Body>> {
        if *uid {
            bail!("UID FETCH not implemented");
        }

        let mail_vec = self
            .known_state
            .idx_by_uid
            .iter()
            .map(|(uid, uuid)| (*uid, *uuid))
            .collect::<Vec<_>>();

        let mut mails = vec![];
        let iter_strat = sequence::Strategy::Naive {
            largest: NonZeroU32::try_from((self.known_state.idx_by_uid.len() + 1) as u32).unwrap(),
        };
        for i in sequence_set.iter(iter_strat) {
            if let Some(mail) = mail_vec.get(i.get() as usize - 1) {
                mails.push((i, *mail));
            } else {
                bail!("No such mail: {}", i);
            }
        }

        let mails_uuid = mails
            .iter()
            .map(|(_i, (_uid, uuid))| *uuid)
            .collect::<Vec<_>>();
        let mails_meta = self.mailbox.fetch_meta(&mails_uuid).await?;

        let fetch_attrs = match attributes {
            MacroOrFetchAttributes::Macro(m) => m.expand(),
            MacroOrFetchAttributes::FetchAttributes(a) => a.clone(),
        };
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
                .map(|((i, (uid, uuid)), meta)| async move {
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
                .map(|((i, (uid, uuid)), meta)| (i, uid, uuid, meta, None))
                .collect::<Vec<_>>()
        };

        let mut ret = vec![];
        for (i, uid, uuid, meta, body) in mails {
            let mut attributes = vec![MessageAttribute::Uid(uid)];

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
                    FetchAttribute::Uid => (),
                    FetchAttribute::Flags => {
                        attributes.push(MessageAttribute::Flags(
                            flags.iter().filter_map(|f| string_to_flag(f)).collect(),
                        ));
                    }
                    FetchAttribute::Rfc822Size => {
                        attributes.push(MessageAttribute::Rfc822Size(meta.rfc822_size as u32))
                    }
                    FetchAttribute::Rfc822Header => attributes.push(
                        MessageAttribute::Rfc822Header(NString(Some(IString::Literal(
                            meta.headers
                                .clone()
                                .try_into()
                                .or(Err(Error::msg("IString conversion error")))?,
                        )))),
                    ),
                    FetchAttribute::Rfc822Text => {
                        let r = parsed
                            .raw_message.get(parsed.offset_body..parsed.offset_end)
                            .ok_or(Error::msg("Unable to extract email body, cursors out of bound. This is a bug."))?
                            .try_into()
                            .or(Err(Error::msg("IString conversion error")))?;

                        attributes.push(MessageAttribute::Rfc822Text(NString(Some(
                            IString::Literal(r),
                        ))))
                    }
                    FetchAttribute::Rfc822 => {
                        attributes.push(MessageAttribute::Rfc822(NString(Some(IString::Literal(
                            body.as_ref().unwrap().clone().try_into().unwrap(),
                        )))))
                    }
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
                        // @TODO This is a stub
                        let is = IString::try_from("test").unwrap();

                        attributes.push(MessageAttribute::BodyExt {
                            section: None,
                            origin: None,
                            data: NString(Some(is)),
                        })
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

    /// Produce an OK [UIDVALIDITY _] message corresponding to `known_state`
    fn uidvalidity(&self) -> Result<Body> {
        let uid_validity = Status::ok(
            None,
            Some(Code::UidValidity(self.known_state.uidvalidity)),
            "UIDs valid",
        )
        .map_err(Error::msg)?;
        Ok(Body::Status(uid_validity))
    }

    /// Produce an OK [UIDNEXT _] message corresponding to `known_state`
    fn uidnext(&self) -> Result<Body> {
        let next_uid = Status::ok(
            None,
            Some(Code::UidNext(self.known_state.uidnext)),
            "Predict next UID",
        )
        .map_err(Error::msg)?;
        Ok(Body::Status(next_uid))
    }

    /// Produces an UNSEEN message (if relevant) corresponding to the
    /// first unseen message id in `known_state`
    fn unseen(&self) -> Result<Option<Body>> {
        let unseen = self
            .known_state
            .idx_by_flag
            .get(&"$unseen".to_string())
            .and_then(|os| os.get_min())
            .cloned();
        if let Some(unseen) = unseen {
            let status_unseen =
                Status::ok(None, Some(Code::Unseen(unseen.clone())), "First unseen UID")
                    .map_err(Error::msg)?;
            Ok(Some(Body::Status(status_unseen)))
        } else {
            Ok(None)
        }
    }

    /// Produce an EXISTS message corresponding to the number of mails
    /// in `known_state`
    fn exists(&self) -> Result<Body> {
        let exists = u32::try_from(self.known_state.idx_by_uid.len())?;
        Ok(Body::Data(Data::Exists(exists)))
    }

    /// Produce a RECENT message corresponding to the number of
    /// recent mails in `known_state`
    fn recent(&self) -> Result<Body> {
        let recent = self
            .known_state
            .idx_by_flag
            .get(&"\\Recent".to_string())
            .map(|os| os.len())
            .unwrap_or(0);
        let recent = u32::try_from(recent)?;
        Ok(Body::Data(Data::Recent(recent)))
    }

    /// Produce a FLAGS and a PERMANENTFLAGS message that indicates
    /// the flags that are in `known_state` + default flags
    fn flags(&self) -> Result<Vec<Body>> {
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
}

fn string_to_flag(f: &str) -> Option<Flag> {
    match f.chars().next() {
        Some('\\') => None,
        Some('$') if f == "$unseen" => None,
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

fn message_envelope(msg: &mail_parser::Message<'_>) -> Envelope {
    Envelope {
        date: NString(
            msg.get_date()
                .map(|d| IString::try_from(d.to_iso8601()).unwrap()),
        ),
        subject: NString(
            msg.get_subject()
                .map(|d| IString::try_from(d.to_string()).unwrap()),
        ),
        from: convert_addresses(msg.get_from()),
        sender: convert_addresses(msg.get_sender()),
        reply_to: convert_addresses(msg.get_reply_to()),
        to: convert_addresses(msg.get_to()),
        cc: convert_addresses(msg.get_cc()),
        bcc: convert_addresses(msg.get_bcc()),
        in_reply_to: NString(None), // TODO
        message_id: NString(
            msg.get_message_id()
                .map(|d| IString::try_from(d.to_string()).unwrap()),
        ),
    }
}

fn convert_addresses(a: &mail_parser::HeaderValue<'_>) -> Vec<Address> {
    match a {
        mail_parser::HeaderValue::Address(a) => vec![convert_address(a)],
        mail_parser::HeaderValue::AddressList(a) => {
            let mut ret = vec![];
            for aa in a {
                ret.push(convert_address(aa));
            }
            ret
        }
        mail_parser::HeaderValue::Empty => vec![],
        mail_parser::HeaderValue::Collection(c) => {
            let mut ret = vec![];
            for cc in c.iter() {
                ret.extend(convert_addresses(cc).into_iter());
            }
            ret
        }
        _ => panic!("Invalid address header"),
    }
}

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
                    let (attrs, mut basic) = headers_to_basic_fields(bp)?;

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
                                    bp.get_text_contents().lines().count(),
                                )?,
                            },
                        },
                        extension: None,
                    })
                }
                MessagePart::Binary(_) | MessagePart::InlineBinary(_) => {
                    /*
                     * Note also that a subtype specification is MANDATORY -- it may not be
                     * omitted from a Content-Type header field.  As such, there are no
                     * default subtypes.
                     */
                    todo!()
                }
                MessagePart::Message(_) => todo!(),
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
fn headers_to_basic_fields<'a>(
    bp: &(impl BodyPart<'a> + MimeHeaders<'a>),
) -> Result<(SpecialAttrs, BasicFields)> {
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

        size: u32::try_from(bp.len())?,
    };

    Ok((attrs, bf))
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
        ];

        for pref in prefixes.iter() {
            println!("{}", pref);
            let txt = fs::read(format!("{}.eml", pref))?;
            let exp = fs::read(format!("{}.body", pref))?;
            let message = Message::parse(&txt).unwrap();

            let mut resp = Vec::new();
            MessageAttribute::Body(build_imap_email_struct(&message, &message.structure)?)
                .encode(&mut resp);

            let resp_str = String::from_utf8_lossy(&resp).to_lowercase();

            let exp_no_parenthesis = &exp[1..exp.len() - 1];
            let exp_str = String::from_utf8_lossy(exp_no_parenthesis).to_lowercase();

            println!("aerogramme: {}\ndovecot:    {}", resp_str, exp_str);
            assert_eq!(resp_str, exp_str);
        }

        Ok(())
    }
}
