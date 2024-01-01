use anyhow::Result;
use imap_codec::imap_types::command::{Command, CommandBody};
use imap_codec::imap_types::core::{AString, NonEmptyVec};
use imap_codec::imap_types::response::{Capability, Data};
use imap_codec::imap_types::secret::Secret;

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
        CommandBody::Noop => Ok((
            Response::ok()
                .to_req(ctx.req)
                .message("Noop completed.")
                .build()?,
            flow::Transition::None,
        )),
        CommandBody::Capability => ctx.capability().await,
        CommandBody::Logout => ctx.logout().await,
        CommandBody::Login { username, password } => ctx.login(username, password).await,
        cmd => {
            tracing::warn!("Unknown command for the anonymous state {:?}", cmd);
            Ok((
                Response::bad()
                    .to_req(ctx.req)
                    .message("Command unavailable")
                    .build()?,
                flow::Transition::None,
            ))
        }
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

    // C: 10 logout
    // S: * BYE Logging out
    // S: 10 OK Logout completed.
    async fn logout(self) -> Result<(Response, flow::Transition)> {
        // @FIXME we should implement  From<Vec<Status>> and From<Vec<ImapStatus>> in
        // boitalettres/src/proto/res/body.rs
        Ok((Response::bye()?, flow::Transition::Logout))
    }
}
