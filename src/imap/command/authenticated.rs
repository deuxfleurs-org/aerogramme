pub async fn dispatch(ctx: Context) -> Result<Response> {
    match req.body {
        CommandBody::Capability => anonymous::capability().await, // we use the same implem for now
        CommandBody::Lsub { reference, mailbox_wildcard, } => authenticated::lsub(reference, mailbox_wildcard).await,
        CommandBody::List { reference, mailbox_wildcard, } => authenticated::list(reference, mailbox_wildcard).await,
        CommandBody::Select { mailbox } => authenticated::select(user, mailbox).await.and_then(|(mailbox, response)| {
            self.state.select(mailbox);
            Ok(response)
        }),
        _ => Status::no(Some(msg.req.tag.clone()), None, "This command is not available in the AUTHENTICATED state.")
            .map(|s| vec![ImapRes::Status(s)])
            .map_err(Error::msg),
    },
}

    pub async fn lsub(
        &self,
        reference: MailboxCodec,
        mailbox_wildcard: ListMailbox,
    ) -> Result<Response> {
        Ok(vec![ImapRes::Status(
            Status::bad(Some(self.tag.clone()), None, "Not implemented").map_err(Error::msg)?,
        )])
    }

    pub async fn list(
        &self,
        reference: MailboxCodec,
        mailbox_wildcard: ListMailbox,
    ) -> Result<Response> {
        Ok(vec![ImapRes::Status(
            Status::bad(Some(self.tag.clone()), None, "Not implemented").map_err(Error::msg)?,
        )])
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
    pub async fn select(&mut self, mailbox: MailboxCodec) -> Result<Response> {
        let name = String::try_from(mailbox)?;
        let user = match self.session.user.as_ref() {
            Some(u) => u,
            _ => {
                return Ok(vec![ImapRes::Status(
                    Status::no(Some(self.tag.clone()), None, "Not implemented")
                        .map_err(Error::msg)?,
                )])
            }
        };

        let mut mb = Mailbox::new(&user.creds, name.clone())?;
        tracing::info!(username=%user.name, mailbox=%name, "mailbox.selected");

        let sum = mb.summary().await?;
        tracing::trace!(summary=%sum, "mailbox.summary");

        let body = vec![Data::Exists(sum.exists.try_into()?), Data::Recent(0)];

        self.session.selected = Some(mb);

        let r_unseen = Status::ok(None, Some(Code::Unseen(0)), "").map_err(Error::msg)?;
        let r_permanentflags = Status::ok(None, Some(Code::

        Ok(vec![
            ImapRes::Data(Data::Exists(0)),
            ImapRes::Data(Data::Recent(0)),
            ImapRes::Data(Data::Flags(vec![]),
            ImapRes::Status(),
            ImapRes::Status(),
            ImapRes::Status()
                Status::ok(
                    Some(self.tag.clone()),
                    Some(Code::ReadWrite),
                    "Select completed",
                )
                .map_err(Error::msg)?,
        )])
    }
