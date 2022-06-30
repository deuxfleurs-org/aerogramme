use std::error::Error as StdError;
use std::fmt;
use std::sync::Arc;

use crate::imap::mailbox_view::MailboxView;
use crate::mail::user::User;

#[derive(Debug)]
pub enum Error {
    ForbiddenTransition,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Forbidden Transition")
    }
}
impl StdError for Error {}

pub enum State {
    NotAuthenticated,
    Authenticated(Arc<User>),
    Selected(Arc<User>, MailboxView),
    // Examined is like Selected, but indicates that the mailbox is read-only
    Examined(Arc<User>, MailboxView),
    Logout,
}

pub enum Transition {
    None,
    Authenticate(Arc<User>),
    Examine(MailboxView),
    Select(MailboxView),
    Unselect,
    Logout,
}

// See RFC3501 section 3.
// https://datatracker.ietf.org/doc/html/rfc3501#page-13
impl State {
    pub fn apply(self, tr: Transition) -> Result<Self, Error> {
        match (self, tr) {
            (s, Transition::None) => Ok(s),
            (State::NotAuthenticated, Transition::Authenticate(u)) => Ok(State::Authenticated(u)),
            (State::Authenticated(u), Transition::Select(m)) => Ok(State::Selected(u, m)),
            (State::Authenticated(u), Transition::Examine(m)) => Ok(State::Examined(u, m)),
            (State::Selected(u, _), Transition::Unselect) => Ok(State::Authenticated(u)),
            (State::Examined(u, _), Transition::Unselect) => Ok(State::Authenticated(u)),
            (_, Transition::Logout) => Ok(State::Logout),
            _ => Err(Error::ForbiddenTransition),
        }
    }
}
