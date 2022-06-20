
use anyhow::{Result, Error};
use boitalettres::proto::Response;
use imap_codec::types::command::CommandBody;
use imap_codec::types::core::AString;
use imap_codec::types::response::{Capability, Data, Response as ImapRes, Status};

use crate::imap::flow;
use crate::imap::session::InnerContext;

//--- dispatching

pub async fn dispatch<'a>(ctx: &'a InnerContext<'a>) -> Result<Response> {
    match ctx.req.body {
        CommandBody::Capability => capability(ctx).await,
        CommandBody::Login { username, password } => login(ctx, username, password).await,
        _ => Status::no(Some(ctx.req.tag.clone()), None, "This command is not available in the ANONYMOUS state.")
            .map(|s| vec![ImapRes::Status(s)])
            .map_err(Error::msg),
    }
}

//--- Command controllers, private

async fn capability<'a>(ctx: InnerContext<'a>) -> Result<Response> {
    let capabilities = vec![Capability::Imap4Rev1, Capability::Idle];
    let res = vec![
        ImapRes::Data(Data::Capability(capabilities)),
        ImapRes::Status(
            Status::ok(Some(ctx.req.tag.clone()), None, "Server capabilities")
            .map_err(Error::msg)?,
            ),
    ];
    Ok(res)
}

async fn login<'a>(ctx: InnerContext<'a>, username: AString, password: AString) -> Result<Response> {
    let (u, p) = (String::try_from(username)?, String::try_from(password)?);
    tracing::info!(user = %u, "command.login");

    let creds = match ctx.login_provider.login(&u, &p).await {
        Err(e) => {
            tracing::debug!(error=%e, "authentication failed");
            return Ok(vec![ImapRes::Status(
                    Status::no(Some(ctx.req.tag.clone()), None, "Authentication failed")
                    .map_err(Error::msg)?,
                    )]);
        }
        Ok(c) => c,
    };

    let user = flow::User {
        creds,
        name: u.clone(),
    };
    ctx.state.authenticate(user)?;

    tracing::info!(username=%u, "connected");
    Ok(vec![
       //@FIXME we could send a capability status here too
       ImapRes::Status(
           Status::ok(Some(ctx.req.tag.clone()), None, "completed").map_err(Error::msg)?,
           ),
    ])
}
