use anyhow::{anyhow, bail, Result};
use base64::Engine;

use aero_bayou::{Bayou, BayouWeak};
use aero_user::cryptoblob::{self, gen_key, Key};
use aero_user::login::Credentials;
use aero_user::storage::{self, BlobRef, BlobVal, Store};

use crate::dav::davindex::{DavIndex, IndexEntry, SyncChange, Token};
use crate::unique_ident::*;

/// A "flat" DAV collection that contains object resources.
///
/// In WebDAV, collections can contain resources that are collections
/// themselves, leading to a nested tree structure. This is not supported here,
/// because we do not need it for CalDAV/CardDAV. A `Collection` thus
/// corresponds to the simpler non-nested case; it can be seen as a directory of
/// files.
#[derive(Clone)]
pub struct Collection {
    blobs_path: String,
    encryption_key: Key,
    storage: Store,
    davindex: Bayou<DavIndex>,
}

const MESSAGE_KEY: &str = "message-key";

impl Collection {
    pub(crate) async fn open(creds: &Credentials, prefix: &str, id: UniqueIdent) -> Result<Self> {
        let bayou_path = format!("{}/dag/{}", prefix, id);
        let blobs_path = format!("{}/events/{}", prefix, id);

        let mut davindex = Bayou::<DavIndex>::new(creds, bayou_path).await?;
        davindex.sync().await?;

        Ok(Self {
            encryption_key: creds.keys.master.clone(),
            storage: creds.storage.clone(),
            davindex,
            blobs_path,
        })
    }
    
    // ---- DAG sync utilities

    /// Sync data with backing store
    pub async fn sync(&mut self) -> Result<()> {
        self.davindex.sync().await
    }

    // ---- Data API

    /// Access the index (you can get the list of files for example)
    pub async fn index(&self) -> DavIndex {
        // Cloning is cheap
        self.davindex.state().clone()
    }

    /// Access the current token
    pub async fn token(&mut self) -> Result<Token> {
        let davstate = self.davindex.state();
        let heads = davstate.heads_vec();
        let token = match heads.as_slice() {
            [token] => *token,
            _ => {
                let op_mg = davstate.op_merge();
                let token = op_mg.token();
                self.davindex.push(op_mg).await?;
                token
            }
        };
        Ok(token)
    }

    /// The diff API is a write API as we might need to push a merge node
    /// to get a new sync token
    pub async fn diff(&mut self, sync_token: Token) -> Result<(Token, Vec<SyncChange>)> {
        let davstate = self.davindex.state();

        let token_changed = davstate.resolve(sync_token)?;
        let changes = token_changed
            .iter()
            .filter_map(|t: &Token| davstate.change.get(t))
            .map(|s| s.clone())
            .filter(|s| match s {
                SyncChange::Ok((filename, _)) => davstate.idx_by_filename.get(filename).is_some(),
                SyncChange::NotFound(filename) => davstate.idx_by_filename.get(filename).is_none(),
            })
            .collect();

        let token = self.token().await?;
        Ok((token, changes))
    }

    /// Get a specific resource
    pub async fn get(&self, res_id: UniqueIdent) -> Result<Vec<u8>> {
        // Fetch message from S3
        let blob_ref = storage::BlobRef(format!("{}/{}", self.blobs_path, res_id));
        let object = self.storage.blob_fetch(&blob_ref).await?;

        // Decrypt message key from headers
        let key_encrypted_b64 = object
            .meta
            .get(MESSAGE_KEY)
            .ok_or(anyhow!("Missing key in metadata"))?;
        let key_encrypted = base64::engine::general_purpose::STANDARD.decode(key_encrypted_b64)?;
        let message_key_raw = cryptoblob::open(&key_encrypted, &self.encryption_key)?;
        let message_key =
            cryptoblob::Key::from_slice(&message_key_raw).ok_or(anyhow!("Invalid message key"))?;

        // Decrypt body
        let body = object.value;
        cryptoblob::open(&body, &message_key)
    }

    /// Put a specific resource
    pub async fn put<'a>(&mut self, name: &str, res: &'a [u8]) -> Result<(Token, IndexEntry)> {
        let message_key = gen_key();
        let blob_id = gen_ident();

        let encrypted_msg_key = cryptoblob::seal(&message_key.as_ref(), &self.encryption_key)?;
        let key_header = base64::engine::general_purpose::STANDARD.encode(&encrypted_msg_key);

        // Write event to S3
        let message_blob = cryptoblob::seal(res, &message_key)?;
        let blob_val = BlobVal::new(
            BlobRef(format!("{}/{}", self.blobs_path, blob_id)),
            message_blob,
        )
        .with_meta(MESSAGE_KEY.to_string(), key_header);

        let etag = self.storage.blob_insert(blob_val).await?;

        // Add entry to Bayou
        let entry: IndexEntry = (name.to_string(), etag);
        let davstate = self.davindex.state();
        let put_op = davstate.op_put(blob_id, entry.clone());
        let token = put_op.token();
        self.davindex.push(put_op).await?;

        Ok((token, entry))
    }

    /// Delete a specific resource
    pub async fn delete(&mut self, blob_id: UniqueIdent) -> Result<Token> {
        let davstate = self.davindex.state();

        if !davstate.table.contains_key(&blob_id) {
            bail!("Cannot delete event that doesn't exist");
        }

        let del_op = davstate.op_delete(blob_id);
        let token = del_op.token();
        self.davindex.push(del_op).await?;

        let blob_ref = BlobRef(format!("{}/{}", self.blobs_path, blob_id));
        self.storage.blob_rm(&blob_ref).await?;

        Ok(token)
    }

    pub fn downgrade(&self) -> CollectionWeak {
        CollectionWeak {
            blobs_path: self.blobs_path.clone(),
            encryption_key: self.encryption_key.clone(),
            storage: self.storage.clone(),
            davindex: self.davindex.downgrade(),
        }
    }
}

/// A "weak reference" to a collection.
///
/// `Collection`/`CollectionWeak` work similarly to `Arc`/`Weak`.
///
/// This is useful to reference the collection in a cache while allowing its
/// memory resources to be destroyed if it is not used elsewhere.
pub struct CollectionWeak {
    blobs_path: String,
    encryption_key: Key,
    storage: Store,
    davindex: BayouWeak<DavIndex>,
}

impl CollectionWeak {
    pub fn upgrade(&self) -> Option<Collection> {
        let davindex = self.davindex.upgrade()?;
        Some(Collection {
            blobs_path: self.blobs_path.clone(),
            encryption_key: self.encryption_key.clone(),
            storage: self.storage.clone(),
            davindex,
        })
    }
}
