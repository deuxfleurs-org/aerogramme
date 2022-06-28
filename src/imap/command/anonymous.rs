use anyhow::{Error, Result};
use boitalettres::proto::Response;
use imap_codec::types::command::CommandBody;
use imap_codec::types::core::AString;
use imap_codec::types::response::{Capability, Data, Response as ImapRes, Status};

use crate::imap::flow;
use crate::imap::session::InnerContext;

//--- dispatching

pub async fn dispatch<'a>(ctx: InnerContext<'a>) -> Result<(Response, flow::Transition)> {
    match &ctx.req.command.body {
        CommandBody::Noop => Ok((Response::ok("Noop completed.")?, flow::Transition::No)),
        CommandBody::Capability => capability(ctx).await,
        CommandBody::Login { username, password } => login(ctx, username, password).await,
        _ => Ok((
            Response::no("This command is not available in the ANONYMOUS state.")?,
            flow::Transition::No,
        )),
    }
}

//--- Command controllers, private

async fn capability<'a>(ctx: InnerContext<'a>) -> Result<(Response, flow::Transition)> {
    let capabilities = vec![Capability::Imap4Rev1, Capability::Idle];
    let res = Response::ok("Server capabilities")?.with_body(Data::Capability(capabilities));
    Ok((res, flow::Transition::No))
}

async fn login<'a>(
    ctx: InnerContext<'a>,
    username: &AString,
    password: &AString,
) -> Result<(Response, flow::Transition)> {
    let (u, p) = (
        String::try_from(username.clone())?,
        String::try_from(password.clone())?,
    );
    tracing::info!(user = %u, "command.login");

    let creds = match ctx.login.login(&u, &p).await {
        Err(e) => {
            tracing::debug!(error=%e, "authentication failed");
            return Ok((Response::no("Authentication failed")?, flow::Transition::No));
        }
        Ok(c) => c,
    };

    let user = flow::User {
        creds,
        name: u.clone(),
    };

    tracing::info!(username=%u, "connected");
    Ok((
        Response::ok("Completed")?,
        flow::Transition::Authenticate(user),
    ))
}
