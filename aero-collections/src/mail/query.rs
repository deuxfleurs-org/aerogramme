use super::mailbox::{MailMeta, MailMetaResult, Mailbox};
use crate::unique_ident::UniqueIdent;
use anyhow::Result;
use futures::future::FutureExt;
use futures::stream::{BoxStream, Stream, StreamExt};

/// Query is in charge of fetching efficiently
/// requested data for a list of emails
pub struct Query {
    pub mailbox: Mailbox,
    pub emails: Vec<UniqueIdent>,
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

impl Query {
    pub fn fetch(&self) -> BoxStream<'_, Result<QueryResult>> {
        match self.scope {
            QueryScope::Index => Box::pin(
                futures::stream::iter(&self.emails)
                    .map(|&uuid| Ok(QueryResult::MailData(MailData::Index { uuid }))),
            ),
            QueryScope::Partial => Box::pin(self.partial()),
            QueryScope::Full => Box::pin(self.full()),
        }
    }

    // --- functions below are private *for reasons*
    fn partial<'d>(&'d self) -> impl Stream<Item = Result<QueryResult>> + 'd + Send {
        async move {
            let maybe_meta_list: Result<Vec<MailMetaResult>> =
                self.mailbox.fetch_meta(&self.emails).await;
            let list_res = maybe_meta_list
                .map(|meta_list| {
                    meta_list
                        .into_iter()
                        .zip(&self.emails)
                        .map(|(metadata, &uuid)| {
                            match metadata {
                                MailMetaResult::NotFound => Ok(QueryResult::NotFound),
                                MailMetaResult::MailMeta(metadata) =>
                                    Ok(QueryResult::MailData(MailData::Partial { uuid, metadata }))
                            }
                        })
                     .collect()
                })
                .unwrap_or_else(|e| vec![Err(e)]);

            futures::stream::iter(list_res)
        }
        .flatten_stream()
    }

    fn full<'d>(&'d self) -> impl Stream<Item = Result<QueryResult>> + 'd + Send {
        self.partial().then(move |maybe_meta| async move {
            let meta = maybe_meta?;
            
            match meta {
                QueryResult::NotFound => Ok(QueryResult::NotFound),
                QueryResult::MailData(meta) => {
                    let content = self
                        .mailbox
                        .fetch_full(
                            *meta.uuid(),
                            &meta
                                .metadata()
                                .expect("meta to be PartialResult")
                                .message_key,
                        )
                        .await?;
                    match content {
                        None => Ok(QueryResult::NotFound),
                        Some(content) =>
                            Ok(QueryResult::MailData(
                                meta.into_full(content).expect("meta to be PartialResult")
                            ))
                    }
                }
            }
        })
    }
}

#[derive(Debug, Clone)]
pub enum QueryResult {
    /// Successfully fetched the required mail data.
    MailData(MailData),
    /// The required mail data was not found (this indicates that the index
    /// referencing this email is outdated and that the email has since then
    /// been deleted in storage).
    NotFound,
}
    
#[derive(Debug, Clone)]
pub enum MailData {
    Index {
        uuid: UniqueIdent,
    },
    Partial {
        uuid: UniqueIdent,
        metadata: MailMeta,
    },
    Full {
        uuid: UniqueIdent,
        metadata: MailMeta,
        content: Vec<u8>,
    },
}
impl MailData {
    pub fn uuid(&self) -> &UniqueIdent {
        match self {
            Self::Index { uuid, .. } => uuid,
            Self::Partial { uuid, .. } => uuid,
            Self::Full { uuid, .. } => uuid,
        }
    }

    pub fn metadata(&self) -> Option<&MailMeta> {
        match self {
            Self::Index { .. } => None,
            Self::Partial { metadata, .. } => Some(metadata),
            Self::Full { metadata, .. } => Some(metadata),
        }
    }

    #[allow(dead_code)]
    pub fn content(&self) -> Option<&[u8]> {
        match self {
            Self::Full { content, .. } => Some(content),
            _ => None,
        }
    }

    fn into_full(self, content: Vec<u8>) -> Option<Self> {
        match self {
            Self::Partial { uuid, metadata } => Some(Self::Full {
                uuid,
                metadata,
                content,
            }),
            _ => None,
        }
    }
}
