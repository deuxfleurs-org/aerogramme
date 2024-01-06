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
    pub scope: QueryScope,
}

#[allow(dead_code)]
pub enum QueryScope {
    Index,
    Partial,
    Full,
}

impl<'a,'b> Query<'a,'b> {
    pub async fn fetch(&self) -> Result<Vec<QueryResult>> {
        match self.scope {
            QueryScope::Index => self.index(),
            QueryScope::Partial => self.partial().await,
            QueryScope::Full => self.full().await,
        }
    }

    // --- functions below are private *for reasons*

    fn index(&self) -> Result<Vec<QueryResult>> {
        self
            .emails
            .iter()
            .map(|uuid| {
                self
                    .frozen
                    .snapshot
                    .table
                    .get(uuid)
                    .map(|index| QueryResult::IndexResult { uuid: *uuid, index })
                    .ok_or(anyhow!("missing email in index"))
            })
            .collect::<Result<Vec<_>, _>>()
    }

    async fn partial(&self) -> Result<Vec<QueryResult>> {
        let meta = self.frozen.mailbox.fetch_meta(self.emails).await?;
        let result = meta
            .into_iter()
            .zip(self.index()?)
            .map(|(metadata, index)| index.into_partial(metadata).expect("index to be IndexResult"))
            .collect::<Vec<_>>();
        Ok(result)
    }

    /// @FIXME WARNING: THIS CAN ALLOCATE A LOT OF MEMORY
    /// AND GENERATE SO MUCH NETWORK TRAFFIC.
    /// THIS FUNCTION SHOULD BE REWRITTEN, FOR EXAMPLE WITH
    /// SOMETHING LIKE AN ITERATOR
    async fn full(&self) -> Result<Vec<QueryResult>> {
        let meta_list = self.partial().await?;
        meta_list
            .into_iter()
            .map(|meta| async move  {
                let content = self.frozen.mailbox.fetch_full(
                    *meta.uuid(), 
                    &meta.metadata().expect("meta to be PartialResult").message_key
                ).await?;

                Ok(meta.into_full(content).expect("meta to be PartialResult"))
            })
            .collect::<FuturesUnordered<_>>()
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
    }
}

pub enum QueryResult<'a> {
    IndexResult {
        uuid: UniqueIdent,
        index: &'a IndexEntry,
    },
    PartialResult {
        uuid: UniqueIdent,
        index: &'a IndexEntry,
        metadata: MailMeta,
    },
    FullResult {
        uuid: UniqueIdent,
        index: &'a IndexEntry,
        metadata: MailMeta,
        content: Vec<u8>,
    }
}
impl<'a> QueryResult<'a> {
    pub fn uuid(&self) -> &UniqueIdent {
        match self {
            Self::IndexResult { uuid, .. } => uuid,
            Self::PartialResult { uuid, .. } => uuid,
            Self::FullResult { uuid, .. } => uuid,
        }
    }

    #[allow(dead_code)]
    pub fn index(&self) -> &IndexEntry {
        match self {
            Self::IndexResult { index, .. } => index,
            Self::PartialResult { index, .. } => index,
            Self::FullResult { index, .. } => index,
        }
    }

    pub fn metadata(&'a self) -> Option<&'a MailMeta> {
        match self {
            Self::IndexResult { .. } => None,
            Self::PartialResult { metadata, .. } => Some(metadata),
            Self::FullResult { metadata, .. } => Some(metadata),
        }
    }

    #[allow(dead_code)]
    pub fn content(&'a self) -> Option<&'a [u8]> {
        match self {
            Self::FullResult { content, .. } => Some(content),
            _ => None,
        }
    }

    fn into_partial(self, metadata: MailMeta) -> Option<Self> {
        match self {
            Self::IndexResult { uuid, index } => Some(Self::PartialResult { uuid, index, metadata }),
            _ => None,
        }
    }

    fn into_full(self, content: Vec<u8>) -> Option<Self> {
        match self {
            Self::PartialResult { uuid, index, metadata } => Some(Self::FullResult { uuid, index, metadata, content }),
            _ => None,
        }
    }
}
