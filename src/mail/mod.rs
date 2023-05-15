use std::convert::TryFrom;
use std::io::Write;

pub mod incoming;
pub mod mailbox;
pub mod uidindex;
pub mod unique_ident;
pub mod user;

// Internet Message Format
// aka RFC 822 - RFC 2822 - RFC 5322
// 2023-05-15 don't want to refactor this struct now.
#[allow(clippy::upper_case_acronyms)]
pub struct IMF<'a> {
    raw: &'a [u8],
    parsed: mail_parser::Message<'a>,
}

impl<'a> TryFrom<&'a [u8]> for IMF<'a> {
    type Error = ();

    fn try_from(body: &'a [u8]) -> Result<IMF<'a>, ()> {
        eprintln!("---- BEGIN PARSED MESSAGE ----");
        let _ = std::io::stderr().write_all(body);
        eprintln!("---- END PARSED MESSAGE ----");
        let parsed = mail_parser::Message::parse(body).ok_or(())?;
        Ok(Self { raw: body, parsed })
    }
}
