use anyhow::Result;
use imap_codec::imap_types::core::Tag;
use imap_codec::imap_types::response::{Data, Status};

use crate::imap::capability::ServerCapability;
use crate::imap::flow;
use crate::imap::response::{Body, Response};

pub(crate) fn capability(
    tag: Tag<'static>,
    cap: &ServerCapability,
) -> Result<(Response<'static>, flow::Transition)> {
    let res = Response::build()
        .tag(tag)
        .message("Server capabilities")
        .data(Data::Capability(cap.to_vec()))
        .ok()?;

    Ok((res, flow::Transition::None))
}

pub(crate) fn noop_nothing(tag: Tag<'static>) -> Result<(Response<'static>, flow::Transition)> {
    Ok((
        Response::build().tag(tag).message("Noop completed.").ok()?,
        flow::Transition::None,
    ))
}

pub(crate) fn logout<'a>(tag: Tag<'a>) -> Result<(Response<'a>, flow::Transition)> {
    Ok((
        Response::build()
            .tag(tag)
            .message("Logout completed")
            .set_body(vec![Body::Status(Status::bye(None, "bye")?)])
            .ok()?,
        flow::Transition::Logout { needs_bye: false },
    ))
}

pub(crate) fn not_implemented<'a>(
    tag: Tag<'a>,
    what: &str,
) -> Result<(Response<'a>, flow::Transition)> {
    Ok((
        Response::build()
            .tag(tag)
            .message(format!("Command not implemented {}", what))
            .bad()?,
        flow::Transition::None,
    ))
}

pub(crate) fn wrong_state(tag: Tag<'static>) -> Result<(Response<'static>, flow::Transition)> {
    Ok((
        Response::build()
            .tag(tag)
            .message("Command not authorized in this state")
            .bad()?,
        flow::Transition::None,
    ))
}
