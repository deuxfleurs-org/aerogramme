use anyhow::Result;
use imap_codec::imap_types::command::{Command, CommandBody};
use imap_codec::imap_types::core::{AString, NonEmptyVec};
use imap_codec::imap_types::response::{Capability, Data};
use imap_codec::imap_types::secret::Secret;

use crate::imap::command::anystate;
use crate::imap::flow;
use crate::imap::response::Response;
use crate::login::ArcLoginProvider;
use crate::mail::user::User;

//--- dispatching

pub struct AnonymousContext<'a> {
    pub req: &'a Command<'static>,
    pub login_provider: &'a ArcLoginProvider,
}

pub async fn dispatch(ctx: AnonymousContext<'_>) -> Result<(Response, flow::Transition)> {
    match &ctx.req.body {
        // Any State
        CommandBody::Noop => anystate::noop_nothing(ctx.req.tag.clone()),
        CommandBody::Capability => anystate::capability(ctx.req.tag.clone()),
        CommandBody::Logout => Ok((Response::bye()?, flow::Transition::Logout)),

        // Specific to anonymous context (3 commands)
        CommandBody::Login { username, password } => ctx.login(username, password).await,
        CommandBody::Authenticate { .. } => {
            anystate::not_implemented(ctx.req.tag.clone(), "authenticate")
        }
        //StartTLS is not implemented for now, we will probably go full TLS.

        // Collect other commands
        _ => anystate::wrong_state(ctx.req.tag.clone()),
    }
}

//--- Command controllers, private

impl<'a> AnonymousContext<'a> {
    async fn capability(self) -> Result<(Response, flow::Transition)> {
        let capabilities: NonEmptyVec<Capability> =
            (vec![Capability::Imap4Rev1, Capability::Idle]).try_into()?;
        let res = Response::ok()
            .to_req(self.req)
            .message("Server capabilities")
            .data(Data::Capability(capabilities))
            .build()?;
        Ok((res, flow::Transition::None))
    }

    async fn login(
        self,
        username: &AString<'a>,
        password: &Secret<AString<'a>>,
    ) -> Result<(Response, flow::Transition)> {
        let (u, p) = (
            std::str::from_utf8(username.as_ref())?,
            std::str::from_utf8(password.declassify().as_ref())?,
        );
        tracing::info!(user = %u, "command.login");

        let creds = match self.login_provider.login(&u, &p).await {
            Err(e) => {
                tracing::debug!(error=%e, "authentication failed");
                return Ok((
                    Response::no()
                        .to_req(self.req)
                        .message("Authentication failed")
                        .build()?,
                    flow::Transition::None,
                ));
            }
            Ok(c) => c,
        };

        let user = User::new(u.to_string(), creds).await?;

        tracing::info!(username=%u, "connected");
        Ok((
            Response::ok()
                .to_req(self.req)
                .message("Completed")
                .build()?,
            flow::Transition::Authenticate(user),
        ))
    }
}
