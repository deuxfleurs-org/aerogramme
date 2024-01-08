use std::borrow::Cow;
use std::collections::HashSet;
use std::num::NonZeroU32;

use anyhow::{anyhow, bail, Result};

use imap_codec::imap_types::body::{
    BasicFields, Body as FetchBody, BodyStructure, MultiPartExtensionData, SinglePartExtensionData,
    SpecificFields,
};
use imap_codec::imap_types::core::{AString, IString, NString, NonEmptyVec};
use imap_codec::imap_types::fetch::{Part as FetchPart, Section as FetchSection};

use eml_codec::{
    header, mime, mime::r#type::Deductible, part::composite, part::discrete, part::AnyPart,
};

use crate::imap::imf_view::ImfView;

pub enum BodySection<'a> {
    Full(Cow<'a, [u8]>),
    Slice {
        body: Cow<'a, [u8]>,
        origin_octet: u32,
    },
}

/// Logic for BODY[<section>]<<partial>>
/// Works in 3 times:
///  1. Find the section (RootMime::subset)
///  2. Apply the extraction logic (SelectedMime::extract), like TEXT, HEADERS, etc.
///  3. Keep only the given subset provided by partial
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
pub fn body_ext<'a>(
    part: &'a AnyPart<'a>,
    section: &'a Option<FetchSection<'a>>,
    partial: &'a Option<(u32, NonZeroU32)>,
) -> Result<BodySection<'a>> {
    let root_mime = NodeMime(part);
    let (extractor, path) = SubsettedSection::from(section);
    let selected_mime = root_mime.subset(path)?;
    let extracted_full = selected_mime.extract(&extractor)?;
    Ok(extracted_full.to_body_section(partial))
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
pub fn bodystructure(part: &AnyPart, is_ext: bool) -> Result<BodyStructure<'static>> {
    NodeMime(part).structure(is_ext)
}

/// NodeMime
///
/// Used for recursive logic on MIME.
/// See SelectedMime for inspection.
struct NodeMime<'a>(&'a AnyPart<'a>);
impl<'a> NodeMime<'a> {
    /// A MIME object is a tree of elements.
    /// The path indicates which element must be picked.
    /// This function returns the picked element as the new view
    fn subset(self, path: Option<&'a FetchPart>) -> Result<SelectedMime<'a>> {
        match path {
            None => Ok(SelectedMime(self.0)),
            Some(v) => self.rec_subset(v.0.as_ref()),
        }
    }

    fn rec_subset(self, path: &'a [NonZeroU32]) -> Result<SelectedMime> {
        if path.is_empty() {
            Ok(SelectedMime(self.0))
        } else {
            match self.0 {
                AnyPart::Mult(x) => {
                    let next = Self(x.children
                        .get(path[0].get() as usize - 1)
                        .ok_or(anyhow!("Unable to resolve subpath {:?}, current multipart has only {} elements", path, x.children.len()))?);
                    next.rec_subset(&path[1..])
                },
                AnyPart::Msg(x) => {
                    let next = Self(x.child.as_ref());
                    next.rec_subset(path)
                },
                _ => bail!("You tried to access a subpart on an atomic part (text or binary). Unresolved subpath {:?}", path),
            }
        }
    }

    fn structure(&self, is_ext: bool) -> Result<BodyStructure<'static>> {
        match self.0 {
            AnyPart::Txt(x) => NodeTxt(self, x).structure(is_ext),
            AnyPart::Bin(x) => NodeBin(self, x).structure(is_ext),
            AnyPart::Mult(x) => NodeMult(self, x).structure(is_ext),
            AnyPart::Msg(x) => NodeMsg(self, x).structure(is_ext),
        }
    }
}

//----------------------------------------------------------

/// A FetchSection must be handled in 2 times:
///  - First we must extract the MIME part
///  - Then we must process it as desired
/// The given struct mixes both work, so
/// we separate this work here.
enum SubsettedSection<'a> {
    Part,
    Header,
    HeaderFields(&'a NonEmptyVec<AString<'a>>),
    HeaderFieldsNot(&'a NonEmptyVec<AString<'a>>),
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
            Some(FetchSection::Part(part)) => (Self::Part, Some(part)),
            None => (Self::Part, None),
        }
    }
}

/// Used for current MIME inspection
///
/// See NodeMime for recursive logic
pub struct SelectedMime<'a>(pub &'a AnyPart<'a>);
impl<'a> SelectedMime<'a> {
    pub fn header_value(&'a self, to_match_ext: &[u8]) -> Option<&'a [u8]> {
        let to_match = to_match_ext.to_ascii_lowercase();

        self.eml_mime()
            .kv
            .iter()
            .filter_map(|field| match field {
                header::Field::Good(header::Kv2(k, v)) => Some((k, v)),
                _ => None,
            })
            .find(|(k, _)| k.to_ascii_lowercase() == to_match)
            .map(|(_, v)| v)
            .copied()
    }

    /// The subsetted fetch section basically tells us the
    /// extraction logic to apply on our selected MIME.
    /// This function acts as a router for these logic.
    fn extract(&self, extractor: &SubsettedSection<'a>) -> Result<ExtractedFull<'a>> {
        match extractor {
            SubsettedSection::Text => self.text(),
            SubsettedSection::Header => self.header(),
            SubsettedSection::HeaderFields(fields) => self.header_fields(fields, false),
            SubsettedSection::HeaderFieldsNot(fields) => self.header_fields(fields, true),
            SubsettedSection::Part => self.part(),
            SubsettedSection::Mime => self.mime(),
        }
    }

    fn mime(&self) -> Result<ExtractedFull<'a>> {
        let bytes = match &self.0 {
            AnyPart::Txt(p) => p.mime.fields.raw,
            AnyPart::Bin(p) => p.mime.fields.raw,
            AnyPart::Msg(p) => p.child.mime().raw,
            AnyPart::Mult(p) => p.mime.fields.raw,
        };
        Ok(ExtractedFull(bytes.into()))
    }

    fn part(&self) -> Result<ExtractedFull<'a>> {
        let bytes = match &self.0 {
            AnyPart::Txt(p) => p.body,
            AnyPart::Bin(p) => p.body,
            AnyPart::Msg(p) => p.raw_part,
            AnyPart::Mult(_) => bail!("Multipart part has no body"),
        };
        Ok(ExtractedFull(bytes.to_vec().into()))
    }

    fn eml_mime(&self) -> &eml_codec::mime::NaiveMIME<'_> {
        match &self.0 {
            AnyPart::Msg(msg) => msg.child.mime(),
            other => other.mime(),
        }
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
        fields: &'a NonEmptyVec<AString<'a>>,
        invert: bool,
    ) -> Result<ExtractedFull<'a>> {
        // Build a lowercase ascii hashset with the fields to fetch
        let index = fields
            .as_ref()
            .iter()
            .map(|x| {
                match x {
                    AString::Atom(a) => a.inner().as_bytes(),
                    AString::String(IString::Literal(l)) => l.as_ref(),
                    AString::String(IString::Quoted(q)) => q.inner().as_bytes(),
                }
                .to_ascii_lowercase()
            })
            .collect::<HashSet<_>>();

        // Extract MIME headers
        let mime = self.eml_mime();

        // Filter our MIME headers based on the field index
        // 1. Keep only the correctly formatted headers
        // 2. Keep only based on the index presence or absence
        // 3. Reduce as a byte vector
        let buffer = mime
            .kv
            .iter()
            .filter_map(|field| match field {
                header::Field::Good(header::Kv2(k, v)) => Some((k, v)),
                _ => None,
            })
            .filter(|(k, _)| index.contains(&k.to_ascii_lowercase()) ^ invert)
            .fold(vec![], |mut acc, (k, v)| {
                acc.extend(*k);
                acc.extend(b": ");
                acc.extend(*v);
                acc.extend(b"\r\n");
                acc
            });

        Ok(ExtractedFull(buffer.into()))
    }

    /// The HEADER [...] part specifiers refer to the [RFC-2822] header of the message or of
    /// an encapsulated [MIME-IMT] MESSAGE/RFC822 message.
    /// ```raw
    /// HEADER     ([RFC-2822] header of the message)
    /// ```
    fn header(&self) -> Result<ExtractedFull<'a>> {
        let msg = self
            .0
            .as_message()
            .ok_or(anyhow!("Selected part must be a message/rfc822"))?;
        Ok(ExtractedFull(msg.raw_headers.into()))
    }

    /// The TEXT part specifier refers to the text body of the message, omitting the [RFC-2822] header.
    fn text(&self) -> Result<ExtractedFull<'a>> {
        let msg = self
            .0
            .as_message()
            .ok_or(anyhow!("Selected part must be a message/rfc822"))?;
        Ok(ExtractedFull(msg.raw_body.into()))
    }

    // ------------

    /// Basic field of a MIME part that is
    /// common to all parts
    fn basic_fields(&self) -> Result<BasicFields<'static>> {
        let sz = match self.0 {
            AnyPart::Txt(x) => x.body.len(),
            AnyPart::Bin(x) => x.body.len(),
            AnyPart::Msg(x) => x.raw_part.len(),
            AnyPart::Mult(_) => 0,
        };
        let m = self.0.mime();
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
                mime::mechanism::Mechanism::QuotedPrintable => {
                    unchecked_istring("quoted-printable")
                }
                mime::mechanism::Mechanism::Base64 => unchecked_istring("base64"),
                _ => unchecked_istring("7bit"),
            },
            // @FIXME we can't compute the size of the message currently...
            size: u32::try_from(sz)?,
        })
    }
}

// ---------------------------
struct NodeMsg<'a>(&'a NodeMime<'a>, &'a composite::Message<'a>);
impl<'a> NodeMsg<'a> {
    fn structure(&self, is_ext: bool) -> Result<BodyStructure<'static>> {
        let basic = SelectedMime(self.0 .0).basic_fields()?;

        Ok(BodyStructure::Single {
            body: FetchBody {
                basic,
                specific: SpecificFields::Message {
                    envelope: Box::new(ImfView(&self.1.imf).message_envelope()),
                    body_structure: Box::new(NodeMime(&self.1.child).structure(is_ext)?),
                    number_of_lines: nol(self.1.raw_part),
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
struct NodeMult<'a>(&'a NodeMime<'a>, &'a composite::Multipart<'a>);
impl<'a> NodeMult<'a> {
    fn structure(&self, is_ext: bool) -> Result<BodyStructure<'static>> {
        let itype = &self.1.mime.interpreted_type;
        let subtype = IString::try_from(itype.subtype.to_string())
            .unwrap_or(unchecked_istring("alternative"));

        let inner_bodies = self
            .1
            .children
            .iter()
            .filter_map(|inner| NodeMime(&inner).structure(is_ext).ok())
            .collect::<Vec<_>>();

        NonEmptyVec::validate(&inner_bodies)?;
        let bodies = NonEmptyVec::unvalidated(inner_bodies);

        Ok(BodyStructure::Multi {
            bodies,
            subtype,
            extension_data: match is_ext {
                true => Some(MultiPartExtensionData {
                    parameter_list: vec![(
                        IString::try_from("boundary").unwrap(),
                        IString::try_from(self.1.mime.interpreted_type.boundary.to_string())?,
                    )],
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
        let mut basic = SelectedMime(self.0 .0).basic_fields()?;

        // Get the interpreted content type, set it
        let itype = match &self.1.mime.interpreted_type {
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
                    number_of_lines: nol(self.1.body),
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
        let basic = SelectedMime(self.0 .0).basic_fields()?;

        let default = mime::r#type::NaiveType {
            main: &b"application"[..],
            sub: &b"octet-stream"[..],
            params: vec![],
        };
        let ct = self.1.mime.fields.ctype.as_ref().unwrap_or(&default);

        let r#type = IString::try_from(String::from_utf8_lossy(ct.main).to_string()).or(Err(
            anyhow!("Unable to build IString from given Content-Type type given"),
        ))?;

        let subtype = IString::try_from(String::from_utf8_lossy(ct.sub).to_string()).or(Err(
            anyhow!("Unable to build IString from given Content-Type subtype given"),
        ))?;

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

/// ---- LEGACY

/// s is set to static to ensure that only compile time values
/// checked by developpers are passed.
fn unchecked_istring(s: &'static str) -> IString {
    IString::try_from(s).expect("this value is expected to be a valid imap-codec::IString")
}

// Number Of Lines
fn nol(input: &[u8]) -> u32 {
    input
        .iter()
        .filter(|x| **x == b'\n')
        .count()
        .try_into()
        .unwrap_or(0)
}
