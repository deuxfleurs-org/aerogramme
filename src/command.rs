use std::sync::{Arc, Mutex};

use boitalettres::errors::Error as BalError;
use boitalettres::proto::{Request, Response};
use imap_codec::types::core::{Tag, AString};
use imap_codec::types::response::{Capability, Data};
use imap_codec::types::mailbox::{Mailbox, ListMailbox};
use imap_codec::types::sequence::SequenceSet;
use imap_codec::types::fetch_attributes::MacroOrFetchAttributes;

use crate::mailstore::Mailstore;
use crate::service::Session;

pub struct Command {
    tag: Tag,
    mailstore: Arc<Mailstore>,
    session: Arc<Mutex<Session>>,
}

impl Command {
    pub fn new(tag: Tag, mailstore: Arc<Mailstore>, session: Arc<Mutex<Session>>) -> Self {
        Self { tag, mailstore, session }
    }

    pub async fn capability(&self) -> Result<Response, BalError> {
        let capabilities = vec![Capability::Imap4Rev1, Capability::Idle];
        let body = vec![Data::Capability(capabilities)];
        let r = Response::ok("Pre-login capabilities listed, post-login capabilities have more.")?
            .with_body(body);
        Ok(r)
    }

    pub async fn login(&self, username: AString, password: AString) -> Result<Response, BalError> {
        let (u, p) = match (String::try_from(username), String::try_from(password)) {
            (Ok(u), Ok(p)) => (u, p),
            _ => return Response::bad("Invalid characters"),
        };

        tracing::debug!(user = %u, "command.login");
        let creds = match self.mailstore.login_provider.login(&u, &p).await {
            Err(_) => return Response::no("[AUTHENTICATIONFAILED] Authentication failed."),
            Ok(c) => c,
        };

        let mut session = match self.session.lock() {
          Err(_) => return Response::bad("[AUTHENTICATIONFAILED] Unable to acquire mutex."),
          Ok(s) => s,
        };
        session.creds = Some(creds);

        Response::ok("Logged in")
    }

    pub async fn lsub(&self, reference: Mailbox, mailbox_wildcard: ListMailbox) -> Result<Response, BalError> {
        Response::bad("Not implemented")
    }

    pub async fn list(&self, reference: Mailbox, mailbox_wildcard: ListMailbox) -> Result<Response, BalError> {
        Response::bad("Not implemented")
    }

    pub async fn select(&self, mailbox: Mailbox) -> Result<Response, BalError> {
        Response::bad("Not implemented")
    }

    pub async fn fetch(&self, sequence_set: SequenceSet, attributes: MacroOrFetchAttributes, uid: bool) -> Result<Response, BalError> {
        Response::bad("Not implemented")
    }
}
