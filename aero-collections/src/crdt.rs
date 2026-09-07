use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::future::Future;

use aero_user::cryptoblob::{open_deserialize, seal_serialize};
use aero_user::login::Credentials;
use aero_user::storage;

pub trait Crdt: Default + Serialize + for<'de> Deserialize<'de> + Sync {
    fn merge(&mut self, other: Self);

    fn load_from_storage(
        creds: &Credentials,
        pk: &str,
        sk: &str,
    ) -> impl Future<Output = Result<(Self, Option<storage::RowRef>)>> + Send {
        async move {
            match creds.storage.row_fetch(pk, sk).await {
                Err(storage::StorageError::NotFound) => Ok((Self::default(), None)),
                Err(e) => Err(e.into()),
                Ok(rv) => {
                    let mut crdt = Self::default();
                    let (row_ref, row_vals) = (rv.row_ref, rv.value);

                    for v in row_vals {
                        if let storage::Alternative::Value(vbytes) = v {
                            let crdt2 = open_deserialize::<Self>(&vbytes, &creds.keys.master)?;
                            crdt.merge(crdt2);
                        }
                    }
                    Ok((crdt, Some(row_ref)))
                }
            }
        }
    }

    fn store_to_storage(
        &self,
        creds: &Credentials,
        pk: &str,
        sk: &str,
        ct: Option<storage::RowRef>,
    ) -> impl Future<Output = Result<()>> + Send {
        async move {
            let list_blob = seal_serialize(self, &creds.keys.master)?;
            let rref = ct.unwrap_or(storage::RowRef::new(pk, sk));
            let row_val = storage::RowVal::new(rref, list_blob);
            creds.storage.row_update(vec![row_val]).await?;
            Ok(())
        }
    }
}
