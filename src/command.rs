use std::sync::Arc;

use boitalettres::errors::Error as BalError;
use boitalettres::proto::{Request, Response};
use imap_codec::types::core::AString;
use imap_codec::types::response::{Capability, Data};

use crate::mailstore;

pub struct Command {
    mailstore: Arc<mailstore::Mailstore>,
}

impl Command {
    pub fn new(mailstore: Arc<mailstore::Mailstore>) -> Self {
        Self { mailstore }
    }

    pub async fn capability(self) -> Result<Response, BalError> {
        let capabilities = vec![Capability::Imap4Rev1, Capability::Idle];
        let body = vec![Data::Capability(capabilities)];
        let r = Response::ok("Pre-login capabilities listed, post-login capabilities have more.")?
            .with_body(body);
        Ok(r)
    }

    pub async fn login(self, username: AString, password: AString) -> Result<Response, BalError> {
        let (u, p) = match (String::try_from(username), String::try_from(password)) {
            (Ok(u), Ok(p)) => (u, p),
            _ => return Response::bad("Invalid characters"),
        };

        tracing::debug!(user = %u, "command.login");
        let creds = match self.mailstore.login_provider.login(&u, &p).await {
            Err(_) => return Response::no("[AUTHENTICATIONFAILED] Authentication failed."),
            Ok(c) => c,
        };

        Response::ok("Logged in")
    }
}
