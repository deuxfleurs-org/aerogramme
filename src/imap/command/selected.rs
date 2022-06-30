use anyhow::Result;
use boitalettres::proto::Request;
use boitalettres::proto::Response;
use imap_codec::types::command::CommandBody;
use imap_codec::types::mailbox::{Mailbox as MailboxCodec};
use imap_codec::types::flag::{Flag, StoreType, StoreResponse};
use imap_codec::types::fetch_attributes::MacroOrFetchAttributes;

use imap_codec::types::sequence::SequenceSet;

use crate::imap::command::examined;
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
        // Only write commands here, read commands are handled in
        // `examined.rs`
        CommandBody::Close => ctx.close().await,
        CommandBody::Expunge => ctx.expunge().await,
        CommandBody::Store {
                sequence_set,
                kind,
                response,
                flags,
                uid
        } => ctx.store(sequence_set, kind, response, flags, uid).await,
        CommandBody::Copy {
            sequence_set,
            mailbox,
            uid,
        } => ctx.copy(sequence_set, mailbox, uid).await,
        _ => {
            let ctx = examined::ExaminedContext {
                req: ctx.req,
                user: ctx.user,
                mailbox: ctx.mailbox,
            };
            examined::dispatch(ctx).await
        }
    }
}

// --- PRIVATE ---

impl<'a> SelectedContext<'a> {
    async fn close(
        self,
    ) -> Result<(Response, flow::Transition)> {
        // We expunge messages,
        // but we don't send the untagged EXPUNGE responses
        self.expunge().await?;
        Ok((Response::ok("CLOSE completed")?, flow::Transition::Unselect))
    }

    async fn expunge(
        self,
    ) -> Result<(Response, flow::Transition)> {
        Ok((Response::bad("Not implemented")?, flow::Transition::None))
    }

    async fn store(
        self,
        sequence_set: &SequenceSet,
        kind: &StoreType,
        response: &StoreResponse,
        flags: &[Flag],
        uid: &bool,
    ) -> Result<(Response, flow::Transition)> {
        Ok((Response::bad("Not implemented")?, flow::Transition::None))
    }

    async fn copy(
        self,
        sequence_set: &SequenceSet,
        mailbox: &MailboxCodec,
        uid: &bool,
    ) -> Result<(Response, flow::Transition)> {
        Ok((Response::bad("Not implemented")?, flow::Transition::None))
    }
}
