use aero_user::login::Credentials;

use crate::dav::namespace::DavNs;

pub(crate) const CAL_PREFIX: &str = "calendar";
pub(crate) const MAIN_CAL: &str = "Personal";

#[derive(Clone)]
pub struct CalendarNs {
    pub dav: DavNs,
}

impl CalendarNs {
    /// Create a new calendar namespace
    pub fn new(creds: Credentials) -> Self {
        Self { dav: DavNs::new(creds, CAL_PREFIX, &[MAIN_CAL]) }
    }
}
