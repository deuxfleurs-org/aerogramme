use std::sync::Arc;

use anyhow::Result;
use boitalettres::proto::Request;
use boitalettres::proto::Response;
use imap_codec::types::command::{CommandBody, SearchKey};
use imap_codec::types::core::Charset;
use imap_codec::types::fetch_attributes::MacroOrFetchAttributes;

use imap_codec::types::sequence::SequenceSet;

use crate::imap::command::authenticated;
use crate::imap::flow;
use crate::imap::mailbox_view::MailboxView;

use crate::mail::user::User;

pub struct ExaminedContext<'a> {
    pub req: &'a Request,
    pub user: &'a Arc<User>,
    pub mailbox: &'a mut MailboxView,
}

pub async fn dispatch<'a>(ctx: ExaminedContext<'a>) -> Result<(Response, flow::Transition)> {
    match &ctx.req.command.body {
        // CLOSE in examined state is not the same as in selected state
        // (in selected state it also does an EXPUNGE, here it doesn't)
        CommandBody::Close => ctx.close().await,
        CommandBody::Fetch {
            sequence_set,
            attributes,
            uid,
        } => ctx.fetch(sequence_set, attributes, uid).await,
        CommandBody::Search {
            charset,
            criteria,
            uid,
        } => ctx.search(charset, criteria, uid).await,
        CommandBody::Noop => ctx.noop().await,
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

impl<'a> ExaminedContext<'a> {
    async fn close(self) -> Result<(Response, flow::Transition)> {
        Ok((Response::ok("CLOSE completed")?, flow::Transition::Unselect))
    }

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

    pub async fn search(
        self,
        _charset: &Option<Charset>,
        _criteria: &SearchKey,
        _uid: &bool,
    ) -> Result<(Response, flow::Transition)> {
        Ok((Response::bad("Not implemented")?, flow::Transition::None))
    }

    pub async fn noop(self) -> Result<(Response, flow::Transition)> {
        let updates = self.mailbox.sync_update().await?;
        Ok((
            Response::ok("NOOP completed.")?.with_body(updates),
            flow::Transition::None,
        ))
    }
}
