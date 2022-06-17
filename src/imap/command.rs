use anyhow::{Error, Result};
use boitalettres::errors::Error as BalError;
use boitalettres::proto::{Request, Response};
use imap_codec::types::core::{AString, Tag};
use imap_codec::types::fetch_attributes::MacroOrFetchAttributes;
use imap_codec::types::mailbox::{ListMailbox, Mailbox as MailboxCodec};
use imap_codec::types::response::{Capability, Code, Data, Response as ImapRes, Status};
use imap_codec::types::sequence::SequenceSet;

use crate::mailbox::Mailbox;
use crate::session;

pub struct Command<'a> {
    tag: Tag,
    session: &'a mut session::Instance,
}

// @FIXME better handle errors, our conversions are bad due to my fork of BàL
// @FIXME store the IMAP state in the session as an enum.
impl<'a> Command<'a> {
    pub fn new(tag: Tag, session: &'a mut session::Instance) -> Self {
        Self { tag, session }
    }





}
