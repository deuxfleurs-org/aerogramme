use boitalettres::errors::Error as BalError;
use boitalettres::proto::{Request, Response};
use imap_codec::types::core::{Tag, AString};
use imap_codec::types::response::{Capability, Data};
use imap_codec::types::mailbox::{Mailbox as MailboxCodec, ListMailbox};
use imap_codec::types::sequence::SequenceSet;
use imap_codec::types::fetch_attributes::MacroOrFetchAttributes;

use crate::mailstore::Mailstore;
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

    pub async fn capability(&self) -> Result<Response, BalError> {
        let capabilities = vec![Capability::Imap4Rev1, Capability::Idle];
        let body = vec![Data::Capability(capabilities)];
        let r = Response::ok("Pre-login capabilities listed, post-login capabilities have more.")?
            .with_body(body);
        Ok(r)
    }

    pub async fn login(&mut self, username: AString, password: AString) -> Result<Response, BalError> {
        let (u, p) = match (String::try_from(username), String::try_from(password)) {
            (Ok(u), Ok(p)) => (u, p),
            _ => return Response::bad("Invalid characters"),
        };

        tracing::debug!(user = %u, "command.login");
        let creds = match self.session.mailstore.login_provider.login(&u, &p).await {
            Err(_) => return Response::no("[AUTHENTICATIONFAILED] Authentication failed."),
            Ok(c) => c,
        };

        self.session.creds = Some(creds);
        self.session.username = Some(u.clone());

        tracing::info!(username=%u, "connected");
        Response::ok("Logged in")
    }

    pub async fn lsub(&self, reference: MailboxCodec, mailbox_wildcard: ListMailbox) -> Result<Response, BalError> {
        Response::bad("Not implemented")
    }

    pub async fn list(&self, reference: MailboxCodec, mailbox_wildcard: ListMailbox) -> Result<Response, BalError> {
        Response::bad("Not implemented")
    }

    pub async fn select(&mut self, mailbox: MailboxCodec) -> Result<Response, BalError> {
        let (name, creds) = match (String::try_from(mailbox), self.session.creds.as_ref()) {
            (Ok(n), Some(c)) => (n, c),
            (_, None) => return Response::no("You must be connected to use SELECT"),
            (Err(e), _) => {
                tracing::warn!("Unable to decode mailbox name: {:#?}", e);
                return Response::bad("Unable to decode mailbox name")
            },
        };

        let mb = Mailbox::new(creds, name.clone()).unwrap();
        self.session.selected = Some(mb);
        let user = self.session.username.as_ref().unwrap();

        tracing::info!(username=%user, mailbox=%name, "mailbox-selected");
        Response::bad("Not implemented")
    }

    pub async fn fetch(&self, sequence_set: SequenceSet, attributes: MacroOrFetchAttributes, uid: bool) -> Result<Response, BalError> {
        Response::bad("Not implemented")
    }
}
