use std::sync::Arc;

use anyhow::Result;
use imap_codec::imap_types::command::{Command, CommandBody};
use imap_codec::imap_types::core::Charset;
use imap_codec::imap_types::fetch::MacroOrMessageDataItemNames;
use imap_codec::imap_types::search::SearchKey;
use imap_codec::imap_types::sequence::SequenceSet;

use crate::imap::capability::ServerCapability;
use crate::imap::command::{anystate, authenticated};
use crate::imap::flow;
use crate::imap::mailbox_view::MailboxView;
use crate::imap::response::Response;
use crate::mail::user::User;

pub struct ExaminedContext<'a> {
    pub req: &'a Command<'static>,
    pub user: &'a Arc<User>,
    pub mailbox: &'a mut MailboxView,
    pub server_capabilities: &'a ServerCapability,
}

pub async fn dispatch(ctx: ExaminedContext<'_>) -> Result<(Response<'static>, flow::Transition)> {
    match &ctx.req.body {
        // Any State
        // noop is specific to this state
        CommandBody::Capability => {
            anystate::capability(ctx.req.tag.clone(), ctx.server_capabilities)
        }
        CommandBody::Logout => anystate::logout(),

        // Specific to the EXAMINE state (specialization of the SELECTED state)
        // ~3 commands -> close, fetch, search + NOOP
        CommandBody::Close => ctx.close("CLOSE").await,
        CommandBody::Fetch {
            sequence_set,
            macro_or_item_names,
            uid,
        } => ctx.fetch(sequence_set, macro_or_item_names, uid).await,
        CommandBody::Search {
            charset,
            criteria,
            uid,
        } => ctx.search(charset, criteria, uid).await,
        CommandBody::Noop | CommandBody::Check => ctx.noop().await,
        CommandBody::Expunge { .. } | CommandBody::Store { .. } => Ok((
            Response::build()
                .to_req(ctx.req)
                .message("Forbidden command: can't write in read-only mode (EXAMINE)")
                .no()?,
            flow::Transition::None,
        )),

        // UNSELECT extension (rfc3691)
        CommandBody::Unselect => ctx.close("UNSELECT").await,

        // In examined mode, we fallback to authenticated when needed
        _ => {
            authenticated::dispatch(authenticated::AuthenticatedContext {
                req: ctx.req,
                server_capabilities: ctx.server_capabilities,
                user: ctx.user,
            })
            .await
        }
    }
}

// --- PRIVATE ---

impl<'a> ExaminedContext<'a> {
    /// CLOSE in examined state is not the same as in selected state
    /// (in selected state it also does an EXPUNGE, here it doesn't)
    async fn close(self, kind: &str) -> Result<(Response<'static>, flow::Transition)> {
        Ok((
            Response::build()
                .to_req(self.req)
                .message(format!("{} completed", kind))
                .ok()?,
            flow::Transition::Unselect,
        ))
    }

    pub async fn fetch(
        self,
        sequence_set: &SequenceSet,
        attributes: &'a MacroOrMessageDataItemNames<'static>,
        uid: &bool,
    ) -> Result<(Response<'static>, flow::Transition)> {
        match self.mailbox.fetch(sequence_set, attributes, uid).await {
            Ok(resp) => Ok((
                Response::build()
                    .to_req(self.req)
                    .message("FETCH completed")
                    .set_body(resp)
                    .ok()?,
                flow::Transition::None,
            )),
            Err(e) => Ok((
                Response::build()
                    .to_req(self.req)
                    .message(e.to_string())
                    .no()?,
                flow::Transition::None,
            )),
        }
    }

    pub async fn search(
        self,
        _charset: &Option<Charset<'a>>,
        _criteria: &SearchKey<'a>,
        _uid: &bool,
    ) -> Result<(Response<'static>, flow::Transition)> {
        Ok((
            Response::build()
                .to_req(self.req)
                .message("Not implemented")
                .bad()?,
            flow::Transition::None,
        ))
    }

    pub async fn noop(self) -> Result<(Response<'static>, flow::Transition)> {
        self.mailbox.mailbox.force_sync().await?;

        let updates = self.mailbox.update().await?;
        Ok((
            Response::build()
                .to_req(self.req)
                .message("NOOP completed.")
                .set_body(updates)
                .ok()?,
            flow::Transition::None,
        ))
    }
}
