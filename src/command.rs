use std::sync::{Arc, Mutex};

use boitalettres::errors::Error as BalError;
use boitalettres::proto::{Request, Response};
use imap_codec::types::core::{Tag, AString};
use imap_codec::types::response::{Capability, Data};
use imap_codec::types::mailbox::{Mailbox as MailboxCodec, ListMailbox};
use imap_codec::types::sequence::SequenceSet;
use imap_codec::types::fetch_attributes::MacroOrFetchAttributes;

use crate::mailstore::Mailstore;
use crate::mailbox::Mailbox;
use crate::service::Session;

pub struct Command<'a> {
    tag: Tag,
    session: &'a mut Session,
}

impl<'a> Command<'a> {
    pub fn new(tag: Tag, session: &'a mut Session) -> Self {
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

        Response::ok("Logged in")
    }

    pub async fn lsub(&self, reference: MailboxCodec, mailbox_wildcard: ListMailbox) -> Result<Response, BalError> {
        Response::bad("Not implemented")
    }

    pub async fn list(&self, reference: MailboxCodec, mailbox_wildcard: ListMailbox) -> Result<Response, BalError> {
        Response::bad("Not implemented")
    }

    pub async fn select(&mut self, mailbox: MailboxCodec) -> Result<Response, BalError> {

        let mb = Mailbox::new(self.session.creds.as_ref().unwrap(), "TestMailbox".to_string()).unwrap();
        self.session.selected = Some(mb);

        Response::bad("Not implemented")
    }

    pub async fn fetch(&self, sequence_set: SequenceSet, attributes: MacroOrFetchAttributes, uid: bool) -> Result<Response, BalError> {
        Response::bad("Not implemented")
    }
}
