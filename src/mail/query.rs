use super::mailbox::MailMeta;
use super::snapshot::FrozenMailbox;
use super::unique_ident::UniqueIdent;
use anyhow::Result;
use futures::stream::{FuturesOrdered, StreamExt};

/// Query is in charge of fetching efficiently
/// requested data for a list of emails
pub struct Query<'a, 'b> {
    pub frozen: &'a FrozenMailbox,
    pub emails: &'b [UniqueIdent],
    pub scope: QueryScope,
}

#[derive(Debug)]
pub enum QueryScope {
    Index,
    Partial,
    Full,
}
impl QueryScope {
    pub fn union(&self, other: &QueryScope) -> QueryScope {
        match (self, other) {
            (QueryScope::Full, _) | (_, QueryScope::Full) => QueryScope::Full,
            (QueryScope::Partial, _) | (_, QueryScope::Partial) => QueryScope::Partial,
            (QueryScope::Index, QueryScope::Index) => QueryScope::Index,
        }
    }
}

impl<'a, 'b> Query<'a, 'b> {
    pub async fn fetch(&self) -> Result<Vec<QueryResult>> {
        match self.scope {
            QueryScope::Index => Ok(self.emails.iter().map(|&uuid| QueryResult::IndexResult { uuid }).collect()),
            QueryScope::Partial =>self.partial().await,
            QueryScope::Full => self.full().await,
        }
    }

    // --- functions below are private *for reasons*

    async fn partial(&self) -> Result<Vec<QueryResult>> {
        let meta = self.frozen.mailbox.fetch_meta(self.emails).await?;
        let result = meta
            .into_iter()
            .zip(self.emails.iter())
            .map(|(metadata, &uuid)| {
                QueryResult::PartialResult  { uuid, metadata }
            })
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
            .map(|meta| async move {
                let content = self
                    .frozen
                    .mailbox
                    .fetch_full(
                        *meta.uuid(),
                        &meta
                            .metadata()
                            .expect("meta to be PartialResult")
                            .message_key,
                    )
                    .await?;

                Ok(meta.into_full(content).expect("meta to be PartialResult"))
            })
            .collect::<FuturesOrdered<_>>()
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
    }
}

#[derive(Debug)]
pub enum QueryResult {
    IndexResult {
        uuid: UniqueIdent,
    },
    PartialResult {
        uuid: UniqueIdent,
        metadata: MailMeta,
    },
    FullResult {
        uuid: UniqueIdent,
        metadata: MailMeta,
        content: Vec<u8>,
    },
}
impl QueryResult {
    pub fn uuid(&self) -> &UniqueIdent {
        match self {
            Self::IndexResult { uuid, .. } => uuid,
            Self::PartialResult { uuid, .. } => uuid,
            Self::FullResult { uuid, .. } => uuid,
        }
    }

    pub fn metadata(&self) -> Option<&MailMeta> {
        match self {
            Self::IndexResult { .. } => None,
            Self::PartialResult { metadata, .. } => Some(metadata),
            Self::FullResult { metadata, .. } => Some(metadata),
        }
    }

    #[allow(dead_code)]
    pub fn content(&self) -> Option<&[u8]> {
        match self {
            Self::FullResult { content, .. } => Some(content),
            _ => None,
        }
    }

    fn into_partial(self, metadata: MailMeta) -> Option<Self> {
        match self {
            Self::IndexResult { uuid } => Some(Self::PartialResult {
                uuid,
                metadata,
            }),
            _ => None,
        }
    }

    fn into_full(self, content: Vec<u8>) -> Option<Self> {
        match self {
            Self::PartialResult {
                uuid,
                metadata,
            } => Some(Self::FullResult {
                uuid,
                metadata,
                content,
            }),
            _ => None,
        }
    }
}
