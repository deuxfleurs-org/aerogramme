use anyhow::Result;
use boitalettres::errors::Error as BalError;
use boitalettres::proto::{Request, Response};
use imap_codec::types::core::{AString, Tag};
use imap_codec::types::fetch_attributes::MacroOrFetchAttributes;
use imap_codec::types::mailbox::{ListMailbox, Mailbox as MailboxCodec};
use imap_codec::types::response::{Capability, Data};
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
        let body = vec![Data::Capability(capabilities)];
        let r = Response::ok("Pre-login capabilities listed, post-login capabilities have more.")?
            .with_body(body);
        Ok(r)
    }

    pub async fn login(&mut self, username: AString, password: AString) -> Result<Response> {
        let (u, p) = (String::try_from(username)?, String::try_from(password)?);
        tracing::info!(user = %u, "command.login");

        let creds = match self.session.login_provider.login(&u, &p).await {
            Err(_) => {
                return Ok(Response::no(
                    "[AUTHENTICATIONFAILED] Authentication failed.",
                )?)
            }
            Ok(c) => c,
        };

        self.session.user = Some(session::User {
            creds,
            name: u.clone(),
        });

        tracing::info!(username=%u, "connected");
        Ok(Response::ok("Logged in")?)
    }

    pub async fn lsub(
        &self,
        reference: MailboxCodec,
        mailbox_wildcard: ListMailbox,
    ) -> Result<Response> {
        Ok(Response::bad("Not implemented")?)
    }

    pub async fn list(
        &self,
        reference: MailboxCodec,
        mailbox_wildcard: ListMailbox,
    ) -> Result<Response> {
        Ok(Response::bad("Not implemented")?)
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
            _ => return Ok(Response::no("You must be connected to use SELECT")?),
        };

        let mut mb = Mailbox::new(&user.creds, name.clone())?;
        tracing::info!(username=%user.name, mailbox=%name, "mailbox.selected");

        let sum = mb.summary().await?;
        tracing::trace!(summary=%sum, "mailbox.summary");

        let body = vec![Data::Exists(sum.exists.try_into()?), Data::Recent(0)];

        self.session.selected = Some(mb);
        Ok(Response::ok("[READ-WRITE] Select completed")?.with_body(body))
    }

    pub async fn fetch(
        &self,
        sequence_set: SequenceSet,
        attributes: MacroOrFetchAttributes,
        uid: bool,
    ) -> Result<Response> {
        Ok(Response::bad("Not implemented")?)
    }
}
