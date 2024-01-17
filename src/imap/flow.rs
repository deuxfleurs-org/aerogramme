use std::error::Error as StdError;
use std::fmt;
use std::sync::Arc;
use tokio::sync::Notify;

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
    Selected(Arc<User>, MailboxView, MailboxPerm),
    Idle(Arc<User>, MailboxView, MailboxPerm, Arc<Notify>),
    Logout,
}

#[derive(Clone)]
pub enum MailboxPerm {
    ReadOnly,
    ReadWrite,
}

pub enum Transition {
    None,
    Authenticate(Arc<User>),
    Select(MailboxView, MailboxPerm),
    Idle(Notify),
    UnIdle,
    Unselect,
    Logout,
}

// See RFC3501 section 3.
// https://datatracker.ietf.org/doc/html/rfc3501#page-13
impl State {
    pub fn apply(&mut self, tr: Transition) -> Result<(), Error> {
        let new_state = match (std::mem::replace(self, State::NotAuthenticated), tr) {
            (_s, Transition::None) => return Ok(()),
            (State::NotAuthenticated, Transition::Authenticate(u)) => State::Authenticated(u),
            (
                State::Authenticated(u) | State::Selected(u, _, _),
                Transition::Select(m, p),
            ) => State::Selected(u, m, p),
            (State::Selected(u, _, _) , Transition::Unselect) => {
                State::Authenticated(u.clone())
            }
            (State::Selected(u, m, p), Transition::Idle(s)) => {
                State::Idle(u, m, p, Arc::new(s))
            },
            (State::Idle(u, m, p, _), Transition::UnIdle) => {
                State::Selected(u, m, p)
            },
            (_, Transition::Logout) => State::Logout,
            _ => return Err(Error::ForbiddenTransition),
        };

        *self = new_state;

        Ok(())
    }
}
