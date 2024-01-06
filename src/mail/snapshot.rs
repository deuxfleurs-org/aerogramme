use std::sync::Arc;

use anyhow::Result;

use super::mailbox::Mailbox;
use super::query::{Query, QueryScope};
use super::uidindex::UidIndex;
use super::unique_ident::UniqueIdent;

/// A Frozen Mailbox has a snapshot of the current mailbox
/// state that is desynchronized with the real mailbox state.
/// It's up to the user to choose when their snapshot must be updated
/// to give useful information to their clients
///
///
pub struct FrozenMailbox {
    pub mailbox: Arc<Mailbox>,
    pub snapshot: UidIndex,
}

impl FrozenMailbox {
    /// Create a snapshot from a mailbox, the mailbox + the snapshot
    /// becomes the "Frozen Mailbox".
    pub async fn new(mailbox: Arc<Mailbox>) -> Self {
        let state = mailbox.current_uid_index().await;

        Self {
            mailbox,
            snapshot: state,
        }
    }

    /// Force the synchronization of the inner mailbox
    /// but do not update the local snapshot
    pub async fn sync(&self) -> Result<()> {
        self.mailbox.opportunistic_sync().await
    }

    /// Peek snapshot without updating the frozen mailbox
    /// Can be useful if you want to plan some writes
    /// while sending a diff to the client later
    pub async fn peek(&self) -> UidIndex {
        self.mailbox.current_uid_index().await
    }

    /// Update the FrozenMailbox local snapshot.
    /// Returns the old snapshot, so you can build a diff
    pub async fn update(&mut self) -> UidIndex {
        let old_snapshot = self.snapshot.clone();
        self.snapshot = self.mailbox.current_uid_index().await;

        old_snapshot
    }

    pub fn query<'a, 'b>(&'a self, uuids: &'b [UniqueIdent], scope: QueryScope) -> Query<'a, 'b> {
        Query {
            frozen: self,
            emails: uuids,
            scope,
        }
    }
}
