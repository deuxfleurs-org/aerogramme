use std::num::NonZeroU64;
use std::sync::Arc;

use anyhow::Result;
use imap_codec::imap_types::command::{Command, CommandBody, FetchModifier, StoreModifier};
use imap_codec::imap_types::core::{Charset, Vec1};
use imap_codec::imap_types::fetch::MacroOrMessageDataItemNames;
use imap_codec::imap_types::flag::{Flag, StoreResponse, StoreType};
use imap_codec::imap_types::mailbox::Mailbox as MailboxCodec;
use imap_codec::imap_types::response::{Code, CodeOther};
use imap_codec::imap_types::search::SearchKey;
use imap_codec::imap_types::sequence::SequenceSet;

use crate::imap::attributes::AttributesProxy;
use crate::imap::capability::{ClientCapability, ServerCapability};
use crate::imap::command::{anystate, authenticated, MailboxName};
use crate::imap::flow;
use crate::imap::mailbox_view::{MailboxView, UpdateParameters};
use crate::imap::response::Response;
use crate::user::User;

pub struct SelectedContext<'a> {
    pub req: &'a Command<'static>,
    pub user: &'a Arc<User>,
    pub mailbox: &'a mut MailboxView,
    pub server_capabilities: &'a ServerCapability,
    pub client_capabilities: &'a mut ClientCapability,
    pub perm: &'a flow::MailboxPerm,
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
        CommandBody::Close => match ctx.perm {
            flow::MailboxPerm::ReadWrite => ctx.close().await,
            flow::MailboxPerm::ReadOnly => ctx.examine_close().await,
        },
        CommandBody::Noop | CommandBody::Check => ctx.noop().await,
        CommandBody::Fetch {
            sequence_set,
            macro_or_item_names,
            modifiers,
            uid,
        } => {
            ctx.fetch(sequence_set, macro_or_item_names, modifiers, uid)
                .await
        }
        //@FIXME SearchKey::And is a legacy hack, should be refactored
        CommandBody::Search {
            charset,
            criteria,
            uid,
        } => {
            ctx.search(charset, &SearchKey::And(criteria.clone()), uid)
                .await
        }
        CommandBody::Expunge {
            // UIDPLUS (rfc4315)
            uid_sequence_set,
        } => ctx.expunge(uid_sequence_set).await,
        CommandBody::Store {
            sequence_set,
            kind,
            response,
            flags,
            modifiers,
            uid,
        } => {
            ctx.store(sequence_set, kind, response, flags, modifiers, uid)
                .await
        }
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
        self.expunge(&None).await?;
        Ok((
            Response::build().tag(tag).message("CLOSE completed").ok()?,
            flow::Transition::Unselect,
        ))
    }

    /// CLOSE in examined state is not the same as in selected state
    /// (in selected state it also does an EXPUNGE, here it doesn't)
    async fn examine_close(self) -> Result<(Response<'static>, flow::Transition)> {
        Ok((
            Response::build()
                .to_req(self.req)
                .message("CLOSE completed")
                .ok()?,
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
        modifiers: &[FetchModifier],
        uid: &bool,
    ) -> Result<(Response<'static>, flow::Transition)> {
        let ap = AttributesProxy::new(attributes, modifiers, *uid);
        let mut changed_since: Option<NonZeroU64> = None;
        modifiers.iter().for_each(|m| match m {
            FetchModifier::ChangedSince(val) => {
                changed_since = Some(*val);
            }
        });

        match self
            .mailbox
            .fetch(sequence_set, &ap, changed_since, uid)
            .await
        {
            Ok(resp) => {
                // Capabilities enabling logic only on successful command
                // (according to my understanding of the spec)
                self.client_capabilities.attributes_enable(&ap);
                self.client_capabilities.fetch_modifiers_enable(modifiers);

                // Response to the client
                Ok((
                    Response::build()
                        .to_req(self.req)
                        .message("FETCH completed")
                        .set_body(resp)
                        .ok()?,
                    flow::Transition::None,
                ))
            }
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
        let (found, enable_condstore) = self.mailbox.search(charset, criteria, *uid).await?;
        if enable_condstore {
            self.client_capabilities.enable_condstore();
        }
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
        self.mailbox.internal.mailbox.force_sync().await?;

        let updates = self.mailbox.update(UpdateParameters::default()).await?;
        Ok((
            Response::build()
                .to_req(self.req)
                .message("NOOP completed.")
                .set_body(updates)
                .ok()?,
            flow::Transition::None,
        ))
    }

    async fn expunge(
        self,
        uid_sequence_set: &Option<SequenceSet>,
    ) -> Result<(Response<'static>, flow::Transition)> {
        if let Some(failed) = self.fail_read_only() {
            return Ok((failed, flow::Transition::None));
        }

        let tag = self.req.tag.clone();
        let data = self.mailbox.expunge(uid_sequence_set).await?;

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
        modifiers: &[StoreModifier],
        uid: &bool,
    ) -> Result<(Response<'static>, flow::Transition)> {
        if let Some(failed) = self.fail_read_only() {
            return Ok((failed, flow::Transition::None));
        }

        let mut unchanged_since: Option<NonZeroU64> = None;
        modifiers.iter().for_each(|m| match m {
            StoreModifier::UnchangedSince(val) => {
                unchanged_since = Some(*val);
            }
        });

        let (data, modified) = self
            .mailbox
            .store(sequence_set, kind, response, flags, unchanged_since, uid)
            .await?;

        let mut ok_resp = Response::build()
            .to_req(self.req)
            .message("STORE completed")
            .set_body(data);

        match modified[..] {
            [] => (),
            [_head, ..] => {
                let modified_str = format!(
                    "MODIFIED {}",
                    modified
                        .into_iter()
                        .map(|x| x.to_string())
                        .collect::<Vec<_>>()
                        .join(",")
                );
                ok_resp = ok_resp.code(Code::Other(CodeOther::unvalidated(
                    modified_str.into_bytes(),
                )));
            }
        };

        self.client_capabilities.store_modifiers_enable(modifiers);

        Ok((ok_resp.ok()?, flow::Transition::None))
    }

    async fn copy(
        self,
        sequence_set: &SequenceSet,
        mailbox: &MailboxCodec<'a>,
        uid: &bool,
    ) -> Result<(Response<'static>, flow::Transition)> {
        //@FIXME Could copy be valid in EXAMINE mode?
        if let Some(failed) = self.fail_read_only() {
            return Ok((failed, flow::Transition::None));
        }

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
        if let Some(failed) = self.fail_read_only() {
            return Ok((failed, flow::Transition::None));
        }

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

    fn fail_read_only(&self) -> Option<Response<'static>> {
        match self.perm {
            flow::MailboxPerm::ReadWrite => None,
            flow::MailboxPerm::ReadOnly => Some(
                Response::build()
                    .to_req(self.req)
                    .message("Write command are forbidden while exmining mailbox")
                    .no()
                    .unwrap(),
            ),
        }
    }
}
