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
    pub fn apply(&mut self, tr: Transition) -> Result<(), Error> {
        let new_state = match (&self, tr) {
            (_s, Transition::None) => return Ok(()),
            (State::NotAuthenticated, Transition::Authenticate(u)) => State::Authenticated(u),
            (
                State::Authenticated(u) | State::Selected(u, _) | State::Examined(u, _),
                Transition::Select(m),
            ) => State::Selected(u.clone(), m),
            (
                State::Authenticated(u) | State::Selected(u, _) | State::Examined(u, _),
                Transition::Examine(m),
            ) => State::Examined(u.clone(), m),
            (State::Selected(u, _) | State::Examined(u, _), Transition::Unselect) => {
                State::Authenticated(u.clone())
            }
            (_, Transition::Logout) => State::Logout,
            _ => return Err(Error::ForbiddenTransition),
        };

        *self = new_state;

        Ok(())
    }
}
