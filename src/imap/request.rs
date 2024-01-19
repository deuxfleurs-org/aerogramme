use imap_codec::imap_types::command::Command;

#[derive(Debug)]
pub enum Request {
    ImapCommand(Command<'static>),
    Idle,
}
