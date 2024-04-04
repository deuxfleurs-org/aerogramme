pub mod namespace;

use anyhow::Result;
use tokio::sync::RwLock;

use aero_bayou::Bayou;
use aero_user::login::Credentials;
use aero_user::cryptoblob::{self, gen_key, open_deserialize, seal_serialize, Key};
use aero_user::storage::{self, BlobRef, BlobVal, RowRef, RowVal, Selector, Store};

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

    /// Sync data with backing store
    pub async fn force_sync(&self) -> Result<()> {
        self.internal.write().await.force_sync().await
    }

    /// Sync data with backing store only if changes are detected
    /// or last sync is too old
    pub async fn opportunistic_sync(&self) -> Result<()> {
        self.internal.write().await.opportunistic_sync().await
    }

    pub async fn get(&self, blob_id: UniqueIdent, message_key: &Key) -> Result<Vec<u8>> {
        self.internal.read().await.get(blob_id, message_key).await
    }

    pub async fn diff(&self, sync_token: Token) -> Result<(Token, Vec<SyncChange>)> {
        self.internal.read().await.diff(sync_token).await
    }

    pub async fn put<'a>(&self, entry: IndexEntry, evt: &'a [u8]) -> Result<Token> {
        self.internal.write().await.put(entry, evt).await
    }

    pub async fn delete(&self, blob_id: UniqueIdent) -> Result<Token> {
        self.internal.write().await.delete(blob_id).await
    }
}

struct CalendarInternal {
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

    async fn get(&self, blob_id: BlobId, message_key: &Key) -> Result<Vec<u8>> {
        todo!()
    }

    async fn put<'a>(&mut self, entry: IndexEntry, evt: &'a [u8]) -> Result<Token> {
        //@TODO write event to S3
        //@TODO add entry into Bayou
        todo!();
    }

    async fn delete(&mut self, blob_id: BlobId) -> Result<Token> {
        todo!();
    }

    async fn diff(&self, sync_token: Token) -> Result<(Token, Vec<SyncChange>)> {
        todo!();
    }
}
