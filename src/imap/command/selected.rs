use anyhow::Result;
use boitalettres::proto::Request;
use boitalettres::proto::Response;
use imap_codec::types::command::CommandBody;

use imap_codec::types::fetch_attributes::MacroOrFetchAttributes;

use imap_codec::types::sequence::SequenceSet;

use crate::imap::command::authenticated;
use crate::imap::flow;
use crate::imap::mailbox_view::MailboxView;

use crate::mail::user::User;

pub struct SelectedContext<'a> {
    pub req: &'a Request,
    pub user: &'a User,
    pub mailbox: &'a mut MailboxView,
}

pub async fn dispatch<'a>(ctx: SelectedContext<'a>) -> Result<(Response, flow::Transition)> {
    match &ctx.req.command.body {
        CommandBody::Noop => ctx.noop().await,
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
        match self.mailbox.fetch(sequence_set, attributes, uid).await {
            Ok(resp) => Ok((
                Response::ok("FETCH completed")?.with_body(resp),
                flow::Transition::None,
            )),
            Err(e) => Ok((Response::no(&e.to_string())?, flow::Transition::None)),
        }
    }

    pub async fn noop(self) -> Result<(Response, flow::Transition)> {
        let updates = self.mailbox.update().await?;
        Ok((
            Response::ok("NOOP completed.")?.with_body(updates),
            flow::Transition::None,
        ))
    }
}
