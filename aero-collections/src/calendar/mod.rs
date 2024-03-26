pub mod namespace;

use anyhow::Result;

use aero_user::login::Credentials;

use crate::unique_ident::*;

pub struct Calendar {
    a: u64,
}

impl Calendar {
    pub(crate) async fn open(
        creds: &Credentials,
        id: UniqueIdent,
    ) -> Result<Self> {
        todo!();
    }
}
