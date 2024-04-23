pub mod namespace;

use anyhow::{anyhow, bail, Result};
use tokio::sync::RwLock;

use aero_bayou::Bayou;
use aero_user::login::Credentials;
use aero_user::cryptoblob::{self, gen_key, Key};
use aero_user::storage::{self, BlobRef, BlobVal, Store};

use crate::unique_ident::*;
use crate::davdag::{DavDag, IndexEntry, Token, BlobId, SyncChange};

pub struct Calendar {
    pub(super) id: UniqueIdent,
    internal: RwLock<CalendarInternal>,
}

impl Calendar {
    pub(crate) async fn open(
        creds: &Credentials,
        id: UniqueIdent,
        ) -> Result<Self> {
        let bayou_path = format!("calendar/dag/{}", id);
        let cal_path = format!("calendar/events/{}", id);

        let mut davdag = Bayou::<DavDag>::new(creds, bayou_path).await?;
        davdag.sync().await?;

        let internal = RwLock::new(CalendarInternal {
            id,
            encryption_key: creds.keys.master.clone(),
            storage: creds.storage.build().await?,
            davdag,
            cal_path,
        });

        Ok(Self { id, internal })
    }

    // ---- DAG sync utilities

    /// Sync data with backing store
    pub async fn force_sync(&self) -> Result<()> {
        self.internal.write().await.force_sync().await
    }

    /// Sync data with backing store only if changes are detected
    /// or last sync is too old
    pub async fn opportunistic_sync(&self) -> Result<()> {
        self.internal.write().await.opportunistic_sync().await
    }

    // ---- Data API

    /// Access the DAG internal data (you can get the list of files for example)
    pub async fn dag(&self) -> DavDag {
        // Cloning is cheap
        self.internal.read().await.davdag.state().clone()
    }

    /// The diff API is a write API as we might need to push a merge node
    /// to get a new sync token
    pub async fn diff(&self, sync_token: Token) -> Result<(Token, Vec<SyncChange>)> {
        self.internal.write().await.diff(sync_token).await
    }

    /// Get a specific event
    pub async fn get(&self, evt_id: UniqueIdent) -> Result<Vec<u8>> {
        self.internal.read().await.get(evt_id).await
    }

    /// Put a specific event
    pub async fn put<'a>(&self, name: &str, evt: &'a [u8]) -> Result<(Token, IndexEntry)> {
        self.internal.write().await.put(name, evt).await
    }

    /// Delete a specific event
    pub async fn delete(&self, blob_id: UniqueIdent) -> Result<Token> {
        self.internal.write().await.delete(blob_id).await
    }
}

use base64::Engine;
const MESSAGE_KEY: &str = "message-key";
struct CalendarInternal {
    #[allow(dead_code)]
    id: UniqueIdent,
    cal_path: String,
    encryption_key: Key,
    storage: Store,
    davdag: Bayou<DavDag>,
}

impl CalendarInternal {
    async fn force_sync(&mut self) -> Result<()> {
        self.davdag.sync().await?;
        Ok(())
    }

    async fn opportunistic_sync(&mut self) -> Result<()> {
        self.davdag.opportunistic_sync().await?;
        Ok(())
    }

    async fn get(&self, blob_id: BlobId) -> Result<Vec<u8>> {
        // Fetch message from S3
        let blob_ref = storage::BlobRef(format!("{}/{}", self.cal_path, blob_id));
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

    async fn put<'a>(&mut self, name: &str, evt: &'a [u8]) -> Result<(Token, IndexEntry)> {
        let message_key = gen_key();
        let blob_id = gen_ident();
        
        let encrypted_msg_key = cryptoblob::seal(&message_key.as_ref(), &self.encryption_key)?;
        let key_header = base64::engine::general_purpose::STANDARD.encode(&encrypted_msg_key);

        // Write event to S3
        let message_blob = cryptoblob::seal(evt, &message_key)?;
        let blob_val = BlobVal::new(
            BlobRef(format!("{}/{}", self.cal_path, blob_id)),
            message_blob,
        )
        .with_meta(MESSAGE_KEY.to_string(), key_header);

        let etag = self.storage
            .blob_insert(blob_val)
            .await?;

        // Add entry to Bayou
        let entry: IndexEntry = (blob_id, name.to_string(), etag);
        let davstate = self.davdag.state();
        let put_op = davstate.op_put(entry.clone());
        let token = put_op.token();
        self.davdag.push(put_op).await?;

        Ok((token, entry))
    }

    async fn delete(&mut self, blob_id: BlobId) -> Result<Token> {
        let davstate = self.davdag.state();

        if davstate.table.contains_key(&blob_id) {
            bail!("Cannot delete event that doesn't exist");
        }

        let del_op = davstate.op_delete(blob_id);
        let token = del_op.token();
        self.davdag.push(del_op).await?;

        let blob_ref = BlobRef(format!("{}/{}", self.cal_path, blob_id));
        self.storage.blob_rm(&blob_ref).await?;

        Ok(token)
    }

    async fn diff(&mut self, sync_token: Token) -> Result<(Token, Vec<SyncChange>)> {
        let davstate = self.davdag.state();

        let token_changed = davstate.resolve(sync_token)?;
        let changes = token_changed
            .iter()
            .filter_map(|t: &Token| davstate.change.get(t))
            .map(|s| s.clone())
            .collect();

        let heads = davstate.heads_vec();
        let token = match heads.as_slice() {
            [ token ] => *token,
            _ => {
                let op_mg = davstate.op_merge();
                let token = op_mg.token();
                self.davdag.push(op_mg).await?;
                token
            }
        };

        Ok((token, changes))
    }
}
