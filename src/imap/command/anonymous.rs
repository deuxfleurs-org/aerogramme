use anyhow::{Error, Result};
use boitalettres::proto::{res::body::Data as Body, Request, Response};
use imap_codec::types::command::CommandBody;
use imap_codec::types::core::AString;
use imap_codec::types::response::{Capability, Data, Status};

use crate::imap::flow;
use crate::login::ArcLoginProvider;
use crate::mail::user::User;

//--- dispatching

pub struct AnonymousContext<'a> {
    pub req: &'a Request,
    pub login_provider: Option<&'a ArcLoginProvider>,
}

pub async fn dispatch<'a>(ctx: AnonymousContext<'a>) -> Result<(Response, flow::Transition)> {
    match &ctx.req.command.body {
        CommandBody::Noop => Ok((Response::ok("Noop completed.")?, flow::Transition::None)),
        CommandBody::Capability => ctx.capability().await,
        CommandBody::Logout => ctx.logout().await,
        CommandBody::Login { username, password } => ctx.login(username, password).await,
        _ => Ok((
            Response::no("This command is not available in the ANONYMOUS state.")?,
            flow::Transition::None,
        )),
    }
}

//--- Command controllers, private

impl<'a> AnonymousContext<'a> {
    async fn capability(self) -> Result<(Response, flow::Transition)> {
        let capabilities = vec![Capability::Imap4Rev1, Capability::Idle];
        let res = Response::ok("Server capabilities")?.with_body(Data::Capability(capabilities));
        Ok((res, flow::Transition::None))
    }

    async fn login(
        self,
        username: &AString,
        password: &AString,
    ) -> Result<(Response, flow::Transition)> {
        let (u, p) = (
            String::try_from(username.clone())?,
            String::try_from(password.clone())?,
        );
        tracing::info!(user = %u, "command.login");

        let login_provider = match &self.login_provider {
            Some(lp) => lp,
            None => {
                return Ok((
                    Response::no("Login command not available (already logged in)")?,
                    flow::Transition::None,
                ))
            }
        };

        let creds = match login_provider.login(&u, &p).await {
            Err(e) => {
                tracing::debug!(error=%e, "authentication failed");
                return Ok((
                    Response::no("Authentication failed")?,
                    flow::Transition::None,
                ));
            }
            Ok(c) => c,
        };

        let user = User::new(u.clone(), creds)?;

        tracing::info!(username=%u, "connected");
        Ok((
            Response::ok("Completed")?,
            flow::Transition::Authenticate(user),
        ))
    }

    // C: 10 logout
    // S: * BYE Logging out
    // S: 10 OK Logout completed.
    async fn logout(self) -> Result<(Response, flow::Transition)> {
        // @FIXME we should implement  From<Vec<Status>> and From<Vec<ImapStatus>> in
        // boitalettres/src/proto/res/body.rs
        Ok((
            Response::ok("Logout completed")?.with_body(vec![Body::Status(
                Status::bye(None, "Logging out")
                    .map_err(|e| Error::msg(e).context("Unable to generate IMAP status"))?,
            )]),
            flow::Transition::Logout,
        ))
    }
}
