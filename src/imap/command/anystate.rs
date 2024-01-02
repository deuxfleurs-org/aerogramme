use anyhow::Result;
use imap_codec::imap_types::core::{NonEmptyVec, Tag};
use imap_codec::imap_types::response::{Capability, Data};

use crate::imap::flow;
use crate::imap::response::Response;

pub(crate) fn capability(tag: Tag<'static>) -> Result<(Response<'static>, flow::Transition)> {
    let capabilities: NonEmptyVec<Capability> =
        (vec![Capability::Imap4Rev1, Capability::Idle]).try_into()?;
    let res = Response::build()
        .tag(tag)
        .message("Server capabilities")
        .data(Data::Capability(capabilities))
        .ok()?;

    Ok((res, flow::Transition::None))
}

pub(crate) fn noop_nothing(tag: Tag<'static>) -> Result<(Response<'static>, flow::Transition)> {
    Ok((
        Response::build().tag(tag).message("Noop completed.").ok()?,
        flow::Transition::None,
    ))
}

pub(crate) fn logout() -> Result<(Response<'static>, flow::Transition)> {
    Ok((Response::bye()?, flow::Transition::Logout))
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
