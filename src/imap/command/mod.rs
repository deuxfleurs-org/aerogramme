pub mod anonymous;
pub mod anystate;
pub mod authenticated;
pub mod selected;

use crate::mail::namespace::INBOX;
use imap_codec::imap_types::mailbox::Mailbox as MailboxCodec;

/// Convert an IMAP mailbox name/identifier representation
/// to an utf-8 string that is used internally in Aerogramme
struct MailboxName<'a>(&'a MailboxCodec<'a>);
impl<'a> TryInto<&'a str> for MailboxName<'a> {
    type Error = std::str::Utf8Error;
    fn try_into(self) -> Result<&'a str, Self::Error> {
        match self.0 {
            MailboxCodec::Inbox => Ok(INBOX),
            MailboxCodec::Other(aname) => Ok(std::str::from_utf8(aname.as_ref())?),
        }
    }
}
