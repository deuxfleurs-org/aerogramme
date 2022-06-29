use anyhow::Result;
use boitalettres::proto::Request;
use boitalettres::proto::Response;
use imap_codec::types::command::CommandBody;

use imap_codec::types::fetch_attributes::MacroOrFetchAttributes;

use imap_codec::types::sequence::SequenceSet;

use crate::imap::command::authenticated;
use crate::imap::flow;

use crate::mail::mailbox::Mailbox;
use crate::mail::user::User;

pub struct SelectedContext<'a> {
    pub req: &'a Request,
    pub user: &'a User,
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
        _sequence_set: &SequenceSet,
        _attributes: &MacroOrFetchAttributes,
        _uid: &bool,
    ) -> Result<(Response, flow::Transition)> {
        Ok((Response::bad("Not implemented")?, flow::Transition::None))
    }
}
