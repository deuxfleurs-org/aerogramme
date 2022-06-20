

use crate::login::Credentials;
use crate::mailbox::Mailbox;

pub struct User {
    pub name: String,
    pub creds: Credentials,
}

pub enum State {
    NotAuthenticated,
    Authenticated(User),
    Selected(User, Mailbox),
    Logout
}
pub enum Error {
    ForbiddenTransition,
}

// See RFC3501 section 3.
// https://datatracker.ietf.org/doc/html/rfc3501#page-13
impl State {
    pub fn authenticate(&mut self, user: User) -> Result<(), Error> {
        self = match self {
            State::NotAuthenticated => State::Authenticated(user),
            _ => return Err(Error::ForbiddenTransition),
        };
        Ok(())
    }

    pub fn logout(&mut self) -> Self {
        self = State::Logout;
        Ok(())
    }

    pub fn select(&mut self, mailbox: Mailbox) -> Result<(), Error> {
        self = match self {
            State::Authenticated(user) => State::Selected(user, mailbox),
            _ => return Err(Error::ForbiddenTransition),
        };
        Ok(())
    }

    pub fn unselect(&mut self) -> Result<(), Error> {
        self = match self {
            State::Selected(user, _) => State::Authenticated(user),
            _ => return Err(Error::ForbiddenTransition),
        };
        Ok(())
    }
}

