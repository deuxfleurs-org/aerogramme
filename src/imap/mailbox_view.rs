use std::sync::Arc;

use anyhow::{Error, Result};
use boitalettres::proto::{res::body::Data as Body, Request, Response};
use imap_codec::types::command::CommandBody;
use imap_codec::types::core::Atom;
use imap_codec::types::flag::Flag;
use imap_codec::types::mailbox::{ListMailbox, Mailbox as MailboxCodec};
use imap_codec::types::response::{Code, Data, Status};

use crate::mail::mailbox::{Mailbox, Summary};
use crate::mail::uidindex::UidIndex;

const DEFAULT_FLAGS: [Flag; 5] = [
    Flag::Seen,
    Flag::Answered,
    Flag::Flagged,
    Flag::Deleted,
    Flag::Draft,
];

/// A MailboxView is responsible for giving the client the information
/// it needs about a mailbox, such as an initial summary of the mailbox's
/// content and continuous updates indicating when the content
/// of the mailbox has been changed.
/// To do this, it keeps a variable `known_state` that corresponds to
/// what the client knows, and produces IMAP messages to be sent to the
/// client that go along updates to `known_state`.
pub struct MailboxView {
    mailbox: Arc<Mailbox>,
    known_state: UidIndex,
}

impl MailboxView {
    /// Creates a new IMAP view into a mailbox.
    /// Generates the necessary IMAP messages so that the client
    /// has a satisfactory summary of the current mailbox's state.
    pub async fn new(mailbox: Arc<Mailbox>) -> Result<(Self, Vec<Body>)> {
        let state = mailbox.current_uid_index().await;

        let new_view = Self {
            mailbox,
            known_state: state,
        };

        let mut data = Vec::<Body>::new();
        data.push(new_view.exists()?);
        data.push(new_view.recent()?);
        data.extend(new_view.flags()?.into_iter());
        data.push(new_view.uidvalidity()?);
        data.push(new_view.uidnext()?);
        if let Some(unseen) = new_view.unseen()? {
            data.push(unseen);
        }

        Ok((new_view, data))
    }

    // ----

    /// Produce an OK [UIDVALIDITY _] message corresponding to `known_state`
    fn uidvalidity(&self) -> Result<Body> {
        let uid_validity = Status::ok(
            None,
            Some(Code::UidValidity(self.known_state.uidvalidity)),
            "UIDs valid",
        )
        .map_err(Error::msg)?;
        Ok(Body::Status(uid_validity))
    }

    /// Produce an OK [UIDNEXT _] message corresponding to `known_state`
    fn uidnext(&self) -> Result<Body> {
        let next_uid = Status::ok(
            None,
            Some(Code::UidNext(self.known_state.uidnext)),
            "Predict next UID",
        )
        .map_err(Error::msg)?;
        Ok(Body::Status(next_uid))
    }

    /// Produces an UNSEEN message (if relevant) corresponding to the
    /// first unseen message id in `known_state`
    fn unseen(&self) -> Result<Option<Body>> {
        let unseen = self
            .known_state
            .idx_by_flag
            .get(&"$unseen".to_string())
            .and_then(|os| os.get_min())
            .cloned();
        if let Some(unseen) = unseen {
            let status_unseen =
                Status::ok(None, Some(Code::Unseen(unseen.clone())), "First unseen UID")
                    .map_err(Error::msg)?;
            Ok(Some(Body::Status(status_unseen)))
        } else {
            Ok(None)
        }
    }

    /// Produce an EXISTS message corresponding to the number of mails
    /// in `known_state`
    fn exists(&self) -> Result<Body> {
        let exists = u32::try_from(self.known_state.idx_by_uid.len())?;
        Ok(Body::Data(Data::Exists(exists)))
    }

    /// Produce a RECENT message corresponding to the number of
    /// recent mails in `known_state`
    fn recent(&self) -> Result<Body> {
        let recent = self
            .known_state
            .idx_by_flag
            .get(&"\\Recent".to_string())
            .map(|os| os.len())
            .unwrap_or(0);
        let recent = u32::try_from(recent)?;
        Ok(Body::Data(Data::Recent(recent)))
    }

    /// Produce a FLAGS and a PERMANENTFLAGS message that indicates
    /// the flags that are in `known_state` + default flags
    fn flags(&self) -> Result<Vec<Body>> {
        let mut flags: Vec<Flag> = self
            .known_state
            .idx_by_flag
            .flags()
            .map(|f| match f.chars().next() {
                Some('\\') => None,
                Some('$') if f == "$unseen" => None,
                Some(_) => match Atom::try_from(f.clone()) {
                    Err(_) => {
                        tracing::error!(flag=%f, "Unable to encode flag as IMAP atom");
                        None
                    }
                    Ok(a) => Some(Flag::Keyword(a)),
                },
                None => None,
            })
            .flatten()
            .collect();
        flags.extend_from_slice(&DEFAULT_FLAGS);
        let mut ret = vec![Body::Data(Data::Flags(flags.clone()))];

        flags.push(Flag::Permanent);
        let permanent_flags =
            Status::ok(None, Some(Code::PermanentFlags(flags)), "Flags permitted")
                .map_err(Error::msg)?;
        ret.push(Body::Status(permanent_flags));

        Ok(ret)
    }
}
