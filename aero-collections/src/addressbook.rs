use aero_user::login::Credentials;

use crate::dav::namespace::DavNs;

pub(crate) const CARD_PREFIX: &str = "addressbook";
pub(crate) const MAIN_CARD: &str = "Personal";

#[derive(Clone)]
pub struct AddressbookNs {
    pub dav: DavNs,
}

impl AddressbookNs {
    /// Create a new addressbook namespace
    pub fn new(creds: Credentials) -> Self {
        Self { dav: DavNs::new(creds, CARD_PREFIX, &[MAIN_CARD]) }
    }
}
