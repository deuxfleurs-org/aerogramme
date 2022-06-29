use anyhow::{Error, Result};
use boitalettres::proto::Request;
use boitalettres::proto::Response;
use imap_codec::types::command::CommandBody;
use imap_codec::types::core::Tag;
use imap_codec::types::fetch_attributes::MacroOrFetchAttributes;
use imap_codec::types::response::{Response as ImapRes, Status};
use imap_codec::types::sequence::SequenceSet;

use crate::imap::command::authenticated;
use crate::imap::flow;
use crate::imap::session::InnerContext;
use crate::mail::Mailbox;

pub struct SelectedContext<'a> {
    pub req: &'a Request,
    pub user: &'a flow::User,
    pub mailbox: &'a mut Mailbox,
}

pub async fn dispatch<'a>(ctx: SelectedContext<'a>) -> Result<(Response, flow::Transition)> {
    match &ctx.req.command.body {
        CommandBody::Fetch {
            sequence_set,
            attributes,
            uid,
        } => ctx.fetch(sequence_set, attributes, uid).await,
        _ => {
            let ctx = authenticated::AuthenticatedContext {
                req: ctx.req,
                user: ctx.user,
            };
            authenticated::dispatch(ctx).await
        }
    }
}

// --- PRIVATE ---

impl<'a> SelectedContext<'a> {
    pub async fn fetch(
        self,
        sequence_set: &SequenceSet,
        attributes: &MacroOrFetchAttributes,
        uid: &bool,
    ) -> Result<(Response, flow::Transition)> {
        Ok((Response::bad("Not implemented")?, flow::Transition::None))
    }
}
