use std::borrow::Cow;
use std::collections::HashSet;
use std::num::NonZeroU32;

use anyhow::{anyhow, bail, Result};

use imap_codec::imap_types::body::{
    BasicFields, Body as FetchBody, BodyStructure, MultiPartExtensionData, SinglePartExtensionData,
    SpecificFields,
};
use imap_codec::imap_types::bounded_static::IntoBoundedStatic;
use imap_codec::imap_types::core::{AString, IString, NString, Vec1};
use imap_codec::imap_types::fetch::{Part as FetchPart, Section as FetchSection};

use eml_codec::{
    header,
    message::{field::MessageField, Message},
    mime,
    part::{composite, discrete, field::EntityField, AnyPart, MimeBody},
    raw_input::RawInput,
    text::misc_token::MIMEWord,
    text::words::MIMEAtom,
};

use crate::imap::imf_view::ImfView;

pub enum BodySection<'a> {
    Full(Cow<'a, [u8]>),
    Slice {
        body: Cow<'a, [u8]>,
        origin_octet: u32,
    },
}

/// `NodeMime` represents a generic email entity; it can be a whole message or a
/// MIME part. It is used for recursively visiting an email AST, and is used
/// both to implement FETCH BODY[<section>] and FETCH BODYSTRUCTURE.
#[derive(Clone, Copy, Debug)]
enum NodeMime<'a> {
    Message(&'a Message<'a>),
    AnyPart(&'a AnyPart<'a>),
}

/// Logic for BODY[<section>]<<partial>>
/// Works in 3 steps:
///  1. Find the email subpart selected by the section's part numbers
///  2. Apply the extraction logic corresponding to the section's non-numeric
///  part specifier, if any (like TEXT, HEADER, etc.)
///  3. Keep only the given byte range specified by partial
///
/// Example of message sections:
///
/// ```text
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
///     ```
pub fn body_ext<'a>(
    msg: &'a Message<'a>,
    section: &'a Option<FetchSection<'a>>,
    partial: &'a Option<(u32, NonZeroU32)>,
) -> Result<BodySection<'a>> {
    let root_mime = NodeMimeSub::from_message(msg);
    let (extractor, path) = SubsettedSection::from(section);
    // Step 1: select subpart
    let sub_mime = root_mime.subset(path)?;
    // Step 2: extract part specifier
    let extracted_full = sub_mime.extract(&extractor)?;
    // Step 3: apply partial byte range
    let extracted_partial = extracted_full.to_body_section(partial); 
    Ok(extracted_partial)
}

/// The subset selected by `NodeMime::subset`. This is either a `NodeMime` or a
/// discrete message body.
#[derive(Clone, Debug)]
enum NodeMimeSub<'a> {
    NodeMime(NodeMime<'a>),
    DiscreteBody(DiscreteBody<'a>),
}

/// A discrete email body (i.e. text)
#[derive(Clone, Debug)]
enum DiscreteBody<'a> {
    Text(&'a discrete::Text<'a>),
    Binary(&'a discrete::Binary<'a>),
}

impl<'a> NodeMimeSub<'a> {
    fn from_message(msg: &'a Message<'a>) -> Self {
        Self::NodeMime(NodeMime::Message(msg))
    }

    /// Returns the sub-part selected by `path` (a sequence of part numbers).
    fn subset(self, path: Option<&'a FetchPart>) -> Result<NodeMimeSub<'a>> {
        match path {
            None => Ok(self),
            Some(v) => self.rec_subset(v.0.as_ref()),
        }
    }

    fn rec_subset(self, path: &'a [NonZeroU32]) -> Result<NodeMimeSub<'a>> {
        if path.is_empty() {
            Ok(self)
        } else {
            let node_mime = match &self {
                NodeMimeSub::DiscreteBody(_) =>
                    bail!("tried to access a subpart on an atomic part (text or binary). Unresolved subpath {:?}",
                          path),
                NodeMimeSub::NodeMime(m) =>
                    m
            };

            match (&node_mime, node_mime.mime_body()) {
                (_, MimeBody::Mult(x)) => {
                    // If this contains a multipart, select a part and explore it recursively.
                    let next = Self::NodeMime(NodeMime::AnyPart(x.children
                        .get(path[0].get() as usize - 1)
                        .ok_or(anyhow!("Unable to resolve subpath {:?}, current multipart has only {} elements", path, x.children.len()))?));
                    next.rec_subset(&path[1..])
                },
                (node_mime, mime_body) => {
                    // Otherwise, there is only a single child to this node
                    let next = match mime_body {
                        MimeBody::Msg(msg) => Self::NodeMime(NodeMime::Message(&msg.child)),
                        MimeBody::Txt(txt) => Self::DiscreteBody(DiscreteBody::Text(txt)),
                        MimeBody::Bin(bin) => Self::DiscreteBody(DiscreteBody::Binary(bin)),
                        MimeBody::Mult(_) => unreachable!("already handled in earlier match case"),
                    };
                    // Recursion through through `next` depends on the type of
                    // the current node...
                    match node_mime {
                        NodeMime::Message(_) => {
                            // Non-multipart full messages behave as containing
                            // a single part with their contents.
                            //
                            // "Every message has at least one part number. Messages
                            // that do not use MIME, and MIME messages that are not
                            // multipart and have no encapsulated message within them,
                            // only have a part 1."
                            if path[0].get() > 1 {
                                bail!("Tried to access subpart {} of a message which has only one part", path[0].get())
                            }
                            next.rec_subset(&path[1..])
                        },
                        NodeMime::AnyPart(_) => {
                            // Non-multipart MIME parts behave as their underlying
                            // contents, without an indirection.
                            next.rec_subset(path)
                        },
                    }
                },
            }
        }
    }

    /// Extract the information specified by a (non-numeric) part specifier: TEXT, HEADER, etc.
    fn extract(&self, extractor: &SubsettedSection<'a>) -> Result<ExtractedFull<'a>> {
        match extractor {
            SubsettedSection::Text => self.text(),
            SubsettedSection::Header => self.header(),
            SubsettedSection::HeaderFields(fields) => self.header_fields(fields, false),
            SubsettedSection::HeaderFieldsNot(fields) => self.header_fields(fields, true),
            SubsettedSection::This => self.this(),
            SubsettedSection::Mime => self.mime(),
        }
    }

    /// The TEXT part specifier refers to the text body of the message, omitting the [RFC-2822] header.
    ///
    /// This should be understood to behave as HEADER: on a part, requires the
    /// part to contain an encapsulated message, and return its body.
    fn text(&self) -> Result<ExtractedFull<'a>> {
        let msg = self.entire_or_encapsulated_message()?;
        Ok(ExtractedFull(msg.mime_body.raw_body().unwrap().into()))
    }

    /// The HEADER [...] part specifiers refer to the [RFC-2822] header of the message or of
    /// an encapsulated [MIME-IMT] MESSAGE/RFC822 message.
    /// ```raw
    /// HEADER     ([RFC-2822] header of the message)
    /// ```
    /// 
    /// Note: consequently, the behavior of HEADER on a part that does not
    /// contain an encapsulated message is undefined. We return an error.
    ///
    /// The blank line is always included as part of the header data, except in
    /// the case of a message that has no body and no blank line.
    fn header(&self) -> Result<ExtractedFull<'a>> {
        Ok(ExtractedFull(
            self
                .entire_or_encapsulated_message()?
                .raw_headers
                .unwrap()
                .into()
        ))
    }

    /// The MIME part specifier refers to the [MIME-IMB] header for
    /// this part.
    fn mime(&self) -> Result<ExtractedFull<'a>> {
        let mime_headers = match self {
            Self::NodeMime(m) => m.mime_headers(),
            Self::DiscreteBody(_) =>
                bail!("Tried to fetch MIME headers of a discrete email body"),
        };
        let res = raw_kv_to_bytes(
            mime_headers
                .into_iter()
                .map(|(f, body)| (f.raw_name(), body)),
        );
        Ok(ExtractedFull(res.into()))
    }

    /// The "full contents" of the currently selected part; corresponds to the
    /// empty extractor. Notably, on MIME parts, this returns the body of the
    /// part, excluding its MIME headers.
    fn this(&self) -> Result<ExtractedFull<'a>> {
        let raw = match self {
            Self::NodeMime(NodeMime::Message(m)) => m.raw.unwrap(),
            Self::NodeMime(NodeMime::AnyPart(p)) => p.mime_body.raw_body().unwrap(),
            Self::DiscreteBody(DiscreteBody::Binary(b)) => b.raw_body.unwrap(),
            Self::DiscreteBody(DiscreteBody::Text(b)) => b.raw_body.unwrap(),
        };
        Ok(ExtractedFull(raw.into()))
    }

    /// The [...] HEADER.FIELDS, and HEADER.FIELDS.NOT part
    /// specifiers refer to the [RFC-2822] header of the message or of
    /// an encapsulated [MIME-IMT] MESSAGE/RFC822 message.
    /// HEADER.FIELDS and HEADER.FIELDS.NOT are followed by a list of
    /// field-name (as defined in [RFC-2822]) names, and return a
    /// subset of the header.  The subset returned by HEADER.FIELDS
    /// contains only those header fields with a field-name that
    /// matches one of the names in the list; similarly, the subset
    /// returned by HEADER.FIELDS.NOT contains only the header fields
    /// with a non-matching field-name.  The field-matching is
    /// case-insensitive but otherwise exact.
    fn header_fields(
        &self,
        fields: &'a Vec1<AString<'a>>,
        invert: bool,
    ) -> Result<ExtractedFull<'a>> {
        // Build a lowercase ascii hashset with the fields to fetch
        let index = fields
            .as_ref()
            .iter()
            .map(|x| x.as_ref().to_ascii_lowercase())
            .collect::<HashSet<_>>();

        // Filter headers
        let msg = self.entire_or_encapsulated_message()?;
        let res = raw_kv_to_bytes(
            msg
                .field_list()
                .into_iter()
                .filter_map(|f| {
                    let name = f.raw_name();
                    let keep = index.contains(&name.bytes().to_ascii_lowercase()) ^ invert;
                    keep.then_some((name, f.raw_body()))
                })
        );

        Ok(ExtractedFull(res.into()))
    }

    /// If `self` represents a full message or an encapsulated full message,
    /// return it. Otherwise raise an error.
    fn entire_or_encapsulated_message(&self) -> Result<&'a Message<'a>> {
        match self {
            Self::NodeMime(NodeMime::Message(msg)) => Ok(*msg),
            Self::NodeMime(NodeMime::AnyPart(part)) => part
                .mime_body
                .as_message()
                .ok_or(anyhow!("Tried to fetch the encapsulated message of a part of a different type"))
                .map(|mime_msg| &*mime_msg.child),
            Self::DiscreteBody(_) =>
                bail!("Tried to use a discrete body as a full message")
        }
    }
}

/// Logic for BODY and BODYSTRUCTURE
///
/// ```raw
/// b fetch 29878:29879 (BODY)
/// * 29878 FETCH (BODY (("text" "plain" ("charset" "utf-8") NIL NIL "quoted-printable" 3264 82)("text" "html" ("charset" "utf-8") NIL NIL "quoted-printable" 31834 643) "alternative"))
/// * 29879 FETCH (BODY ("text" "html" ("charset" "us-ascii") NIL NIL "7bit" 4107 131))
///                                   ^^^^^^^^^^^^^^^^^^^^^^ ^^^ ^^^ ^^^^^^ ^^^^ ^^^
///                                   |                      |   |   |      |    | number of lines
///                                   |                      |   |   |      | size
///                                   |                      |   |   | content transfer encoding
///                                   |                      |   | description
///                                   |                      | id
///                                   | parameter list
/// b OK Fetch completed (0.001 + 0.000 secs).
/// ```
pub fn bodystructure<'a>(msg: &Message<'a>, is_ext: bool) -> Result<BodyStructure<'static>> {
    NodeMime::Message(msg).structure(is_ext)
}

pub fn raw_kv_headers<'a>(msg: &'a Message<'a>) -> Vec<(Cow<'a, [u8]>, &'a [u8])> {
    msg
        .field_list()
        .into_iter()
        .map(|f| (f.raw_name().0, f.raw_body().unwrap()))
        .collect()
}

impl<'a> NodeMime<'a> {
    fn structure(&self, is_ext: bool) -> Result<BodyStructure<'static>> {
        match &self.mime_body() {
            MimeBody::Txt(x) => NodeTxt(self, x).structure(is_ext),
            MimeBody::Bin(x) => NodeBin(self, x).structure(is_ext),
            MimeBody::Mult(x) => NodeMult(self, x).structure(is_ext),
            MimeBody::Msg(x) => NodeMsg(self, x).structure(is_ext),
        }
    }

    /// Basic fields of a MIME entity, common to all entity types
    fn basic_fields(&self) -> Result<BasicFields<'static>> {
        let sz = self.mime_body().raw_body().unwrap().len();
        let m = self.mime_body().mime();
        let parameter_list: Vec<(IString<'static>, IString<'static>)> = m
            .ctype()
            .params()
            .iter()
            .map(|p| {
                (
                    mime_atom_to_istring(&p.name),
                    mime_word_to_istring(&p.value),
                )
            })
            .collect();
        let common = m.common();

        Ok(BasicFields {
            parameter_list,
            id: NString(
                common
                    .id
                    .as_ref()
                    .and_then(|ci| IString::try_from(ci.to_string()).ok()),
            ),
            description: NString(
                common
                    .description
                    .as_ref()
                    .and_then(|cd| IString::try_from(cd.to_string()).ok()),
            ),
            content_transfer_encoding: match &common.transfer_encoding {
                mime::mechanism::Mechanism::_7Bit => unchecked_istring("7bit"),
                mime::mechanism::Mechanism::_8Bit => unchecked_istring("8bit"),
                mime::mechanism::Mechanism::Binary => unchecked_istring("binary"),
                mime::mechanism::Mechanism::QuotedPrintable => {
                    unchecked_istring("quoted-printable")
                }
                mime::mechanism::Mechanism::Base64 => unchecked_istring("base64"),
                mime::mechanism::Mechanism::Other(m) => mime_atom_to_istring(m),
            },
            size: u32::try_from(sz)?,
        })
    }
}

//----------------------------------------------------------
// Auxiliary functions

impl<'a> NodeMime<'a> {
    fn mime_body(&self) -> &'a MimeBody<'a> {
        match self {
            NodeMime::Message(m) => &m.mime_body,
            NodeMime::AnyPart(p) => &p.mime_body,
        }
    }

    fn mime_headers(&self) -> Vec<(mime::field::Field<'a>, RawInput<'a>)> {
        match self {
            NodeMime::Message(m) => m
                .field_list()
                .into_iter()
                .filter_map(|f| {
                    if let MessageField::MIME { f, raw_body } = f {
                        Some((f, raw_body))
                    } else {
                        None
                    }
                })
                .collect(),
            NodeMime::AnyPart(p) => p
                .field_list()
                .into_iter()
                .filter_map(|f| {
                    if let EntityField::MIME { f, raw_body } = f {
                        Some((f, raw_body))
                    } else {
                        None
                    }
                })
                .collect(),
        }
    }
}

//----------------------------------------------------------

/// A FetchSection must be handled in 2 times:
///  - First we must extract the MIME part
///  - Then we must process it as desired
/// The given struct mixes both work, so
/// we separate this work here.
#[derive(Debug)]
enum SubsettedSection<'a> {
    This,
    Header,
    HeaderFields(&'a Vec1<AString<'a>>),
    HeaderFieldsNot(&'a Vec1<AString<'a>>),
    Text,
    Mime,
}
impl<'a> SubsettedSection<'a> {
    fn from(section: &'a Option<FetchSection>) -> (Self, Option<&'a FetchPart>) {
        match section {
            Some(FetchSection::Text(maybe_part)) => (Self::Text, maybe_part.as_ref()),
            Some(FetchSection::Header(maybe_part)) => (Self::Header, maybe_part.as_ref()),
            Some(FetchSection::HeaderFields(maybe_part, fields)) => {
                (Self::HeaderFields(fields), maybe_part.as_ref())
            }
            Some(FetchSection::HeaderFieldsNot(maybe_part, fields)) => {
                (Self::HeaderFieldsNot(fields), maybe_part.as_ref())
            }
            Some(FetchSection::Mime(part)) => (Self::Mime, Some(part)),
            Some(FetchSection::Part(part)) => (Self::This, Some(part)),
            None => (Self::This, None),
        }
    }
}

// ---------------------------
struct NodeMsg<'a>(&'a NodeMime<'a>, &'a composite::Message<'a>);
impl<'a> NodeMsg<'a> {
    fn structure(&self, is_ext: bool) -> Result<BodyStructure<'static>> {
        let basic = self.0.basic_fields()?;

        Ok(BodyStructure::Single {
            body: FetchBody {
                basic,
                specific: SpecificFields::Message {
                    envelope: Box::new(ImfView(&self.1.child.imf).message_envelope()),
                    body_structure: Box::new(NodeMime::Message(&self.1.child).structure(is_ext)?),
                    number_of_lines: nol(self.1.child.raw.unwrap()),
                },
            },
            extension_data: match is_ext {
                true => Some(SinglePartExtensionData {
                    md5: NString(None),
                    tail: None,
                }),
                _ => None,
            },
        })
    }
}

#[allow(dead_code)]
struct NodeMult<'a>(&'a NodeMime<'a>, &'a composite::Multipart<'a>);
impl<'a> NodeMult<'a> {
    fn structure(&self, is_ext: bool) -> Result<BodyStructure<'static>> {
        let ctype = &self.1.mime.ctype;
        let params: Vec<_> = ctype
            .params()
            .into_iter()
            .map(|p| {
                (
                    mime_atom_to_istring(&p.name),
                    mime_word_to_istring(&p.value),
                )
            })
            .collect();
        // SAFETY: MultipartSubtype is a parsed MIMEAtom, which is safe to convert to IString
        let subtype = IString::try_from(ctype.subtype.as_bytes())
            .unwrap()
            .into_static();

        let inner_bodies: Vec<_> = self
            .1
            .children
            .iter()
            .filter_map(|inner| NodeMime::AnyPart(&inner).structure(is_ext).ok())
            .collect();
        let bodies = Vec1::try_from(inner_bodies)?;

        Ok(BodyStructure::Multi {
            bodies,
            subtype,
            extension_data: match is_ext {
                true => Some(MultiPartExtensionData {
                    parameter_list: params,
                    tail: None,
                }),
                _ => None,
            },
        })
    }
}
struct NodeTxt<'a>(&'a NodeMime<'a>, &'a discrete::Text<'a>);
impl<'a> NodeTxt<'a> {
    fn structure(&self, is_ext: bool) -> Result<BodyStructure<'static>> {
        let basic = self.0.basic_fields()?;
        // TextSubtype is a parsed MIMEAtom, which is safe to convert to IString
        let subtype = IString::try_from(self.1.mime.ctype.subtype.as_bytes())
            .unwrap()
            .into_static();

        Ok(BodyStructure::Single {
            body: FetchBody {
                basic,
                specific: SpecificFields::Text {
                    subtype,
                    number_of_lines: nol(&self.1.body),
                },
            },
            extension_data: match is_ext {
                true => Some(SinglePartExtensionData {
                    md5: NString(None),
                    tail: None,
                }),
                _ => None,
            },
        })
    }
}

struct NodeBin<'a>(&'a NodeMime<'a>, &'a discrete::Binary<'a>);
impl<'a> NodeBin<'a> {
    fn structure(&self, is_ext: bool) -> Result<BodyStructure<'static>> {
        let basic = self.0.basic_fields()?;
        let ctype = &self.1.mime.ctype.ctype;
        let r#type = mime_atom_to_istring(&ctype.main);
        let subtype = mime_atom_to_istring(&ctype.sub);

        Ok(BodyStructure::Single {
            body: FetchBody {
                basic,
                specific: SpecificFields::Basic { r#type, subtype },
            },
            extension_data: match is_ext {
                true => Some(SinglePartExtensionData {
                    md5: NString(None),
                    tail: None,
                }),
                _ => None,
            },
        })
    }
}

// ---------------------------

#[derive(Debug)]
struct ExtractedFull<'a>(Cow<'a, [u8]>);
impl<'a> ExtractedFull<'a> {
    /// It is possible to fetch a substring of the designated text.
    /// This is done by appending an open angle bracket ("<"), the
    /// octet position of the first desired octet, a period, the
    /// maximum number of octets desired, and a close angle bracket
    /// (">") to the part specifier.  If the starting octet is beyond
    /// the end of the text, an empty string is returned.
    ///
    /// Any partial fetch that attempts to read beyond the end of the
    /// text is truncated as appropriate.  A partial fetch that starts
    /// at octet 0 is returned as a partial fetch, even if this
    /// truncation happened.
    ///
    /// Note: This means that BODY[]<0.2048> of a 1500-octet message
    /// will return BODY[]<0> with a literal of size 1500, not
    /// BODY[].
    ///
    /// Note: A substring fetch of a HEADER.FIELDS or
    /// HEADER.FIELDS.NOT part specifier is calculated after
    /// subsetting the header.
    fn to_body_section(self, partial: &'_ Option<(u32, NonZeroU32)>) -> BodySection<'a> {
        match partial {
            Some((begin, len)) => self.partialize(*begin, *len),
            None => BodySection::Full(self.0),
        }
    }

    fn partialize(self, begin: u32, len: NonZeroU32) -> BodySection<'a> {
        // Asked range is starting after the end of the content,
        // returning an empty buffer
        if begin as usize > self.0.len() {
            return BodySection::Slice {
                body: Cow::Borrowed(&[][..]),
                origin_octet: begin,
            };
        }

        // Asked range is ending after the end of the content,
        // slice only the beginning of the buffer
        if (begin + len.get()) as usize >= self.0.len() {
            return BodySection::Slice {
                body: match self.0 {
                    Cow::Borrowed(body) => Cow::Borrowed(&body[begin as usize..]),
                    Cow::Owned(body) => Cow::Owned(body[begin as usize..].to_vec()),
                },
                origin_octet: begin,
            };
        }

        // Range is included inside the considered content,
        // this is the "happy case"
        BodySection::Slice {
            body: match self.0 {
                Cow::Borrowed(body) => {
                    Cow::Borrowed(&body[begin as usize..(begin + len.get()) as usize])
                }
                Cow::Owned(body) => {
                    Cow::Owned(body[begin as usize..(begin + len.get()) as usize].to_vec())
                }
            },
            origin_octet: begin,
        }
    }
}

/// ---- helpers

/// s is set to static to ensure that only compile time values
/// checked by developpers are passed.
fn unchecked_istring(s: &'static str) -> IString<'static> {
    IString::try_from(s).expect("this value is expected to be a valid imap-codec::IString")
}

// Number Of Lines
fn nol(input: &[u8]) -> u32 {
    // NOTE: this line computation is somewhat strange, as it counts 0 lines for
    // a text that has no \n terminator (counting 1 line may be more intuitive);
    // but it seems to match what dovecot does...
    input
        .iter()
        .filter(|x| **x == b'\n')
        .count()
        .try_into()
        .unwrap_or(0)
}

/// Formats a subset of header fields.
///
/// The result must include the final separating blank line between headers and
/// body:
/// 
/// Note also that the [RFC5322] delimiting blank line between the header and
/// the body is not affected by header-line subsetting; the blank line is always
/// included as part of the header data, except in the case of a message that
/// has no body and no blank line.
fn raw_kv_to_bytes<'a, I>(kv: I) -> Vec<u8>
where
    I: Iterator<Item = (header::FieldName<'a>, RawInput<'a>)>,
{
    let mut res = kv.fold(vec![], |mut acc, (k, v)| {
        acc.extend(k.bytes());
        acc.extend(b":");
        acc.extend(v.unwrap());
        acc.extend(b"\r\n");
        acc
    });
    // FIXME: for now we always add the separating blank line, even if there was
    // none in the original message (eml_codec does not currently expose in its
    // AST the separating blank line between headers & body).
    res.extend(b"\r\n");
    res
}

fn mime_atom_to_istring<'a>(a: &MIMEAtom<'a>) -> IString<'static> {
    // A MIMEAtom can always be represented as an IString. The only requirement
    // of an IString is that it does not contain null bytes. This is always true
    // of a MIMEAtom.
    IString::try_from(a.0.as_ref()).unwrap().into_static()
}

fn mime_word_to_istring<'a>(w: &MIMEWord<'a>) -> IString<'static> {
    // A MIMEWord can always be represented as an IString. A MIMEWord does not
    // contain null bytes.
    IString::try_from(w.chars().collect::<String>()).unwrap()
}
