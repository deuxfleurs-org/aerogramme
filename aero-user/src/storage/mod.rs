/*
 *
 * This abstraction goal is to leverage all the semantic of Garage K2V+S3,
 * to be as tailored as possible to it ; it aims to be a zero-cost abstraction
 * compared to when we where directly using the K2V+S3 client.
 *
 * My idea: we can encapsulate the causality token
 * into the object system so it is not exposed.
 */

pub mod garage;
pub mod in_memory;

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;

use async_trait::async_trait;

#[derive(Debug, Clone)]
pub enum Alternative {
    Tombstone,
    Value(Vec<u8>),
}
type ConcurrentValues = Vec<Alternative>;

#[derive(Debug, Clone)]
pub enum StorageError {
    NotFound,
    Internal,
}
impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Storage Error: ")?;
        match self {
            Self::NotFound => f.write_str("Item not found"),
            Self::Internal => f.write_str("An internal error occured"),
        }
    }
}
impl std::error::Error for StorageError {}

#[derive(Debug, Clone, PartialEq)]
pub struct RowUid {
    pub shard: String,
    pub sort: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RowRef {
    pub uid: RowUid,
    pub causality: Option<String>,
}
impl std::fmt::Display for RowRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "RowRef({}, {}, {:?})",
            self.uid.shard, self.uid.sort, self.causality
        )
    }
}

impl RowRef {
    pub fn new(shard: &str, sort: &str) -> Self {
        Self {
            uid: RowUid {
                shard: shard.to_string(),
                sort: sort.to_string(),
            },
            causality: None,
        }
    }
    pub fn with_causality(mut self, causality: String) -> Self {
        self.causality = Some(causality);
        self
    }
}

#[derive(Debug, Clone)]
pub struct RowVal {
    pub row_ref: RowRef,
    pub value: Alternative,
}

impl RowVal {
    pub fn new(row_ref: RowRef, value: Vec<u8>) -> Self {
        Self {
            row_ref,
            value: Alternative::Value(value),
        }
    }

    pub fn deleted(row_ref: RowRef) -> Self {
        Self {
            row_ref,
            value: Alternative::Tombstone,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConcurrentRowVal {
    pub row_ref: RowRef,
    pub value: ConcurrentValues,
}

impl ConcurrentRowVal {
    pub fn new(row_ref: RowRef, value: Vec<u8>) -> Self {
        Self {
            row_ref,
            value: vec![Alternative::Value(value)],
        }
    }
}

#[derive(Debug, Clone)]
pub struct BlobRef(pub String);
impl std::fmt::Display for BlobRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BlobRef({})", self.0)
    }
}

#[derive(Debug, Clone)]
pub struct BlobVal {
    pub blob_ref: BlobRef,
    pub meta: HashMap<String, String>,
    pub value: Vec<u8>,
}
impl BlobVal {
    pub fn new(blob_ref: BlobRef, value: Vec<u8>) -> Self {
        Self {
            blob_ref,
            value,
            meta: HashMap::new(),
        }
    }

    pub fn with_meta(mut self, k: String, v: String) -> Self {
        self.meta.insert(k, v);
        self
    }
}

#[derive(Debug)]
pub enum Selector<'a> {
    Range {
        shard: &'a str,
        sort_begin: Option<&'a str>,
        sort_end: Option<&'a str>,
    },
    List {
        shard: &'a str,
        sort_list: &'a[&'a str],
    },
    #[allow(dead_code)]
    Prefix {
        shard: &'a str,
        sort_prefix: &'a str,
    },
    Single {
        shard: &'a str,
        sort: &'a str,
    },
}
impl<'a> std::fmt::Display for Selector<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Range {
                shard,
                sort_begin,
                sort_end,
            } => {
                write!(f, "Range({}, ", shard)?;
                if let Some(begin) = sort_begin {
                    write!(f, "{}", begin)?;
                }
                write!(f, "..")?;
                if let Some(end) = sort_end {
                    write!(f, "{}", end)?;
                };
                write!(f, ")")
            },
            Self::List { shard, sort_list } => write!(f, "List({}, {:?})", shard, sort_list),
            Self::Prefix { shard, sort_prefix } => write!(f, "Prefix({}, {})", shard, sort_prefix),
            Self::Single { shard, sort } => write!(f, "Single({}, {})", shard, sort),
        }
    }
}

#[async_trait]
pub trait IStore {
    /// Read a single value. Fails with `StorageError::NotFound` if there is no
    /// value for this key in the store.
    async fn row_fetch(&self, shard: &str, sort: &str) -> Result<ConcurrentRowVal, StorageError>;

    /// Read a batch of values. This function never fails with
    /// `StorageError::NotFound`. Instead, keys that are not in the store
    /// (especially for `Selector::Single` and `Selector::List`) are simply
    /// ignored.
    async fn row_fetch_batch<'a>(&self, select: &Selector<'a>) -> Result<Vec<ConcurrentRowVal>, StorageError>;

    /// Delete a batch of values. This function does not takes causality
    /// information as input. Instead, it internally inserts deletion markers
    /// that supersede all of the current versions of all selected keys.
    async fn row_delete_batch<'a>(&self, select: &Selector<'a>) -> Result<(), StorageError>;

    /// Inserts or deletes a number of values.
    /// This takes causality information contained in `values` into account.
    /// - Inserting a new value or updating an existing one is done by using a
    /// `RowVal` created using `RowVal::new`.
    /// - Deleting a value is done by using a `RowVal` created using `RowVal::deleted`.
    async fn row_update(&self, values: Vec<RowVal>) -> Result<(), StorageError>;

    /// Polls a single key, waiting for a new value. The key does not need to
    /// currently exist in the store. If `row_ref` does not contain causality
    /// information, the current value (if any) is returned immediately.
    /// Otherwise `row_poll` waits until a new value is written.
    async fn row_poll(&self, row_ref: &RowRef) -> Result<ConcurrentRowVal, StorageError>;

    async fn blob_fetch(&self, blob_ref: &BlobRef) -> Result<BlobVal, StorageError>;
    async fn blob_insert(&self, blob_val: BlobVal) -> Result<String, StorageError>;
    async fn blob_copy(&self, src: &BlobRef, dst: &BlobRef) -> Result<(), StorageError>;
    async fn blob_list(&self, prefix: &str) -> Result<Vec<BlobRef>, StorageError>;
    async fn blob_rm(&self, blob_ref: &BlobRef) -> Result<(), StorageError>;
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct UnicityBuffer(Vec<u8>);

#[async_trait]
pub trait IBuilder: std::fmt::Debug {
    async fn build(&self) -> Result<Store, StorageError>;

    /// Returns an opaque buffer that uniquely identifies this builder
    fn unique(&self) -> UnicityBuffer;
}

pub type Builder = Arc<dyn IBuilder + Send + Sync>;
pub type Store = Box<dyn IStore + Send + Sync>;
