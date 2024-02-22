use super::mailbox::MailMeta;
use super::snapshot::FrozenMailbox;
use super::unique_ident::UniqueIdent;
use anyhow::Result;
use futures::stream::{Stream, StreamExt, BoxStream};
use futures::future::FutureExt;

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

//type QueryResultStream = Box<dyn Stream<Item = Result<QueryResult>>>;

impl<'a, 'b> Query<'a, 'b> {
    pub fn fetch(&self) -> BoxStream<Result<QueryResult>> {
        match self.scope {
            QueryScope::Index => Box::pin(futures::stream::iter(self.emails).map(|&uuid| Ok(QueryResult::IndexResult { uuid }))),
            QueryScope::Partial => Box::pin(self.partial()),
            QueryScope::Full => Box::pin(self.full()),
        }
    }

    // --- functions below are private *for reasons*
    fn partial<'d>(&'d self) -> impl Stream<Item = Result<QueryResult>> + 'd + Send {
        async move { 
            let maybe_meta_list: Result<Vec<MailMeta>> = self.frozen.mailbox.fetch_meta(self.emails).await;
            let list_res = maybe_meta_list
                .map(|meta_list| meta_list
                     .into_iter()
                     .zip(self.emails)
                     .map(|(metadata, &uuid)| Ok(QueryResult::PartialResult { uuid, metadata }))
                     .collect()
                    )
                .unwrap_or_else(|e| vec![Err(e)]);

            futures::stream::iter(list_res)
        }.flatten_stream()
    }

    fn full<'d>(&'d self) -> impl Stream<Item = Result<QueryResult>> + 'd + Send {
        self.partial()
            .then(move |maybe_meta| async move {
                let meta = maybe_meta?;

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
    }
}

#[derive(Debug, Clone)]
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

    fn into_full(self, content: Vec<u8>) -> Option<Self> {
        match self {
            Self::PartialResult { uuid, metadata } => Some(Self::FullResult {
                uuid,
                metadata,
                content,
            }),
            _ => None,
        }
    }
}
