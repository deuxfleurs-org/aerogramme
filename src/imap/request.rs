use imap_codec::imap_types::command::Command;
use imap_codec::imap_types::core::Tag;

#[derive(Debug)]
pub enum Request {
    ImapCommand(Command<'static>),
    IdleStart(Tag<'static>),
    IdlePoll,
}
