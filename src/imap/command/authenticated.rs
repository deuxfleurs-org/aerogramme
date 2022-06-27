use anyhow::{anyhow, Error, Result};
use boitalettres::proto::Response;
use imap_codec::types::command::CommandBody;
use imap_codec::types::core::{Atom, Tag};
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
    async fn select(&self, mailbox: &MailboxCodec) -> Result<(Response, flow::Transition)> {
        let name = String::try_from(mailbox.clone())?;

        let mut mb = Mailbox::new(&self.user.creds, name.clone())?;
        tracing::info!(username=%self.user.name, mailbox=%name, "mailbox.selected");

        let sum = mb.summary().await?;
        tracing::trace!(summary=%sum, "mailbox.summary");

        let r_unseen = Status::ok(
            None,
            Some(Code::Unseen(
                std::num::NonZeroU32::new(1).ok_or(anyhow!("Invalid message identifier"))?,
            )),
            "First unseen UID",
        )
        .map_err(Error::msg)?;

        let mut res = Vec::<ImapRes>::new();

        res.push(ImapRes::Data(Data::Exists(sum.exists)));

        res.push(ImapRes::Data(Data::Recent(sum.recent)));

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

        res.push(ImapRes::Data(Data::Flags(flags.clone())));

        let uid_validity = Status::ok(
            None,
            Some(Code::UidValidity(sum.validity)),
            "UIDs valid"
            )
            .map_err(Error::msg)?;
        res.push(ImapRes::Status(uid_validity));

        let next_uid = Status::ok(
            None,
            Some(Code::UidNext(sum.next)),
            "Predict next UID"
        ).map_err(Error::msg)?;
        res.push(ImapRes::Status(next_uid));

        if let Some(unseen) = sum.unseen {
            let status_unseen = Status::ok(
                None,
                Some(Code::Unseen(unseen.clone())),
                "First unseen UID",
            )
            .map_err(Error::msg)?;
            res.push(ImapRes::Status(status_unseen));
        }

        flags.push(Flag::Permanent);
        let permanent_flags = Status::ok(
            None, 
            Some(Code::PermanentFlags(flags)),
            "Flags permitted",
        ).map_err(Error::msg)?;
        res.push(ImapRes::Status(permanent_flags));

        let last = Status::ok(
            Some(self.tag.clone()),
            Some(Code::ReadWrite),
            "Select completed",
        ).map_err(Error::msg)?;
        res.push(ImapRes::Status(last));

        let tr = flow::Transition::Select(mb);
        Ok((res, tr))
    }
}
