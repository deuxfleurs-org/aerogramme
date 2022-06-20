
use anyhow::{Result, Error};
use boitalettres::proto::Response;
use imap_codec::types::command::CommandBody;
use imap_codec::types::core::Tag;
use imap_codec::types::fetch_attributes::MacroOrFetchAttributes;
use imap_codec::types::response::{Response as ImapRes, Status};
use imap_codec::types::sequence::SequenceSet;

use crate::imap::command::authenticated;
use crate::imap::session::InnerContext;
use crate::imap::flow::User;
use crate::mailbox::Mailbox;

pub async fn dispatch<'a>(inner: &'a InnerContext<'a>, user: &'a User, mailbox: &'a Mailbox) -> Result<Response> {
    let ctx = StateContext { inner, user, mailbox, tag: &inner.req.tag };

    match ctx.inner.req.body {
        CommandBody::Fetch { sequence_set, attributes, uid, } => ctx.fetch(sequence_set, attributes, uid).await,
        _ => authenticated::dispatch(inner, user).await,
    }
}

// --- PRIVATE ---

struct StateContext<'a> {
    inner: InnerContext<'a>,
    user: &'a User,
    mailbox: &'a Mailbox,
    tag: &'a Tag,
}


impl<'a> StateContext<'a> {
    pub async fn fetch(
        &self,
        sequence_set: SequenceSet,
        attributes: MacroOrFetchAttributes,
        uid: bool,
        ) -> Result<Response> {
        Ok(vec![
           ImapRes::Status(Status::bad(Some(self.tag.clone()), None, "Not implemented").map_err(Error::msg)?),
        ])
    }
}
