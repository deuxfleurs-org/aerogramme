use std::convert::TryFrom;

pub mod mailbox;
pub mod uidindex;
pub mod unique_ident;
pub mod user;
pub mod incoming;

// Internet Message Format
// aka RFC 822 - RFC 2822 - RFC 5322
pub struct IMF<'a> {
    raw: &'a [u8],
    parsed: mail_parser::Message<'a>,
}

impl<'a> TryFrom<&'a [u8]> for IMF<'a> {
    type Error = ();

    fn try_from(body: &'a [u8]) -> Result<IMF<'a>, ()> {
        let parsed = mail_parser::Message::parse(body).ok_or(())?;
        Ok(Self { raw: body, parsed })
    }
}
