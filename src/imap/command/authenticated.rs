use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use boitalettres::proto::res::body::Data as Body;
use boitalettres::proto::{Request, Response};
use imap_codec::types::command::{CommandBody, StatusAttribute};
use imap_codec::types::core::NonZeroBytes;
use imap_codec::types::datetime::MyDateTime;
use imap_codec::types::flag::{Flag, FlagNameAttribute};
use imap_codec::types::mailbox::{ListMailbox, Mailbox as MailboxCodec};
use imap_codec::types::response::{Code, Data, StatusAttributeValue};

use crate::imap::command::anonymous;
use crate::imap::flow;
use crate::imap::mailbox_view::MailboxView;

use crate::mail::mailbox::Mailbox;
use crate::mail::uidindex::*;
use crate::mail::user::{User, INBOX, MAILBOX_HIERARCHY_DELIMITER};
use crate::mail::IMF;

pub struct AuthenticatedContext<'a> {
    pub req: &'a Request,
    pub user: &'a Arc<User>,
}

pub async fn dispatch<'a>(ctx: AuthenticatedContext<'a>) -> Result<(Response, flow::Transition)> {
    match &ctx.req.command.body {
        CommandBody::Create { mailbox } => ctx.create(mailbox).await,
        CommandBody::Delete { mailbox } => ctx.delete(mailbox).await,
        CommandBody::Rename {
            mailbox,
            new_mailbox,
        } => ctx.rename(mailbox, new_mailbox).await,
        CommandBody::Lsub {
            reference,
            mailbox_wildcard,
        } => ctx.list(reference, mailbox_wildcard, true).await,
        CommandBody::List {
            reference,
            mailbox_wildcard,
        } => ctx.list(reference, mailbox_wildcard, false).await,
        CommandBody::Status {
            mailbox,
            attributes,
        } => ctx.status(mailbox, attributes).await,
        CommandBody::Subscribe { mailbox } => ctx.subscribe(mailbox).await,
        CommandBody::Unsubscribe { mailbox } => ctx.unsubscribe(mailbox).await,
        CommandBody::Select { mailbox } => ctx.select(mailbox).await,
        CommandBody::Examine { mailbox } => ctx.examine(mailbox).await,
        CommandBody::Append {
            mailbox,
            flags,
            date,
            message,
        } => ctx.append(mailbox, flags, date, message).await,
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
    async fn create(self, mailbox: &MailboxCodec) -> Result<(Response, flow::Transition)> {
        let name = String::try_from(mailbox.clone())?;

        if name == INBOX {
            return Ok((
                Response::bad("Cannot create INBOX")?,
                flow::Transition::None,
            ));
        }

        match self.user.create_mailbox(&name).await {
            Ok(()) => Ok((Response::ok("CREATE complete")?, flow::Transition::None)),
            Err(e) => Ok((Response::no(&e.to_string())?, flow::Transition::None)),
        }
    }

    async fn delete(self, mailbox: &MailboxCodec) -> Result<(Response, flow::Transition)> {
        let name = String::try_from(mailbox.clone())?;

        match self.user.delete_mailbox(&name).await {
            Ok(()) => Ok((Response::ok("DELETE complete")?, flow::Transition::None)),
            Err(e) => Ok((Response::no(&e.to_string())?, flow::Transition::None)),
        }
    }

    async fn rename(
        self,
        mailbox: &MailboxCodec,
        new_mailbox: &MailboxCodec,
    ) -> Result<(Response, flow::Transition)> {
        let name = String::try_from(mailbox.clone())?;
        let new_name = String::try_from(new_mailbox.clone())?;

        match self.user.rename_mailbox(&name, &new_name).await {
            Ok(()) => Ok((Response::ok("RENAME complete")?, flow::Transition::None)),
            Err(e) => Ok((Response::no(&e.to_string())?, flow::Transition::None)),
        }
    }

    async fn list(
        self,
        reference: &MailboxCodec,
        mailbox_wildcard: &ListMailbox,
        is_lsub: bool,
    ) -> Result<(Response, flow::Transition)> {
        let reference = String::try_from(reference.clone())?;
        if !reference.is_empty() {
            return Ok((
                Response::bad("References not supported")?,
                flow::Transition::None,
            ));
        }

        let wildcard = String::try_from(mailbox_wildcard.clone())?;
        if wildcard.is_empty() {
            if is_lsub {
                return Ok((
                    Response::ok("LSUB complete")?.with_body(vec![Data::Lsub {
                        items: vec![],
                        delimiter: Some(MAILBOX_HIERARCHY_DELIMITER),
                        mailbox: "".try_into().unwrap(),
                    }]),
                    flow::Transition::None,
                ));
            } else {
                return Ok((
                    Response::ok("LIST complete")?.with_body(vec![Data::List {
                        items: vec![],
                        delimiter: Some(MAILBOX_HIERARCHY_DELIMITER),
                        mailbox: "".try_into().unwrap(),
                    }]),
                    flow::Transition::None,
                ));
            }
        }

        let mailboxes = self.user.list_mailboxes().await?;
        let mut vmailboxes = BTreeMap::new();
        for mb in mailboxes.iter() {
            for (i, _) in mb.match_indices(MAILBOX_HIERARCHY_DELIMITER) {
                if i > 0 {
                    let smb = &mb[..i];
                    if !vmailboxes.contains_key(&smb) {
                        vmailboxes.insert(smb, false);
                    }
                }
            }
            vmailboxes.insert(mb, true);
        }

        let mut ret = vec![];
        for (mb, is_real) in vmailboxes.iter() {
            if matches_wildcard(&wildcard, &mb) {
                let mailbox = mb
                    .to_string()
                    .try_into()
                    .map_err(|_| anyhow!("invalid mailbox name"))?;
                let mut items = vec![FlagNameAttribute::Extension(
                    "Subscribed".try_into().unwrap(),
                )];
                if !*is_real {
                    items.push(FlagNameAttribute::Noselect);
                }
                if is_lsub {
                    ret.push(Data::Lsub {
                        items,
                        delimiter: Some(MAILBOX_HIERARCHY_DELIMITER),
                        mailbox,
                    });
                } else {
                    ret.push(Data::List {
                        items,
                        delimiter: Some(MAILBOX_HIERARCHY_DELIMITER),
                        mailbox,
                    });
                }
            }
        }

        let msg = if is_lsub {
            "LSUB completed"
        } else {
            "LIST completed"
        };
        Ok((Response::ok(msg)?.with_body(ret), flow::Transition::None))
    }

    async fn status(
        self,
        mailbox: &MailboxCodec,
        attributes: &[StatusAttribute],
    ) -> Result<(Response, flow::Transition)> {
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

        let (view, _data) = MailboxView::new(mb).await?;

        let mut ret_attrs = vec![];
        for attr in attributes.iter() {
            ret_attrs.push(match attr {
                StatusAttribute::Messages => StatusAttributeValue::Messages(view.exists()?),
                StatusAttribute::Unseen => {
                    StatusAttributeValue::Unseen(view.unseen().map(|x| x.get()).unwrap_or(0))
                }
                StatusAttribute::Recent => StatusAttributeValue::Recent(view.recent()?),
                StatusAttribute::UidNext => StatusAttributeValue::UidNext(view.uidnext()),
                StatusAttribute::UidValidity => {
                    StatusAttributeValue::UidValidity(view.uidvalidity())
                }
            });
        }

        let data = vec![Body::Data(Data::Status {
            mailbox: mailbox.clone(),
            attributes: ret_attrs,
        })];

        Ok((
            Response::ok("STATUS completed")?.with_body(data),
            flow::Transition::None,
        ))
    }

    async fn subscribe(self, mailbox: &MailboxCodec) -> Result<(Response, flow::Transition)> {
        let name = String::try_from(mailbox.clone())?;

        if self.user.has_mailbox(&name).await? {
            Ok((Response::ok("SUBSCRIBE complete")?, flow::Transition::None))
        } else {
            Ok((
                Response::bad(&format!("Mailbox {} does not exist", name))?,
                flow::Transition::None,
            ))
        }
    }

    async fn unsubscribe(self, mailbox: &MailboxCodec) -> Result<(Response, flow::Transition)> {
        let name = String::try_from(mailbox.clone())?;

        if self.user.has_mailbox(&name).await? {
            Ok((
                Response::bad(&format!(
                    "Cannot unsubscribe from mailbox {}: not supported by Aerogramme",
                    name
                ))?,
                flow::Transition::None,
            ))
        } else {
            Ok((
                Response::bad(&format!("Mailbox {} does not exist", name))?,
                flow::Transition::None,
            ))
        }
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

    async fn examine(self, mailbox: &MailboxCodec) -> Result<(Response, flow::Transition)> {
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
        tracing::info!(username=%self.user.username, mailbox=%name, "mailbox.examined");

        let (mb, data) = MailboxView::new(mb).await?;

        Ok((
            Response::ok("Examine completed")?
                .with_extra_code(Code::ReadOnly)
                .with_body(data),
            flow::Transition::Examine(mb),
        ))
    }

    async fn append(
        self,
        mailbox: &MailboxCodec,
        flags: &[Flag],
        date: &Option<MyDateTime>,
        message: &NonZeroBytes,
    ) -> Result<(Response, flow::Transition)> {
        match self.append_internal(mailbox, flags, date, message).await {
            Ok((_mb, uidvalidity, uid)) => Ok((
                Response::ok("APPEND completed")?.with_extra_code(Code::Other(
                    "APPENDUID".try_into().unwrap(),
                    Some(format!("{} {}", uidvalidity, uid)),
                )),
                flow::Transition::None,
            )),
            Err(e) => Ok((Response::no(&e.to_string())?, flow::Transition::None)),
        }
    }

    pub(crate) async fn append_internal(
        self,
        mailbox: &MailboxCodec,
        flags: &[Flag],
        date: &Option<MyDateTime>,
        message: &NonZeroBytes,
    ) -> Result<(Arc<Mailbox>, ImapUidvalidity, ImapUidvalidity)> {
        let name = String::try_from(mailbox.clone())?;

        let mb_opt = self.user.open_mailbox(&name).await?;
        let mb = match mb_opt {
            Some(mb) => mb,
            None => bail!("Mailbox does not exist"),
        };

        if date.is_some() {
            bail!("Cannot set date when appending message");
        }

        let msg = IMF::try_from(message.as_slice())
            .map_err(|_| anyhow!("Could not parse e-mail message"))?;
        let flags = flags.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        // TODO: filter allowed flags? ping @Quentin

        let (uidvalidity, uid) = mb.append(msg, None, &flags[..]).await?;

        Ok((mb, uidvalidity, uid))
    }
}

fn matches_wildcard(wildcard: &str, name: &str) -> bool {
    let wildcard = wildcard.chars().collect::<Vec<char>>();
    let name = name.chars().collect::<Vec<char>>();

    let mut matches = vec![vec![false; wildcard.len() + 1]; name.len() + 1];

    for i in 0..=name.len() {
        for j in 0..=wildcard.len() {
            matches[i][j] = (i == 0 && j == 0)
                || (j > 0
                    && matches[i][j - 1]
                    && (wildcard[j - 1] == '%' || wildcard[j - 1] == '*'))
                || (i > 0
                    && j > 0
                    && matches[i - 1][j - 1]
                    && wildcard[j - 1] == name[i - 1]
                    && wildcard[j - 1] != '%'
                    && wildcard[j - 1] != '*')
                || (i > 0
                    && j > 0
                    && matches[i - 1][j]
                    && (wildcard[j - 1] == '*'
                        || (wildcard[j - 1] == '%' && name[i - 1] != MAILBOX_HIERARCHY_DELIMITER)));
        }
    }

    matches[name.len()][wildcard.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wildcard_matches() {
        assert!(matches_wildcard("INBOX", "INBOX"));
        assert!(matches_wildcard("*", "INBOX"));
        assert!(matches_wildcard("%", "INBOX"));
        assert!(!matches_wildcard("%", "Test.Azerty"));
        assert!(!matches_wildcard("INBOX.*", "INBOX"));
        assert!(matches_wildcard("Sent.*", "Sent.A"));
        assert!(matches_wildcard("Sent.*", "Sent.A.B"));
        assert!(!matches_wildcard("Sent.%", "Sent.A.B"));
    }
}
