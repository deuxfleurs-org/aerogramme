use anyhow::{Error, Result};
use boitalettres::proto::{res::body::Data as Body, Response};
use imap_codec::types::command::CommandBody;
use imap_codec::types::core::{AString, Atom};
use imap_codec::types::response::{Capability, Code, Data, Response as ImapRes, Status};

use crate::imap::flow;
use crate::imap::session::InnerContext;

//--- dispatching

pub async fn dispatch<'a>(ctx: InnerContext<'a>) -> Result<(Response, flow::Transition)> {
    match &ctx.req.command.body {
        CommandBody::Noop => Ok((Response::ok("Noop completed.")?, flow::Transition::No)),
        CommandBody::Capability => capability(ctx).await,
        CommandBody::Logout => logout(ctx).await,
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
// C: 10 logout
// S: * BYE Logging out
// S: 10 OK Logout completed.
async fn logout<'a>(ctx: InnerContext<'a>) -> Result<(Response, flow::Transition)> {
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
