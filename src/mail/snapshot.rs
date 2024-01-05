use std::sync::Arc;
use super::mailbox::Mailbox;
use super::uidindex::UidIndex;

pub struct Snapshot {
    pub mailbox: Arc<Mailbox>,
    pub snapshot: UidIndex,
}

impl Snapshot {
}
