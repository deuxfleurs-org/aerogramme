use std::error::Error as StdError;
use std::fmt;
use std::sync::Arc;
use tokio::sync::Notify;

use imap_codec::imap_types::core::Tag;
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
    Idle(Arc<User>, MailboxView, MailboxPerm, Tag<'static>, Arc<Notify>),
    Logout,
}
impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use State::*;
        match self {
            NotAuthenticated => write!(f, "NotAuthenticated"),
            Authenticated(..) => write!(f, "Authenticated"),
            Selected(..) => write!(f, "Selected"),
            Idle(..) => write!(f, "Idle"),
            Logout => write!(f, "Logout"),
        }
    }
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
    Idle(Tag<'static>, Notify),
    UnIdle,
    Unselect,
    Logout,
}
impl fmt::Display for Transition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Transition::*;
        match self {
            None => write!(f, "None"),
            Authenticate(..) => write!(f, "Authenticated"),
            Select(..) => write!(f, "Selected"),
            Idle(..) => write!(f, "Idle"),
            UnIdle => write!(f, "UnIdle"),
            Unselect => write!(f, "Unselect"),
            Logout => write!(f, "Logout"),
        }
    }
}

// See RFC3501 section 3.
// https://datatracker.ietf.org/doc/html/rfc3501#page-13
impl State {
    pub fn apply(&mut self, tr: Transition) -> Result<(), Error> {
        tracing::debug!(state=%self, transition=%tr, "try change state");

        let new_state = match (std::mem::replace(self, State::Logout), tr) {
            (s, Transition::None) => s,
            (State::NotAuthenticated, Transition::Authenticate(u)) => State::Authenticated(u),
            (
                State::Authenticated(u) | State::Selected(u, _, _),
                Transition::Select(m, p),
            ) => State::Selected(u, m, p),
            (State::Selected(u, _, _) , Transition::Unselect) => {
                State::Authenticated(u.clone())
            }
            (State::Selected(u, m, p), Transition::Idle(t, s)) => {
                State::Idle(u, m, p, t, Arc::new(s))
            },
            (State::Idle(u, m, p, _, _), Transition::UnIdle) => {
                State::Selected(u, m, p)
            },
            (_, Transition::Logout) => State::Logout,
            (s, t) => {
                tracing::error!(state=%s, transition=%t, "forbidden transition");
                return Err(Error::ForbiddenTransition)
            }
        };
        *self = new_state;
        tracing::debug!(state=%self, "transition succeeded");

        Ok(())
    }
}
