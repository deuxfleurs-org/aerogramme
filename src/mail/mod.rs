pub mod mailbox;
pub mod uidindex;
pub mod unique_ident;
pub mod user;

// Internet Message Format
// aka RFC 822 - RFC 2822 - RFC 5322
pub struct IMF<'a> {
    raw: &'a [u8],
    parsed: mail_parser::Message<'a>,
}
