use imap_codec::imap_types::command::Command;
use tokio::sync::Notify;

#[derive(Debug)]
pub enum Request {
    ImapCommand(Command<'static>),
    IdleUntil(Notify),
}
