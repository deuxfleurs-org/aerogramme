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

use crate::imap::capability::{ClientCapability, ServerCapability};
use crate::imap::command::{anystate, authenticated, MailboxName};
use crate::imap::flow;
use crate::imap::mailbox_view::MailboxView;
use crate::imap::response::Response;

use crate::mail::user::User;

pub struct SelectedContext<'a> {
    pub req: &'a Command<'static>,
    pub user: &'a Arc<User>,
    pub mailbox: &'a mut MailboxView,
    pub server_capabilities: &'a ServerCapability,
    pub client_capabilities: &'a mut ClientCapability,
}

pub async fn dispatch<'a>(
    ctx: SelectedContext<'a>,
) -> Result<(Response<'static>, flow::Transition)> {
    match &ctx.req.body {
        // Any State
        // noop is specific to this state
        CommandBody::Capability => {
            anystate::capability(ctx.req.tag.clone(), ctx.server_capabilities)
        }
        CommandBody::Logout => anystate::logout(),

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
        CommandBody::Move {
            sequence_set,
            mailbox,
            uid,
        } => ctx.r#move(sequence_set, mailbox, uid).await,

        // UNSELECT extension (rfc3691)
        CommandBody::Unselect => ctx.unselect().await,

        // In selected mode, we fallback to authenticated when needed
        _ => {
            authenticated::dispatch(authenticated::AuthenticatedContext {
                req: ctx.req,
                server_capabilities: ctx.server_capabilities,
                client_capabilities: ctx.client_capabilities,
                user: ctx.user,
            })
            .await
        }
    }
}

// --- PRIVATE ---

impl<'a> SelectedContext<'a> {
    async fn close(self) -> Result<(Response<'static>, flow::Transition)> {
        // We expunge messages,
        // but we don't send the untagged EXPUNGE responses
        let tag = self.req.tag.clone();
        self.expunge().await?;
        Ok((
            Response::build().tag(tag).message("CLOSE completed").ok()?,
            flow::Transition::Unselect,
        ))
    }

    async fn unselect(self) -> Result<(Response<'static>, flow::Transition)> {
        Ok((
            Response::build()
                .to_req(self.req)
                .message("UNSELECT completed")
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
        charset: &Option<Charset<'a>>,
        criteria: &SearchKey<'a>,
        uid: &bool,
    ) -> Result<(Response<'static>, flow::Transition)> {
        let found = self.mailbox.search(charset, criteria, *uid).await?;
        Ok((
            Response::build()
                .to_req(self.req)
                .set_body(found)
                .message("SEARCH completed")
                .ok()?,
            flow::Transition::None,
        ))
    }

    pub async fn noop(self) -> Result<(Response<'static>, flow::Transition)> {
        self.mailbox.0.mailbox.force_sync().await?;

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

    async fn expunge(self) -> Result<(Response<'static>, flow::Transition)> {
        let tag = self.req.tag.clone();
        let data = self.mailbox.expunge().await?;

        Ok((
            Response::build()
                .tag(tag)
                .message("EXPUNGE completed")
                .set_body(data)
                .ok()?,
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
    ) -> Result<(Response<'static>, flow::Transition)> {
        let data = self
            .mailbox
            .store(sequence_set, kind, response, flags, uid)
            .await?;

        Ok((
            Response::build()
                .to_req(self.req)
                .message("STORE completed")
                .set_body(data)
                .ok()?,
            flow::Transition::None,
        ))
    }

    async fn copy(
        self,
        sequence_set: &SequenceSet,
        mailbox: &MailboxCodec<'a>,
        uid: &bool,
    ) -> Result<(Response<'static>, flow::Transition)> {
        let name: &str = MailboxName(mailbox).try_into()?;

        let mb_opt = self.user.open_mailbox(&name).await?;
        let mb = match mb_opt {
            Some(mb) => mb,
            None => {
                return Ok((
                    Response::build()
                        .to_req(self.req)
                        .message("Destination mailbox does not exist")
                        .code(Code::TryCreate)
                        .no()?,
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
            Response::build()
                .to_req(self.req)
                .message("COPY completed")
                .code(Code::Other(CodeOther::unvalidated(
                    format!("COPYUID {}", copyuid_str).into_bytes(),
                )))
                .ok()?,
            flow::Transition::None,
        ))
    }

    async fn r#move(
        self,
        sequence_set: &SequenceSet,
        mailbox: &MailboxCodec<'a>,
        uid: &bool,
    ) -> Result<(Response<'static>, flow::Transition)> {
        let name: &str = MailboxName(mailbox).try_into()?;

        let mb_opt = self.user.open_mailbox(&name).await?;
        let mb = match mb_opt {
            Some(mb) => mb,
            None => {
                return Ok((
                    Response::build()
                        .to_req(self.req)
                        .message("Destination mailbox does not exist")
                        .code(Code::TryCreate)
                        .no()?,
                    flow::Transition::None,
                ))
            }
        };

        let (uidval, uid_map, data) = self.mailbox.r#move(sequence_set, mb, uid).await?;

        // compute code
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
            Response::build()
                .to_req(self.req)
                .message("COPY completed")
                .code(Code::Other(CodeOther::unvalidated(
                    format!("COPYUID {}", copyuid_str).into_bytes(),
                )))
                .set_body(data)
                .ok()?,
            flow::Transition::None,
        ))
    }
}
