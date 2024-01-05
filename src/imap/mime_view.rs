use std::borrow::Cow;
use std::num::NonZeroU32;
use std::collections::HashSet;

use anyhow::{anyhow, bail, Result};

use imap_codec::imap_types::body::{BasicFields, Body as FetchBody, BodyStructure, SpecificFields};
use imap_codec::imap_types::core::{AString, IString, NonEmptyVec};
use imap_codec::imap_types::fetch::{
    Section as FetchSection, Part as FetchPart
};

use eml_codec::{
    header, part::AnyPart, part::composite, part::discrete,
};


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
    partial: &'a Option<(u32, NonZeroU32)>
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
pub fn bodystructure(part: &AnyPart) -> Result<BodyStructure<'static>> {
    unimplemented!();
}

/// NodeMime
///



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
            Some(FetchSection::HeaderFields(maybe_part, fields)) => (Self::HeaderFields(fields), maybe_part.as_ref()),
            Some(FetchSection::HeaderFieldsNot(maybe_part, fields)) => (Self::HeaderFieldsNot(fields), maybe_part.as_ref()),
            Some(FetchSection::Mime(part)) => (Self::Mime, Some(part)),
            Some(FetchSection::Part(part)) => (Self::Part, Some(part)),
            None => (Self::Part, None),
        }
    }
}

struct SelectedMime<'a>(&'a AnyPart<'a>);
impl<'a> SelectedMime<'a> {
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
    fn header_fields(&self, fields: &'a NonEmptyVec<AString<'a>>, invert: bool) -> Result<ExtractedFull<'a>> {
        // Build a lowercase ascii hashset with the fields to fetch
        let index = fields
            .as_ref()
            .iter()
            .map(|x| match x {
                AString::Atom(a) => a.inner().as_bytes(),
                AString::String(IString::Literal(l)) => l.as_ref(),
                AString::String(IString::Quoted(q)) => q.inner().as_bytes(),
            }.to_ascii_lowercase())
        .collect::<HashSet<_>>();

        // Extract MIME headers
        let mime = match &self.0 {
            AnyPart::Msg(msg) => msg.child.mime(),
            other => other.mime(),
        };

        // Filter our MIME headers based on the field index
        // 1. Keep only the correctly formatted headers
        // 2. Keep only based on the index presence or absence
        // 3. Reduce as a byte vector
        let buffer = mime.kv.iter()
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
        let msg = self.0.as_message().ok_or(anyhow!("Selected part must be a message/rfc822"))?;
        Ok(ExtractedFull(msg.raw_headers.into()))
    }

    /// The TEXT part specifier refers to the text body of the message, omitting the [RFC-2822] header.
    fn text(&self) -> Result<ExtractedFull<'a>> {
        let msg = self.0.as_message().ok_or(anyhow!("Selected part must be a message/rfc822"))?;
        Ok(ExtractedFull(msg.raw_body.into()))
    }

    // ------------
    
    /// Returns the structure of the message
    fn structure(&self) -> Result<BodyStructure<'static>> {
        unimplemented!();
    } 
}

// ---------------------------
struct SelectedMsg<'a>(&'a SelectedMime<'a>, &'a composite::Message<'a>);
struct SelectedMult<'a>(&'a SelectedMime<'a>, &'a composite::Multipart<'a>);
struct SelectedTxt<'a>(&'a SelectedMime<'a>, &'a discrete::Text<'a>);
struct SelectedBin<'a>(&'a SelectedMime<'a>, &'a discrete::Binary<'a>);

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
            }
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
            }
        }

        // Range is included inside the considered content,
        // this is the "happy case"
        BodySection::Slice {
            body: match self.0 {
                Cow::Borrowed(body) => Cow::Borrowed(&body[begin as usize..(begin + len.get()) as usize]),
                Cow::Owned(body) => Cow::Owned(body[begin as usize..(begin + len.get()) as usize].to_vec()),
            },
            origin_octet: begin,
        }
    }
}
