use anyhow::{bail, Result};
use std::collections::HashMap;
use std::sync::Arc;

use aero_user::login::Credentials;
use aero_user::storage;

use crate::dav::collection::{Collection, CollectionWeak};
use crate::ident_list::{CreatedResult, IdentList};
use crate::unique_ident::UniqueIdent;

pub(crate) const MAX_COLNAME_CHARS: usize = 32;

/// A restricted WebDAV namespace, containing collections of object resources.
#[derive(Clone)]
pub struct DavNs {
    creds: Credentials,
    prefix: String,
    default_collections: Vec<String>,
    collections: Arc<std::sync::Mutex<HashMap<UniqueIdent, CollectionWeak>>>,
}

impl DavNs {
    /// Create a new DAV namespace.
    ///
    /// `prefix` is used as prefix for primary keys in storage (it must be
    /// unique wrt other namespaces in the same bucket). `default_collections`
    /// specify collection names that will be automatically created in the
    /// namespace and which cannot be deleted.
    pub fn new(creds: Credentials, prefix: &str, default_collections: &[&str]) -> Self {
        Self {
            creds,
            prefix: prefix.to_string(),
            default_collections: default_collections.iter().map(|s| s.to_string()).collect(),
            collections: Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }

    /// Open a collection by name
    pub async fn open(&self, name: &str) -> Result<Option<Collection>> {
        let (list, _ct) = self.load_collection_list().await?;

        match list.get(name) {
            None => Ok(None),
            Some(ident) => Ok(Some(self.open_by_id(ident).await?)),
        }
    }

    /// Open a collection by unique id
    /// Check mail::namespace::open_mailbox_by_id to understand this function
    pub async fn open_by_id(&self, id: UniqueIdent) -> Result<Collection> {
        if let Some(mut col) = {
            let cache = self.collections.lock().unwrap();
            cache.get(&id)
                 .and_then(|col_weak| col_weak.upgrade())
        } {
            // Sync now to get a recent collection state
            col.sync().await?;
            return Ok(col)
        }

        let col = Collection::open(&self.creds, &self.prefix, id).await?;

        let mut cache = self.collections.lock().unwrap();
        if let Some(concurrent_col_weak) = cache.get(&id) {
            if let Some(concurrent_col) = concurrent_col_weak.upgrade() {
                drop(col); // we worked for nothing but at least we didn't starve someone else
                return Ok(concurrent_col);
            }
        }

        cache.insert(id, col.downgrade());
        Ok(col)
    }

    /// List collections
    pub async fn list(&self) -> Result<Vec<String>> {
        self.load_collection_list()
            .await
            .map(|(list, _)| list.names())
    }

    /// Delete a collection from the index
    pub async fn delete(&self, name: &str) -> Result<()> {
        // We prevent deleting default collections
        if let Some(n) = self.is_default_collection(name) {
            bail!("Cannot delete the default collection {}", n);
        }

        let (mut list, ct) = self.load_collection_list().await?;
        if list.has(name) {
            //@TODO: actually delete collection content
            list.set(name, None);
            self.save_collection_list(&list, ct).await?;
            Ok(())
        } else {
            bail!("Collection {} does not exist", name);
        }
    }

    /// Rename a collection in the index
    pub async fn rename(&self, old: &str, new: &str) -> Result<()> {
        if let Some(n) = self.is_default_collection(old) {
            bail!("Renaming default collection {} is not supported currently", n);
        }
        if !new.chars().all(char::is_alphanumeric) {
            bail!("Unsupported characters in new collection name, only alphanumeric characters are allowed currently");
        }
        if new.len() > MAX_COLNAME_CHARS {
            bail!("Collection name can't contain more than 32 characters");
        }

        let (mut list, ct) = self.load_collection_list().await?;
        list.rename(old, new)?;
        self.save_collection_list(&list, ct).await?;

        Ok(())
    }
   
    /// Create collection
    pub async fn create(&self, name: &str) -> Result<()> {
        if let Some(n) = self.is_default_collection(name) {
            bail!("Default collection {} is automatically created, can't create it manually", n);
        }
        if !name.chars().all(char::is_alphanumeric) {
            bail!("Unsupported characters in new collection name, only alphanumeric characters are allowed");
        }
        if name.len() > MAX_COLNAME_CHARS {
            bail!("Collection name can't contain more than 32 characters");
        }

        let (mut list, ct) = self.load_collection_list().await?;
        match list.create(name) {
            CreatedResult::Existed(_) => bail!("Collection {} already exists", name),
            CreatedResult::Created(_) => (),
        }
        self.save_collection_list(&list, ct).await?;

        Ok(())
    }

    /// Has collection
    pub async fn has(&self, name: &str) -> Result<bool> {
        self.load_collection_list()
            .await
            .map(|(list, _)| list.has(name))
    }
    
    fn is_default_collection(&self, name: &str) -> Option<&str> {
        self.default_collections.iter().find(|n| *n == name).map(|x| x.as_str())
    }

    // --- internal collction list management ----

    /// Load from storage
    async fn load_collection_list(&self) -> Result<(IdentList, Option<storage::RowRef>)> {
        let (mut list, row) = IdentList::load_from_storage(&self.creds, &self.prefix, "list").await?;

        // Create default collections
        let is_default_col_missing = self.default_collections
            .iter()
            .map(|colname| list.create(colname))
            .fold(false, |acc, r| {
                acc || matches!(r, CreatedResult::Created(..))
            });

        // Save the index if we created a new collection
        if is_default_col_missing {
            self.save_collection_list(&list, row.clone()).await?;
        }

        Ok((list, row))
    }

    /// Save an updated index
    async fn save_collection_list(
        &self,
        list: &IdentList,
        ct: Option<storage::RowRef>,
    ) -> Result<()> {
        list.store_to_storage(&self.creds, &self.prefix, "list", ct).await
    }
}
