pub mod incoming;
pub mod mailbox;
pub mod query;
pub mod snapshot;
pub mod uidindex;
pub mod unique_ident;
pub mod namespace;

// Internet Message Format
// aka RFC 822 - RFC 2822 - RFC 5322
// 2023-05-15 don't want to refactor this struct now.
#[allow(clippy::upper_case_acronyms)]
pub struct IMF<'a> {
    raw: &'a [u8],
    parsed: eml_codec::part::composite::Message<'a>,
}

impl<'a> TryFrom<&'a [u8]> for IMF<'a> {
    type Error = ();

    fn try_from(body: &'a [u8]) -> Result<IMF<'a>, ()> {
        let parsed = eml_codec::parse_message(body).or(Err(()))?.1;
        Ok(Self { raw: body, parsed })
    }
}
