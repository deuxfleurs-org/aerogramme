use std::error::Error as StdError;
use std::fmt;

use crate::login::Credentials;
use crate::mail::Mailbox;

pub struct User {
    pub name: String,
    pub creds: Credentials,
}

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
    Authenticated(User),
    Selected(User, Mailbox),
    Logout,
}

pub enum Transition {
    No,
    Authenticate(User),
    Select(Mailbox),
    Unselect,
    Logout,
}

// See RFC3501 section 3.
// https://datatracker.ietf.org/doc/html/rfc3501#page-13
impl State {
    pub fn apply(self, tr: Transition) -> Result<Self, Error> {
        match (self, tr) {
            (s, Transition::No) => Ok(s),
            (State::NotAuthenticated, Transition::Authenticate(u)) => Ok(State::Authenticated(u)),
            (State::Authenticated(u), Transition::Select(m)) => Ok(State::Selected(u, m)),
            (State::Selected(u, _), Transition::Unselect) => Ok(State::Authenticated(u)),
            (_, Transition::Logout) => Ok(State::Logout),
            _ => Err(Error::ForbiddenTransition),
        }
    }
}
