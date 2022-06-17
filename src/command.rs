use anyhow::{Error, Result};
use boitalettres::errors::Error as BalError;
use boitalettres::proto::{Request, Response};
use imap_codec::types::core::{AString, Tag};
use imap_codec::types::fetch_attributes::MacroOrFetchAttributes;
use imap_codec::types::mailbox::{ListMailbox, Mailbox as MailboxCodec};
use imap_codec::types::response::{Capability, Code, Data, Response as ImapRes, Status};
use imap_codec::types::sequence::SequenceSet;

use crate::mailbox::Mailbox;
use crate::session;

pub struct Command<'a> {
    tag: Tag,
    session: &'a mut session::Instance,
}

impl<'a> Command<'a> {
    pub fn new(tag: Tag, session: &'a mut session::Instance) -> Self {
        Self { tag, session }
    }

    pub async fn capability(&self) -> Result<Response> {
        let capabilities = vec![Capability::Imap4Rev1, Capability::Idle];
        let res = vec![
            ImapRes::Data(Data::Capability(capabilities)),
            ImapRes::Status(
                Status::ok(Some(self.tag.clone()), None, "Server capabilities")
                    .map_err(Error::msg)?,
            ),
        ];
        Ok(res)
    }

    pub async fn login(&mut self, username: AString, password: AString) -> Result<Response> {
        let (u, p) = (String::try_from(username)?, String::try_from(password)?);
        tracing::info!(user = %u, "command.login");

        let creds = match self.session.login_provider.login(&u, &p).await {
            Err(e) => {
                tracing::debug!(error=%e, "authentication failed");
                return Ok(vec![ImapRes::Status(
                    Status::no(Some(self.tag.clone()), None, "Authentication failed")
                        .map_err(Error::msg)?,
                )]);
            }
            Ok(c) => c,
        };

        self.session.user = Some(session::User {
            creds,
            name: u.clone(),
        });

        tracing::info!(username=%u, "connected");
        Ok(vec![
            //@FIXME we could send a capability status here too
            ImapRes::Status(
                Status::ok(Some(self.tag.clone()), None, "completed").map_err(Error::msg)?,
            ),
        ])
    }

    pub async fn lsub(
        &self,
        reference: MailboxCodec,
        mailbox_wildcard: ListMailbox,
    ) -> Result<Response> {
        Ok(vec![ImapRes::Status(
            Status::bad(Some(self.tag.clone()), None, "Not implemented").map_err(Error::msg)?,
        )])
    }

    pub async fn list(
        &self,
        reference: MailboxCodec,
        mailbox_wildcard: ListMailbox,
    ) -> Result<Response> {
        Ok(vec![ImapRes::Status(
            Status::bad(Some(self.tag.clone()), None, "Not implemented").map_err(Error::msg)?,
        )])
    }

    /*
      * TRACE BEGIN ---


    Example:    C: A142 SELECT INBOX
                S: * 172 EXISTS
                S: * 1 RECENT
                S: * OK [UNSEEN 12] Message 12 is first unseen
                S: * OK [UIDVALIDITY 3857529045] UIDs valid
                S: * OK [UIDNEXT 4392] Predicted next UID
                S: * FLAGS (\Answered \Flagged \Deleted \Seen \Draft)
                S: * OK [PERMANENTFLAGS (\Deleted \Seen \*)] Limited
                S: A142 OK [READ-WRITE] SELECT completed

      * TRACE END ---
      */
    pub async fn select(&mut self, mailbox: MailboxCodec) -> Result<Response> {
        let name = String::try_from(mailbox)?;
        let user = match self.session.user.as_ref() {
            Some(u) => u,
            _ => {
                return Ok(vec![ImapRes::Status(
                    Status::no(Some(self.tag.clone()), None, "Not implemented")
                        .map_err(Error::msg)?,
                )])
            }
        };

        let mut mb = Mailbox::new(&user.creds, name.clone())?;
        tracing::info!(username=%user.name, mailbox=%name, "mailbox.selected");

        let sum = mb.summary().await?;
        tracing::trace!(summary=%sum, "mailbox.summary");

        let body = vec![Data::Exists(sum.exists.try_into()?), Data::Recent(0)];

        self.session.selected = Some(mb);
        Ok(vec![ImapRes::Status(
            Status::ok(
                Some(self.tag.clone()),
                Some(Code::ReadWrite),
                "Select completed",
            )
            .map_err(Error::msg)?,
        )])
    }

    pub async fn fetch(
        &self,
        sequence_set: SequenceSet,
        attributes: MacroOrFetchAttributes,
        uid: bool,
    ) -> Result<Response> {
        Ok(vec![ImapRes::Status(
            Status::bad(Some(self.tag.clone()), None, "Not implemented").map_err(Error::msg)?,
        )])
    }
}
