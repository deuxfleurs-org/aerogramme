use std::sync::Arc;

use anyhow::Result;
use imap_codec::imap_types::command::{Command, CommandBody};
use imap_codec::imap_types::core::Charset;
use imap_codec::imap_types::fetch::MacroOrMessageDataItemNames;
use imap_codec::imap_types::search::SearchKey;
use imap_codec::imap_types::sequence::SequenceSet;

use crate::imap::command::anystate;
use crate::imap::flow;
use crate::imap::mailbox_view::MailboxView;
use crate::imap::response::Response;
use crate::mail::user::User;

pub struct ExaminedContext<'a> {
    pub req: &'a Command<'a>,
    pub user: &'a Arc<User>,
    pub mailbox: &'a mut MailboxView,
}

pub async fn dispatch(ctx: ExaminedContext<'_>) -> Result<(Response, flow::Transition)> {
    match &ctx.req.body {
        // Any State
        // noop is specific to this state
        CommandBody::Capability => anystate::capability(ctx.req.tag.clone()),
        CommandBody::Logout => Ok((Response::bye()?, flow::Transition::Logout)),

        // Specific to the EXAMINE state (specialization of the SELECTED state)
        // ~3 commands -> close, fetch, search + NOOP
        CommandBody::Close => ctx.close().await,
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
            Response::bad()
                .to_req(ctx.req)
                .message("Forbidden command: can't write in read-only mode (EXAMINE)")
                .build()?,
            flow::Transition::None,
        )),

        // The command does not belong to this state
        _ => anystate::wrong_state(ctx.req.tag.clone()),
    }
}

// --- PRIVATE ---

impl<'a> ExaminedContext<'a> {
    /// CLOSE in examined state is not the same as in selected state
    /// (in selected state it also does an EXPUNGE, here it doesn't)
    async fn close(self) -> Result<(Response, flow::Transition)> {
        Ok((
            Response::ok()
                .to_req(self.req)
                .message("CLOSE completed")
                .build()?,
            flow::Transition::Unselect,
        ))
    }

    pub async fn fetch(
        self,
        sequence_set: &SequenceSet,
        attributes: &MacroOrMessageDataItemNames<'a>,
        uid: &bool,
    ) -> Result<(Response, flow::Transition)> {
        match self.mailbox.fetch(sequence_set, attributes, uid).await {
            Ok(resp) => Ok((
                Response::ok()
                    .to_req(self.req)
                    .message("FETCH completed")
                    .set_data(resp)
                    .build()?,
                flow::Transition::None,
            )),
            Err(e) => Ok((
                Response::no()
                    .to_req(self.req)
                    .message(e.to_string())
                    .build()?,
                flow::Transition::None,
            )),
        }
    }

    pub async fn search(
        self,
        _charset: &Option<Charset<'a>>,
        _criteria: &SearchKey<'a>,
        _uid: &bool,
    ) -> Result<(Response, flow::Transition)> {
        Ok((
            Response::bad()
                .to_req(self.req)
                .message("Not implemented")
                .build()?,
            flow::Transition::None,
        ))
    }

    pub async fn noop(self) -> Result<(Response, flow::Transition)> {
        self.mailbox.mailbox.force_sync().await?;

        let updates = self.mailbox.update().await?;
        Ok((
            Response::ok()
                .to_req(self.req)
                .message("NOOP completed.")
                .set_data(updates)
                .build()?,
            flow::Transition::None,
        ))
    }
}
