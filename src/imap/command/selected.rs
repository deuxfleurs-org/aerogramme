use std::sync::Arc;

use anyhow::Result;
use imap_codec::imap_types::command::{Command, CommandBody};
use imap_codec::imap_types::core::Charset;
use imap_codec::imap_types::fetch::MacroOrMessageDataItemNames;
use imap_codec::imap_types::flag::{Flag, StoreResponse, StoreType};
use imap_codec::imap_types::mailbox::Mailbox as MailboxCodec;
use imap_codec::imap_types::response::{Code, CodeOther};
use imap_codec::imap_types::search::SearchKey;
use imap_codec::imap_types::sequence::SequenceSet;

use crate::imap::command::{anystate, MailboxName};
use crate::imap::flow;
use crate::imap::mailbox_view::MailboxView;
use crate::imap::response::Response;

use crate::mail::user::User;

pub struct SelectedContext<'a> {
    pub req: &'a Command<'a>,
    pub user: &'a Arc<User>,
    pub mailbox: &'a mut MailboxView,
}

pub async fn dispatch(ctx: SelectedContext<'_>) -> Result<(Response, flow::Transition)> {
    match &ctx.req.body {
        // Any State
        // noop is specific to this state
        CommandBody::Capability => anystate::capability(ctx.req.tag.clone()),
        CommandBody::Logout => Ok((Response::bye()?, flow::Transition::Logout)),

        // Specific to this state (7 commands + NOOP)
        CommandBody::Close => ctx.close().await,
        CommandBody::Noop | CommandBody::Check => ctx.noop().await,
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
        CommandBody::Expunge => ctx.expunge().await,
        CommandBody::Store {
            sequence_set,
            kind,
            response,
            flags,
            uid,
        } => ctx.store(sequence_set, kind, response, flags, uid).await,
        CommandBody::Copy {
            sequence_set,
            mailbox,
            uid,
        } => ctx.copy(sequence_set, mailbox, uid).await,

        // The command does not belong to this state
        _ => anystate::wrong_state(ctx.req.tag.clone()),
    }
}

// --- PRIVATE ---

impl<'a> SelectedContext<'a> {
    async fn close(self) -> Result<(Response, flow::Transition)> {
        // We expunge messages,
        // but we don't send the untagged EXPUNGE responses
        let tag = self.req.tag.clone();
        self.expunge().await?;
        Ok((
            Response::ok().tag(tag).message("CLOSE completed").build()?,
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

    async fn expunge(self) -> Result<(Response, flow::Transition)> {
        let tag = self.req.tag.clone();
        let data = self.mailbox.expunge().await?;

        Ok((
            Response::ok()
                .tag(tag)
                .message("EXPUNGE completed")
                .data(data)
                .build()?,
            flow::Transition::None,
        ))
    }

    async fn store(
        self,
        sequence_set: &SequenceSet,
        kind: &StoreType,
        response: &StoreResponse,
        flags: &[Flag<'a>],
        uid: &bool,
    ) -> Result<(Response, flow::Transition)> {
        let data = self
            .mailbox
            .store(sequence_set, kind, response, flags, uid)
            .await?;

        Ok((
            Response::ok()
                .to_req(self.req)
                .message("STORE completed")
                .set_data(data)
                .build()?,
            flow::Transition::None,
        ))
    }

    async fn copy(
        self,
        sequence_set: &SequenceSet,
        mailbox: &MailboxCodec<'a>,
        uid: &bool,
    ) -> Result<(Response, flow::Transition)> {
        let name: &str = MailboxName(mailbox).try_into()?;

        let mb_opt = self.user.open_mailbox(&name).await?;
        let mb = match mb_opt {
            Some(mb) => mb,
            None => {
                return Ok((
                    Response::no()
                        .to_req(self.req)
                        .message("Destination mailbox does not exist")
                        .code(Code::TryCreate)
                        .build()?,
                    flow::Transition::None,
                ))
            }
        };

        let (uidval, uid_map) = self.mailbox.copy(sequence_set, mb, uid).await?;

        let copyuid_str = format!(
            "{} {} {}",
            uidval,
            uid_map
                .iter()
                .map(|(sid, _)| format!("{}", sid))
                .collect::<Vec<_>>()
                .join(","),
            uid_map
                .iter()
                .map(|(_, tuid)| format!("{}", tuid))
                .collect::<Vec<_>>()
                .join(",")
        );

        Ok((
            Response::ok()
                .to_req(self.req)
                .message("COPY completed")
                .code(Code::Other(CodeOther::unvalidated(
                    format!("COPYUID {}", copyuid_str).into_bytes(),
                )))
                .build()?,
            flow::Transition::None,
        ))
    }
}
