use anyhow::{Result, anyhow};
use super::mailbox::MailMeta;
use super::snapshot::FrozenMailbox;
use super::unique_ident::UniqueIdent;
use super::uidindex::IndexEntry;
use futures::stream::{FuturesUnordered, StreamExt};

/// Query is in charge of fetching efficiently
/// requested data for a list of emails
pub struct Query<'a,'b> {
    pub frozen: &'a FrozenMailbox,
    pub emails: &'b [UniqueIdent],
}

impl<'a,'b> Query<'a,'b> {
    pub fn index(&self) -> Result<Vec<IndexResult>> {
        self
            .emails
            .iter()
            .map(|uuid| {
                self
                    .frozen
                    .snapshot
                    .table
                    .get(uuid)
                    .map(|index| IndexResult { uuid: *uuid, index })
                    .ok_or(anyhow!("missing email in index"))
            })
            .collect::<Result<Vec<_>, _>>()
    }

    pub async fn partial(&self) -> Result<Vec<PartialResult>> {
        let meta = self.frozen.mailbox.fetch_meta(self.emails).await?;
        let result = meta
            .into_iter()
            .zip(self.index()?)
            .map(|(metadata, index)| PartialResult { uuid: index.uuid, index: index.index, metadata })
            .collect::<Vec<_>>();
        Ok(result)
    }

    /// @FIXME WARNING: THIS CAN ALLOCATE A LOT OF MEMORY
    /// AND GENERATE SO MUCH NETWORK TRAFFIC.
    /// THIS FUNCTION SHOULD BE REWRITTEN, FOR EXAMPLE WITH
    /// SOMETHING LIKE AN ITERATOR
    pub async fn full(&self) -> Result<Vec<FullResult>> {
        let meta_list = self.partial().await?;
        meta_list
            .into_iter()
            .map(|meta| async move  {
                let content = self.frozen.mailbox.fetch_full(meta.uuid, &meta.metadata.message_key).await?;
                Ok(FullResult {
                    uuid: meta.uuid,
                    index: meta.index,
                    metadata: meta.metadata,
                    content,
                })
            })
            .collect::<FuturesUnordered<_>>()
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
    }
}

pub struct IndexResult<'a> {
    pub uuid: UniqueIdent,
    pub index: &'a IndexEntry,
}
pub struct PartialResult<'a> {
    pub uuid: UniqueIdent,
    pub index: &'a IndexEntry,
    pub metadata: MailMeta,
}
pub struct FullResult<'a> {
    pub uuid: UniqueIdent,
    pub index: &'a IndexEntry,
    pub metadata: MailMeta,
    pub content: Vec<u8>,
}
