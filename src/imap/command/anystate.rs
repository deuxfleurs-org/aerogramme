use anyhow::Result;
use imap_codec::imap_types::core::{NonEmptyVec, Tag};
use imap_codec::imap_types::response::{Capability, Data};

use crate::imap::flow;
use crate::imap::response::Response;

pub(crate) fn capability(tag: Tag) -> Result<(Response, flow::Transition)> {
    let capabilities: NonEmptyVec<Capability> =
        (vec![Capability::Imap4Rev1, Capability::Idle]).try_into()?;
    let res = Response::ok()
        .tag(tag)
        .message("Server capabilities")
        .data(Data::Capability(capabilities))
        .build()?;

    Ok((res, flow::Transition::None))
}

pub(crate) fn noop_nothing(tag: Tag) -> Result<(Response, flow::Transition)> {
    Ok((
        Response::ok().tag(tag).message("Noop completed.").build()?,
        flow::Transition::None,
    ))
}

pub(crate) fn logout() -> Result<(Response, flow::Transition)> {
    Ok((Response::bye()?, flow::Transition::Logout))
}

pub(crate) fn not_implemented(tag: Tag, what: &str) -> Result<(Response, flow::Transition)> {
    Ok((
        Response::bad()
            .tag(tag)
            .message(format!("Command not implemented {}", what))
            .build()?,
        flow::Transition::None,
    ))
}

pub(crate) fn wrong_state(tag: Tag) -> Result<(Response, flow::Transition)> {
    Ok((
        Response::bad()
            .tag(tag)
            .message("Command not authorized in this state")
            .build()?,
        flow::Transition::None,
    ))
}
