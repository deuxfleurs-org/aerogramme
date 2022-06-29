use anyhow::{anyhow, Error, Result};
use boitalettres::proto::{res::body::Data as Body, Request, Response};
use imap_codec::types::command::CommandBody;
use imap_codec::types::core::Atom;
use imap_codec::types::flag::Flag;
use imap_codec::types::mailbox::{ListMailbox, Mailbox as MailboxCodec};
use imap_codec::types::response::{Code, Data, Response as ImapRes, Status};

use crate::imap::command::anonymous;
use crate::imap::flow;
use crate::imap::session::InnerContext;
use crate::mail::Mailbox;

const DEFAULT_FLAGS: [Flag; 5] = [
    Flag::Seen,
    Flag::Answered,
    Flag::Flagged,
    Flag::Deleted,
    Flag::Draft,
];

pub struct AuthenticatedContext<'a> {
    pub req: &'a Request,
    pub user: &'a flow::User,
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
        reference: &MailboxCodec,
        mailbox_wildcard: &ListMailbox,
    ) -> Result<(Response, flow::Transition)> {
        Ok((Response::bad("Not implemented")?, flow::Transition::None))
    }

    async fn list(
        self,
        reference: &MailboxCodec,
        mailbox_wildcard: &ListMailbox,
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

        let mut mb = Mailbox::new(&self.user.creds, name.clone())?;
        tracing::info!(username=%self.user.name, mailbox=%name, "mailbox.selected");

        let sum = mb.summary().await?;
        tracing::trace!(summary=%sum, "mailbox.summary");

        let mut res = Vec::<Body>::new();

        res.push(Body::Data(Data::Exists(sum.exists)));

        res.push(Body::Data(Data::Recent(sum.recent)));

        let mut flags: Vec<Flag> = sum.flags.map(|f| match f.chars().next() {
            Some('\\') => None,
            Some('$') if f == "$unseen" => None,
            Some(_) => match Atom::try_from(f.clone()) {
                Err(_) => {
                    tracing::error!(username=%self.user.name, mailbox=%name, flag=%f, "Unable to encode flag as IMAP atom");
                    None
                },
                Ok(a) => Some(Flag::Keyword(a)),
            },
            None => None,
        }).flatten().collect();
        flags.extend_from_slice(&DEFAULT_FLAGS);

        res.push(Body::Data(Data::Flags(flags.clone())));

        let uid_validity = Status::ok(None, Some(Code::UidValidity(sum.validity)), "UIDs valid")
            .map_err(Error::msg)?;
        res.push(Body::Status(uid_validity));

        let next_uid = Status::ok(None, Some(Code::UidNext(sum.next)), "Predict next UID")
            .map_err(Error::msg)?;
        res.push(Body::Status(next_uid));

        if let Some(unseen) = sum.unseen {
            let status_unseen =
                Status::ok(None, Some(Code::Unseen(unseen.clone())), "First unseen UID")
                    .map_err(Error::msg)?;
            res.push(Body::Status(status_unseen));
        }

        flags.push(Flag::Permanent);
        let permanent_flags =
            Status::ok(None, Some(Code::PermanentFlags(flags)), "Flags permitted")
                .map_err(Error::msg)?;
        res.push(Body::Status(permanent_flags));

        Ok((
            Response::ok("Select completed")?
                .with_extra_code(Code::ReadWrite)
                .with_body(res),
            flow::Transition::Select(mb),
        ))
    }
}
