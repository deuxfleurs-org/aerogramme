use std::sync::Arc;

use anyhow::Result;
use boitalettres::proto::Request;
use boitalettres::proto::Response;
use imap_codec::types::command::{CommandBody, SearchKey};
use imap_codec::types::core::{Charset, NonZeroBytes};
use imap_codec::types::datetime::MyDateTime;
use imap_codec::types::fetch_attributes::MacroOrFetchAttributes;
use imap_codec::types::flag::Flag;
use imap_codec::types::mailbox::Mailbox as MailboxCodec;
use imap_codec::types::response::Code;
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

pub async fn dispatch(ctx: ExaminedContext<'_>) -> Result<(Response, flow::Transition)> {
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
        CommandBody::Append {
            mailbox,
            flags,
            date,
            message,
        } => ctx.append(mailbox, flags, date, message).await,
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
        self.mailbox.mailbox.force_sync().await?;

        let updates = self.mailbox.update().await?;
        Ok((
            Response::ok("NOOP completed.")?.with_body(updates),
            flow::Transition::None,
        ))
    }

    async fn append(
        self,
        mailbox: &MailboxCodec,
        flags: &[Flag],
        date: &Option<MyDateTime>,
        message: &NonZeroBytes,
    ) -> Result<(Response, flow::Transition)> {
        let ctx2 = authenticated::AuthenticatedContext {
            req: self.req,
            user: self.user,
        };

        match ctx2.append_internal(mailbox, flags, date, message).await {
            Ok((mb, uidvalidity, uid)) => {
                let resp = Response::ok("APPEND completed")?.with_extra_code(Code::Other(
                    "APPENDUID".try_into().unwrap(),
                    Some(format!("{} {}", uidvalidity, uid)),
                ));

                if Arc::ptr_eq(&mb, &self.mailbox.mailbox) {
                    let data = self.mailbox.update().await?;
                    Ok((resp.with_body(data), flow::Transition::None))
                } else {
                    Ok((resp, flow::Transition::None))
                }
            }
            Err(e) => Ok((Response::no(&e.to_string())?, flow::Transition::None)),
        }
    }
}
