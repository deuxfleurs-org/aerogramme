use anyhow::{Error, Result};
use boitalettres::proto::{res::body::Data as Body, Request, Response};
use imap_codec::types::command::CommandBody;
use imap_codec::types::core::Atom;
use imap_codec::types::flag::Flag;
use imap_codec::types::mailbox::{ListMailbox, Mailbox as MailboxCodec};
use imap_codec::types::response::{Code, Data, Status};

use crate::imap::command::anonymous;
use crate::imap::flow;
use crate::imap::mailbox_view::MailboxView;

use crate::mail::mailbox::Mailbox;
use crate::mail::user::User;

pub struct AuthenticatedContext<'a> {
    pub req: &'a Request,
    pub user: &'a User,
}

pub async fn dispatch<'a>(ctx: AuthenticatedContext<'a>) -> Result<(Response, flow::Transition)> {
    match &ctx.req.command.body {
        CommandBody::Lsub {
            reference,
            mailbox_wildcard,
        } => ctx.lsub(reference, mailbox_wildcard).await,
        CommandBody::List {
            reference,
            mailbox_wildcard,
        } => ctx.list(reference, mailbox_wildcard).await,
        CommandBody::Select { mailbox } => ctx.select(mailbox).await,
        _ => {
            let ctx = anonymous::AnonymousContext {
                req: ctx.req,
                login_provider: None,
            };
            anonymous::dispatch(ctx).await
        }
    }
}

// --- PRIVATE ---

impl<'a> AuthenticatedContext<'a> {
    async fn lsub(
        self,
        _reference: &MailboxCodec,
        _mailbox_wildcard: &ListMailbox,
    ) -> Result<(Response, flow::Transition)> {
        Ok((Response::bad("Not implemented")?, flow::Transition::None))
    }

    async fn list(
        self,
        _reference: &MailboxCodec,
        _mailbox_wildcard: &ListMailbox,
    ) -> Result<(Response, flow::Transition)> {
        Ok((Response::bad("Not implemented")?, flow::Transition::None))
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

    --- a mailbox with no unseen message -> no unseen entry

    20 select "INBOX.achats"
    * FLAGS (\Answered \Flagged \Deleted \Seen \Draft $Forwarded JUNK $label1)
    * OK [PERMANENTFLAGS (\Answered \Flagged \Deleted \Seen \Draft $Forwarded JUNK $label1 \*)] Flags permitted.
    * 88 EXISTS
    * 0 RECENT
    * OK [UIDVALIDITY 1347986788] UIDs valid
    * OK [UIDNEXT 91] Predicted next UID
    * OK [HIGHESTMODSEQ 72] Highest
    20 OK [READ-WRITE] Select completed (0.001 + 0.000 secs).

    * TRACE END ---
    */
    async fn select(self, mailbox: &MailboxCodec) -> Result<(Response, flow::Transition)> {
        let name = String::try_from(mailbox.clone())?;

        let mb_opt = self.user.open_mailbox(&name).await?;
        let mb = match mb_opt {
            Some(mb) => mb,
            None => {
                return Ok((
                    Response::no("Mailbox does not exist")?,
                    flow::Transition::None,
                ))
            }
        };
        tracing::info!(username=%self.user.username, mailbox=%name, "mailbox.selected");

        let (mb, data) = MailboxView::new(mb).await?;

        Ok((
            Response::ok("Select completed")?
                .with_extra_code(Code::ReadWrite)
                .with_body(data),
            flow::Transition::Select(mb),
        ))
    }
}
