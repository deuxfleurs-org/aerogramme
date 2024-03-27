pub mod namespace;

use anyhow::Result;
use tokio::sync::RwLock;

use aero_bayou::Bayou;
use aero_user::login::Credentials;
use aero_user::cryptoblob::{self, gen_key, open_deserialize, seal_serialize, Key};
use aero_user::storage::{self, BlobRef, BlobVal, RowRef, RowVal, Selector, Store};

use crate::unique_ident::*;
use crate::davdag::DavDag;

pub struct Calendar {
    pub(super) id: UniqueIdent,
    internal: RwLock<CalendarInternal>,
}

impl Calendar {
    pub(crate) async fn open(
        creds: &Credentials,
        id: UniqueIdent,
    ) -> Result<Self> {
        todo!();
    }
}

struct CalendarInternal {
    id: UniqueIdent,
    cal_path: String,
    encryption_key: Key,
    storage: Store,
    uid_index: Bayou<DavDag>,
}
