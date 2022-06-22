use anyhow::{anyhow, Error, Result};
use boitalettres::proto::Response;
use imap_codec::types::command::CommandBody;
use imap_codec::types::core::Tag;
use imap_codec::types::mailbox::{ListMailbox, Mailbox as MailboxCodec};
use imap_codec::types::response::{Code, Data, Response as ImapRes, Status};

use crate::imap::command::anonymous;
use crate::imap::flow;
use crate::imap::session::InnerContext;
use crate::mailbox::Mailbox;

pub async fn dispatch<'a>(
    inner: InnerContext<'a>,
    user: &'a flow::User,
) -> Result<(Response, flow::Transition)> {
    let ctx = StateContext {
        user,
        tag: &inner.req.tag,
        inner,
    };

    match &ctx.inner.req.body {
        CommandBody::Lsub {
            reference,
            mailbox_wildcard,
        } => ctx.lsub(reference, mailbox_wildcard).await,
        CommandBody::List {
            reference,
            mailbox_wildcard,
        } => ctx.list(reference, mailbox_wildcard).await,
        CommandBody::Select { mailbox } => ctx.select(mailbox).await,
        _ => anonymous::dispatch(ctx.inner).await,
    }
}

// --- PRIVATE ---

struct StateContext<'a> {
    inner: InnerContext<'a>,
    user: &'a flow::User,
    tag: &'a Tag,
}

impl<'a> StateContext<'a> {
    async fn lsub(
        &self,
        reference: &MailboxCodec,
        mailbox_wildcard: &ListMailbox,
    ) -> Result<(Response, flow::Transition)> {
        Ok((
            vec![ImapRes::Status(
                Status::bad(Some(self.tag.clone()), None, "Not implemented").map_err(Error::msg)?,
            )],
            flow::Transition::No,
        ))
    }

    async fn list(
        &self,
        reference: &MailboxCodec,
        mailbox_wildcard: &ListMailbox,
    ) -> Result<(Response, flow::Transition)> {
        Ok((
            vec![ImapRes::Status(
                Status::bad(Some(self.tag.clone()), None, "Not implemented").map_err(Error::msg)?,
            )],
            flow::Transition::No,
        ))
    }

    /*
    * TRACE BEGIN ---


    Example:    C: A142 SELECT INBOX
    S: * 172 EXISTS
    S: * 1 RECENT
    S: * OK [UNSEEN 12] Message 12 is first unseen
    S: * OK [UIDVALIDITY 3857529045] UIDs valid
    S: * OK [UIDNEXT 4392] Predicted next UID
    S: * FLAGS (\Answered \Flagged \Deleted \Seen \Draft)
    S: * OK [PERMANENTFLAGS (\Deleted \Seen \*)] Limited
    S: A142 OK [READ-WRITE] SELECT completed

    * TRACE END ---
    */
    async fn select(&self, mailbox: &MailboxCodec) -> Result<(Response, flow::Transition)> {
        let name = String::try_from(mailbox.clone())?;

        let mut mb = Mailbox::new(&self.user.creds, name.clone())?;
        tracing::info!(username=%self.user.name, mailbox=%name, "mailbox.selected");

        let sum = mb.summary().await?;
        tracing::trace!(summary=%sum, "mailbox.summary");

        let body = vec![Data::Exists(sum.exists.try_into()?), Data::Recent(0)];

        let tr = flow::Transition::Select(mb);

        let r_unseen = Status::ok(
            None,
            Some(Code::Unseen(
                std::num::NonZeroU32::new(1).ok_or(anyhow!("Invalid message identifier"))?,
            )),
            "First unseen UID",
        )
        .map_err(Error::msg)?;
        //let r_permanentflags = Status::ok(None, Some(Code::

        Ok((
            vec![
                ImapRes::Data(Data::Exists(0)),
                ImapRes::Data(Data::Recent(0)),
                ImapRes::Data(Data::Flags(vec![])),
                /*ImapRes::Status(),
                ImapRes::Status(),
                ImapRes::Status(),*/
                ImapRes::Status(
                    Status::ok(
                        Some(self.tag.clone()),
                        Some(Code::ReadWrite),
                        "Select completed",
                    )
                    .map_err(Error::msg)?,
                ),
            ],
            tr,
        ))
    }
}
