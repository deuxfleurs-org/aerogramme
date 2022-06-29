use std::num::NonZeroU32;
use std::sync::Arc;

use anyhow::{Error, Result};
use boitalettres::proto::res::body::Data as Body;
use imap_codec::types::core::Atom;
use imap_codec::types::flag::Flag;
use imap_codec::types::response::{Code, Data, MessageAttribute, Status};

use crate::mail::mailbox::Mailbox;
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
    /// These are the messages that are sent in response to a SELECT command.
    pub async fn new(mailbox: Arc<Mailbox>) -> Result<(Self, Vec<Body>)> {
        // TODO THIS IS JUST A TEST REMOVE LATER
        mailbox.test().await?;

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

    /// Looks up state changes in the mailbox and produces a set of IMAP
    /// responses describing the new state.
    pub async fn update(&mut self) -> Result<Vec<Body>> {
        self.mailbox.sync().await?;
        // TODO THIS IS JUST A TEST REMOVE LATER
        self.mailbox.test().await?;

        let new_view = MailboxView {
            mailbox: self.mailbox.clone(),
            known_state: self.mailbox.current_uid_index().await,
        };

        let mut data = Vec::<Body>::new();

        if new_view.known_state.uidvalidity != self.known_state.uidvalidity {
            // TODO: do we want to push less/more info than this?
            data.push(new_view.uidvalidity()?);
            data.push(new_view.exists()?);
            data.push(new_view.uidnext()?);
        } else {
            // Calculate diff between two mailbox states
            // See example in IMAP RFC in section on NOOP command:
            // we want to produce something like this:
            // C: a047 NOOP
            // S: * 22 EXPUNGE
            // S: * 23 EXISTS
            // S: * 14 FETCH (UID 1305 FLAGS (\Seen \Deleted))
            // S: a047 OK Noop completed
            // In other words:
            // - notify client of expunged mails
            // - if new mails arrived, notify client of number of existing mails
            // - if flags changed for existing mails, tell client

            // - notify client of expunged mails
            let mut n_expunge = 0;
            for (i, (uid, uuid)) in self.known_state.idx_by_uid.iter().enumerate() {
                if !new_view.known_state.table.contains_key(uuid) {
                    data.push(Body::Data(Data::Expunge(
                        NonZeroU32::try_from((i + 1 - n_expunge) as u32).unwrap(),
                    )));
                    n_expunge += 1;
                }
            }

            // - if new mails arrived, notify client of number of existing mails
            if new_view.known_state.table.len() != self.known_state.table.len() - n_expunge {
                data.push(new_view.exists()?);
            }

            // - if flags changed for existing mails, tell client
            for (i, (uid, uuid)) in new_view.known_state.idx_by_uid.iter().enumerate() {
                let old_mail = self.known_state.table.get(uuid);
                let new_mail = new_view.known_state.table.get(uuid);
                if old_mail.is_some() && old_mail != new_mail {
                    if let Some((uid, flags)) = new_mail {
                        data.push(Body::Data(Data::Fetch {
                            seq_or_uid: NonZeroU32::try_from((i + 1) as u32).unwrap(),
                            attributes: vec![
                                MessageAttribute::Uid((*uid).try_into().unwrap()),
                                MessageAttribute::Flags(
                                    flags.iter().filter_map(|f| string_to_flag(f)).collect(),
                                ),
                            ],
                        }));
                    }
                }
            }
        }

        *self = new_view;
        Ok(data)
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
            .map(|f| string_to_flag(f))
            .flatten()
            .collect();
        for f in DEFAULT_FLAGS.iter() {
            if !flags.contains(f) {
                flags.push(f.clone());
            }
        }
        let mut ret = vec![Body::Data(Data::Flags(flags.clone()))];

        flags.push(Flag::Permanent);
        let permanent_flags =
            Status::ok(None, Some(Code::PermanentFlags(flags)), "Flags permitted")
                .map_err(Error::msg)?;
        ret.push(Body::Status(permanent_flags));

        Ok(ret)
    }
}

fn string_to_flag(f: &str) -> Option<Flag> {
    match f.chars().next() {
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
    }
}
