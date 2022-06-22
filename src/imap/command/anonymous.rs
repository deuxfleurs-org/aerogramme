
use anyhow::{Result, Error};
use boitalettres::proto::Response;
use imap_codec::types::command::CommandBody;
use imap_codec::types::core::AString;
use imap_codec::types::response::{Capability, Data, Response as ImapRes, Status};

use crate::imap::flow;
use crate::imap::session::InnerContext;

//--- dispatching

pub async fn dispatch<'a>(ctx: InnerContext<'a>) -> Result<(Response, flow::Transition)> {
    match &ctx.req.body {
        CommandBody::Capability => capability(ctx).await,
        CommandBody::Login { username, password } => login(ctx, username, password).await,
        _ => Status::no(Some(ctx.req.tag.clone()), None, "This command is not available in the ANONYMOUS state.")
            .map(|s| (vec![ImapRes::Status(s)], flow::Transition::No))
            .map_err(Error::msg),
    }
}

//--- Command controllers, private

async fn capability<'a>(ctx: InnerContext<'a>) -> Result<(Response, flow::Transition)> {
    let capabilities = vec![Capability::Imap4Rev1, Capability::Idle];
    let res = vec![
        ImapRes::Data(Data::Capability(capabilities)),
        ImapRes::Status(
            Status::ok(Some(ctx.req.tag.clone()), None, "Server capabilities")
            .map_err(Error::msg)?,
            ),
    ];
    Ok((res, flow::Transition::No))
}

async fn login<'a>(ctx: InnerContext<'a>, username: &AString, password: &AString) -> Result<(Response, flow::Transition)> {
    let (u, p) = (String::try_from(username.clone())?, String::try_from(password.clone())?);
    tracing::info!(user = %u, "command.login");

    let creds = match ctx.login.login(&u, &p).await {
        Err(e) => {
            tracing::debug!(error=%e, "authentication failed");
            return Ok((vec![ImapRes::Status(
                    Status::no(Some(ctx.req.tag.clone()), None, "Authentication failed")
                    .map_err(Error::msg)?,
                    )], flow::Transition::No));
        }
        Ok(c) => c,
    };

    let user = flow::User {
        creds,
        name: u.clone(),
    };
    let tr = flow::Transition::Authenticate(user);

    tracing::info!(username=%u, "connected");
    Ok((vec![
       //@FIXME we could send a capability status here too
       ImapRes::Status(
           Status::ok(Some(ctx.req.tag.clone()), None, "completed").map_err(Error::msg)?,
           ),
    ], tr))
}
