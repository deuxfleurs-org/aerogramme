
use anyhow::{Result, Error};
use boitalettres::proto::Response;
use imap_codec::types::command::CommandBody;
use imap_codec::types::core::Tag;
use imap_codec::types::mailbox::{ListMailbox, Mailbox as MailboxCodec};
use imap_codec::types::response::{Code, Data, Response as ImapRes, Status};

use crate::imap::command::anonymous;
use crate::imap::session::InnerContext;
use crate::imap::flow::User;
use crate::mailbox::Mailbox;

/*pub async fn dispatch<'a>(inner: &'a mut InnerContext<'a>, user: &'a User) -> Result<Response> {
    let ctx = StateContext { inner, user, tag: &inner.req.tag };

    match &ctx.inner.req.body {
        CommandBody::Lsub { reference, mailbox_wildcard, } => ctx.lsub(reference.clone(), mailbox_wildcard.clone()).await,
        CommandBody::List { reference, mailbox_wildcard, } => ctx.list(reference.clone(), mailbox_wildcard.clone()).await,
        CommandBody::Select { mailbox } => ctx.select(mailbox.clone()).await,
        _ => anonymous::dispatch(ctx.inner).await,
    }
}*/

// --- PRIVATE ---

/*
struct StateContext<'a> {
    inner: &'a mut InnerContext<'a>,
    user: &'a User,
    tag: &'a Tag,
}

impl<'a> StateContext<'a> {
    async fn lsub(
        &self,
        reference: MailboxCodec,
        mailbox_wildcard: ListMailbox,
        ) -> Result<Response> {
        Ok(vec![ImapRes::Status(
                Status::bad(Some(self.tag.clone()), None, "Not implemented").map_err(Error::msg)?,
                )])
    }

    async fn list(
        &self,
        reference: MailboxCodec,
        mailbox_wildcard: ListMailbox,
        ) -> Result<Response> {
        Ok(vec![
           ImapRes::Status(Status::bad(Some(self.tag.clone()), None, "Not implemented").map_err(Error::msg)?),
        ])
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
    async fn select(&self, mailbox: MailboxCodec) -> Result<Response> {
        let name = String::try_from(mailbox)?;

        let mut mb = Mailbox::new(&self.user.creds, name.clone())?;
        tracing::info!(username=%self.user.name, mailbox=%name, "mailbox.selected");

        let sum = mb.summary().await?;
        tracing::trace!(summary=%sum, "mailbox.summary");

        let body = vec![Data::Exists(sum.exists.try_into()?), Data::Recent(0)];

        self.inner.state.select(mb)?;

        let r_unseen = Status::ok(None, Some(Code::Unseen(std::num::NonZeroU32::new(1)?)), "").map_err(Error::msg)?;
        //let r_permanentflags = Status::ok(None, Some(Code::

        Ok(vec![
           ImapRes::Data(Data::Exists(0)),
           ImapRes::Data(Data::Recent(0)),
           ImapRes::Data(Data::Flags(vec![])),
           /*ImapRes::Status(),
             ImapRes::Status(),
             ImapRes::Status(),*/
           Status::ok(
               Some(self.tag.clone()),
               Some(Code::ReadWrite),
               "Select completed",
               )
           .map_err(Error::msg)?,
        ])
    }
}
*/
