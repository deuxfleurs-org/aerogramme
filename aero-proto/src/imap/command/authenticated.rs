use std::collections::BTreeMap;
use std::sync::Arc;
use thiserror::Error;

use anyhow::{anyhow, bail, Result};
use imap_codec::imap_types::command::{
    Command, CommandBody, ListReturnItem, SelectExamineModifier,
};
use imap_codec::imap_types::core::{Atom, Literal, QuotedChar, Vec1};
use imap_codec::imap_types::datetime::DateTime;
use imap_codec::imap_types::extensions::enable::CapabilityEnable;
use imap_codec::imap_types::flag::{Flag, FlagNameAttribute};
use imap_codec::imap_types::mailbox::{ListMailbox, Mailbox as MailboxCodec};
use imap_codec::imap_types::response::{Code, CodeOther, Data};
use imap_codec::imap_types::status::{StatusDataItem, StatusDataItemName};

use aero_collections::mail::uidindex::*;
use aero_collections::user::User;
use aero_collections::mail::IMF;
use aero_collections::mail::namespace::MAILBOX_HIERARCHY_DELIMITER as MBX_HIER_DELIM_RAW;

use crate::imap::capability::{ClientCapability, ServerCapability};
use crate::imap::command::{anystate, MailboxName};
use crate::imap::flow;
use crate::imap::mailbox_view::MailboxView;
use crate::imap::response::Response;

pub struct AuthenticatedContext<'a> {
    pub req: &'a Command<'static>,
    pub server_capabilities: &'a ServerCapability,
    pub client_capabilities: &'a mut ClientCapability,
    pub user: &'a Arc<User>,
}

pub async fn dispatch<'a>(
    mut ctx: AuthenticatedContext<'a>,
) -> Result<(Response<'static>, flow::Transition)> {
    match &ctx.req.body {
        // Any state
        CommandBody::Noop => anystate::noop_nothing(ctx.req.tag.clone()),
        CommandBody::Capability => {
            anystate::capability(ctx.req.tag.clone(), ctx.server_capabilities)
        }
        CommandBody::Logout => anystate::logout(),

        // Specific to this state (11 commands)
        CommandBody::Create { mailbox } => ctx.create(mailbox).await,
        CommandBody::Delete { mailbox } => ctx.delete(mailbox).await,
        CommandBody::Rename { from, to } => ctx.rename(from, to).await,
        CommandBody::Lsub {
            reference,
            mailbox_wildcard,
        } => ctx.list(reference, mailbox_wildcard, &[], true).await,
        CommandBody::List {
            reference,
            mailbox_wildcard,
            r#return,
        } => ctx.list(reference, mailbox_wildcard, r#return, false).await,
        CommandBody::Status {
            mailbox,
            item_names,
        } => ctx.status(mailbox, item_names).await,
        CommandBody::Subscribe { mailbox } => ctx.subscribe(mailbox).await,
        CommandBody::Unsubscribe { mailbox } => ctx.unsubscribe(mailbox).await,
        CommandBody::Select { mailbox, modifiers } => ctx.select(mailbox, modifiers).await,
        CommandBody::Examine { mailbox, modifiers } => ctx.examine(mailbox, modifiers).await,
        CommandBody::Append {
            mailbox,
            flags,
            date,
            message,
        } => ctx.append(mailbox, flags, date, message).await,

        // rfc5161 ENABLE
        CommandBody::Enable { capabilities } => ctx.enable(capabilities),

        // Collect other commands
        _ => anystate::wrong_state(ctx.req.tag.clone()),
    }
}

// --- PRIVATE ---
impl<'a> AuthenticatedContext<'a> {
    async fn create(
        self,
        mailbox: &MailboxCodec<'a>,
    ) -> Result<(Response<'static>, flow::Transition)> {
        let name = match mailbox {
            MailboxCodec::Inbox => {
                return Ok((
                    Response::build()
                        .to_req(self.req)
                        .message("Cannot create INBOX")
                        .bad()?,
                    flow::Transition::None,
                ));
            }
            MailboxCodec::Other(aname) => std::str::from_utf8(aname.as_ref())?,
        };

        match self.user.create_mailbox(&name).await {
            Ok(()) => Ok((
                Response::build()
                    .to_req(self.req)
                    .message("CREATE complete")
                    .ok()?,
                flow::Transition::None,
            )),
            Err(e) => Ok((
                Response::build()
                    .to_req(self.req)
                    .message(&e.to_string())
                    .no()?,
                flow::Transition::None,
            )),
        }
    }

    async fn delete(
        self,
        mailbox: &MailboxCodec<'a>,
    ) -> Result<(Response<'static>, flow::Transition)> {
        let name: &str = MailboxName(mailbox).try_into()?;

        match self.user.delete_mailbox(&name).await {
            Ok(()) => Ok((
                Response::build()
                    .to_req(self.req)
                    .message("DELETE complete")
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

    async fn rename(
        self,
        from: &MailboxCodec<'a>,
        to: &MailboxCodec<'a>,
    ) -> Result<(Response<'static>, flow::Transition)> {
        let name: &str = MailboxName(from).try_into()?;
        let new_name: &str = MailboxName(to).try_into()?;

        match self.user.rename_mailbox(&name, &new_name).await {
            Ok(()) => Ok((
                Response::build()
                    .to_req(self.req)
                    .message("RENAME complete")
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

    async fn list(
        &mut self,
        reference: &MailboxCodec<'a>,
        mailbox_wildcard: &ListMailbox<'a>,
        must_return: &[ListReturnItem],
        is_lsub: bool,
    ) -> Result<(Response<'static>, flow::Transition)> {
        let mbx_hier_delim: QuotedChar = QuotedChar::unvalidated(MBX_HIER_DELIM_RAW);

        let reference: &str = MailboxName(reference).try_into()?;
        if !reference.is_empty() {
            return Ok((
                Response::build()
                    .to_req(self.req)
                    .message("References not supported")
                    .bad()?,
                flow::Transition::None,
            ));
        }

        let status_item_names = must_return.iter().find_map(|m| match m {
            ListReturnItem::Status(v) => Some(v),
            _ => None,
        });

        // @FIXME would probably need a rewrite to better use the imap_codec library
        let wildcard = match mailbox_wildcard {
            ListMailbox::Token(v) => std::str::from_utf8(v.as_ref())?,
            ListMailbox::String(v) => std::str::from_utf8(v.as_ref())?,
        };
        if wildcard.is_empty() {
            if is_lsub {
                return Ok((
                    Response::build()
                        .to_req(self.req)
                        .message("LSUB complete")
                        .data(Data::Lsub {
                            items: vec![],
                            delimiter: Some(mbx_hier_delim),
                            mailbox: "".try_into().unwrap(),
                        })
                        .ok()?,
                    flow::Transition::None,
                ));
            } else {
                return Ok((
                    Response::build()
                        .to_req(self.req)
                        .message("LIST complete")
                        .data(Data::List {
                            items: vec![],
                            delimiter: Some(mbx_hier_delim),
                            mailbox: "".try_into().unwrap(),
                        })
                        .ok()?,
                    flow::Transition::None,
                ));
            }
        }

        let mailboxes = self.user.list_mailboxes().await?;
        let mut vmailboxes = BTreeMap::new();
        for mb in mailboxes.iter() {
            for (i, _) in mb.match_indices(MBX_HIER_DELIM_RAW) {
                if i > 0 {
                    let smb = &mb[..i];
                    vmailboxes.entry(smb).or_insert(false);
                }
            }
            vmailboxes.insert(mb, true);
        }

        let mut ret = vec![];
        for (mb, is_real) in vmailboxes.iter() {
            if matches_wildcard(&wildcard, mb) {
                let mailbox: MailboxCodec = mb
                    .to_string()
                    .try_into()
                    .map_err(|_| anyhow!("invalid mailbox name"))?;
                let mut items = vec![FlagNameAttribute::from(Atom::unvalidated("Subscribed"))];

                // Decoration
                if !*is_real {
                    items.push(FlagNameAttribute::Noselect);
                } else {
                    match *mb {
                        "Drafts" => items.push(Atom::unvalidated("Drafts").into()),
                        "Archive" => items.push(Atom::unvalidated("Archive").into()),
                        "Sent" => items.push(Atom::unvalidated("Sent").into()),
                        "Trash" => items.push(Atom::unvalidated("Trash").into()),
                        _ => (),
                    };
                }

                // Result type
                if is_lsub {
                    ret.push(Data::Lsub {
                        items,
                        delimiter: Some(mbx_hier_delim),
                        mailbox: mailbox.clone(),
                    });
                } else {
                    ret.push(Data::List {
                        items,
                        delimiter: Some(mbx_hier_delim),
                        mailbox: mailbox.clone(),
                    });
                }

                // Also collect status
                if let Some(sin) = status_item_names {
                    let ret_attrs = match self.status_items(mb, sin).await {
                        Ok(a) => a,
                        Err(e) => {
                            tracing::error!(err=?e, mailbox=%mb, "Unable to fetch status for mailbox");
                            continue;
                        }
                    };

                    let data = Data::Status {
                        mailbox,
                        items: ret_attrs.into(),
                    };

                    ret.push(data);
                }
            }
        }

        let msg = if is_lsub {
            "LSUB completed"
        } else {
            "LIST completed"
        };
        Ok((
            Response::build()
                .to_req(self.req)
                .message(msg)
                .many_data(ret)
                .ok()?,
            flow::Transition::None,
        ))
    }

    async fn status(
        &mut self,
        mailbox: &MailboxCodec<'static>,
        attributes: &[StatusDataItemName],
    ) -> Result<(Response<'static>, flow::Transition)> {
        let name: &str = MailboxName(mailbox).try_into()?;

        let ret_attrs = match self.status_items(name, attributes).await {
            Ok(v) => v,
            Err(e) => match e.downcast_ref::<CommandError>() {
                Some(CommandError::MailboxNotFound) => {
                    return Ok((
                        Response::build()
                            .to_req(self.req)
                            .message("Mailbox does not exist")
                            .no()?,
                        flow::Transition::None,
                    ))
                }
                _ => return Err(e.into()),
            },
        };

        let data = Data::Status {
            mailbox: mailbox.clone(),
            items: ret_attrs.into(),
        };

        Ok((
            Response::build()
                .to_req(self.req)
                .message("STATUS completed")
                .data(data)
                .ok()?,
            flow::Transition::None,
        ))
    }

    async fn status_items(
        &mut self,
        name: &str,
        attributes: &[StatusDataItemName],
    ) -> Result<Vec<StatusDataItem>> {
        let mb_opt = self.user.open_mailbox(name).await?;
        let mb = match mb_opt {
            Some(mb) => mb,
            None => return Err(CommandError::MailboxNotFound.into()),
        };

        let view = MailboxView::new(mb, self.client_capabilities.condstore.is_enabled()).await;

        let mut ret_attrs = vec![];
        for attr in attributes.iter() {
            ret_attrs.push(match attr {
                StatusDataItemName::Messages => StatusDataItem::Messages(view.exists()?),
                StatusDataItemName::Unseen => StatusDataItem::Unseen(view.unseen_count() as u32),
                StatusDataItemName::Recent => StatusDataItem::Recent(view.recent()?),
                StatusDataItemName::UidNext => StatusDataItem::UidNext(view.uidnext()),
                StatusDataItemName::UidValidity => {
                    StatusDataItem::UidValidity(view.uidvalidity())
                }
                StatusDataItemName::Deleted => {
                    bail!("quota not implemented, can't return deleted elements waiting for EXPUNGE");
                },
                StatusDataItemName::DeletedStorage => {
                    bail!("quota not implemented, can't return freed storage after EXPUNGE will be run");
                },
                StatusDataItemName::HighestModSeq => {
                    self.client_capabilities.enable_condstore();
                    StatusDataItem::HighestModSeq(view.highestmodseq().get())
                },
            });
        }
        Ok(ret_attrs)
    }

    async fn subscribe(
        self,
        mailbox: &MailboxCodec<'a>,
    ) -> Result<(Response<'static>, flow::Transition)> {
        let name: &str = MailboxName(mailbox).try_into()?;

        if self.user.has_mailbox(&name).await? {
            Ok((
                Response::build()
                    .to_req(self.req)
                    .message("SUBSCRIBE complete")
                    .ok()?,
                flow::Transition::None,
            ))
        } else {
            Ok((
                Response::build()
                    .to_req(self.req)
                    .message(format!("Mailbox {} does not exist", name))
                    .bad()?,
                flow::Transition::None,
            ))
        }
    }

    async fn unsubscribe(
        self,
        mailbox: &MailboxCodec<'a>,
    ) -> Result<(Response<'static>, flow::Transition)> {
        let name: &str = MailboxName(mailbox).try_into()?;

        if self.user.has_mailbox(&name).await? {
            Ok((
                Response::build()
                    .to_req(self.req)
                    .message(format!(
                        "Cannot unsubscribe from mailbox {}: not supported by Aerogramme",
                        name
                    ))
                    .bad()?,
                flow::Transition::None,
            ))
        } else {
            Ok((
                Response::build()
                    .to_req(self.req)
                    .message(format!("Mailbox {} does not exist", name))
                    .no()?,
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
    NOTES:
      RFC3501 (imap4rev1) says if there is no OK [UNSEEN] response, client must make no assumption,
                          it is therefore correct to not return it even if there are unseen messages
      RFC9051 (imap4rev2) says that OK [UNSEEN] responses are deprecated after SELECT and EXAMINE
      For Aerogramme, we just don't send the OK [UNSEEN], it's correct to do in both specifications.


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
    async fn select(
        self,
        mailbox: &MailboxCodec<'a>,
        modifiers: &[SelectExamineModifier],
    ) -> Result<(Response<'static>, flow::Transition)> {
        self.client_capabilities.select_enable(modifiers);

        let name: &str = MailboxName(mailbox).try_into()?;

        let mb_opt = self.user.open_mailbox(&name).await?;
        let mb = match mb_opt {
            Some(mb) => mb,
            None => {
                return Ok((
                    Response::build()
                        .to_req(self.req)
                        .message("Mailbox does not exist")
                        .no()?,
                    flow::Transition::None,
                ))
            }
        };
        tracing::info!(username=%self.user.username, mailbox=%name, "mailbox.selected");

        let mb = MailboxView::new(mb, self.client_capabilities.condstore.is_enabled()).await;
        let data = mb.summary()?;

        Ok((
            Response::build()
                .message("Select completed")
                .to_req(self.req)
                .code(Code::ReadWrite)
                .set_body(data)
                .ok()?,
            flow::Transition::Select(mb, flow::MailboxPerm::ReadWrite),
        ))
    }

    async fn examine(
        self,
        mailbox: &MailboxCodec<'a>,
        modifiers: &[SelectExamineModifier],
    ) -> Result<(Response<'static>, flow::Transition)> {
        self.client_capabilities.select_enable(modifiers);

        let name: &str = MailboxName(mailbox).try_into()?;

        let mb_opt = self.user.open_mailbox(&name).await?;
        let mb = match mb_opt {
            Some(mb) => mb,
            None => {
                return Ok((
                    Response::build()
                        .to_req(self.req)
                        .message("Mailbox does not exist")
                        .no()?,
                    flow::Transition::None,
                ))
            }
        };
        tracing::info!(username=%self.user.username, mailbox=%name, "mailbox.examined");

        let mb = MailboxView::new(mb, self.client_capabilities.condstore.is_enabled()).await;
        let data = mb.summary()?;

        Ok((
            Response::build()
                .to_req(self.req)
                .message("Examine completed")
                .code(Code::ReadOnly)
                .set_body(data)
                .ok()?,
            flow::Transition::Select(mb, flow::MailboxPerm::ReadOnly),
        ))
    }

    //@FIXME we should write a specific version for the "selected" state
    //that returns some unsollicited responses
    async fn append(
        self,
        mailbox: &MailboxCodec<'a>,
        flags: &[Flag<'a>],
        date: &Option<DateTime>,
        message: &Literal<'a>,
    ) -> Result<(Response<'static>, flow::Transition)> {
        let append_tag = self.req.tag.clone();
        match self.append_internal(mailbox, flags, date, message).await {
            Ok((_mb_view, uidvalidity, uid, _modseq)) => Ok((
                Response::build()
                    .tag(append_tag)
                    .message("APPEND completed")
                    .code(Code::Other(CodeOther::unvalidated(
                        format!("APPENDUID {} {}", uidvalidity, uid).into_bytes(),
                    )))
                    .ok()?,
                flow::Transition::None,
            )),
            Err(e) => Ok((
                Response::build()
                    .tag(append_tag)
                    .message(e.to_string())
                    .no()?,
                flow::Transition::None,
            )),
        }
    }

    fn enable(
        self,
        cap_enable: &Vec1<CapabilityEnable<'static>>,
    ) -> Result<(Response<'static>, flow::Transition)> {
        let mut response_builder = Response::build().to_req(self.req);
        let capabilities = self.client_capabilities.try_enable(cap_enable.as_ref());
        if capabilities.len() > 0 {
            response_builder = response_builder.data(Data::Enabled { capabilities });
        }
        Ok((
            response_builder.message("ENABLE completed").ok()?,
            flow::Transition::None,
        ))
    }

    //@FIXME should be refactored and integrated to the mailbox view
    pub(crate) async fn append_internal(
        self,
        mailbox: &MailboxCodec<'a>,
        flags: &[Flag<'a>],
        date: &Option<DateTime>,
        message: &Literal<'a>,
    ) -> Result<(MailboxView, ImapUidvalidity, ImapUid, ModSeq)> {
        let name: &str = MailboxName(mailbox).try_into()?;

        let mb_opt = self.user.open_mailbox(&name).await?;
        let mb = match mb_opt {
            Some(mb) => mb,
            None => bail!("Mailbox does not exist"),
        };
        let view = MailboxView::new(mb, self.client_capabilities.condstore.is_enabled()).await;

        if date.is_some() {
            tracing::warn!("Cannot set date when appending message");
        }

        let msg =
            IMF::try_from(message.data()).map_err(|_| anyhow!("Could not parse e-mail message"))?;
        let flags = flags.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        // TODO: filter allowed flags? ping @Quentin

        let (uidvalidity, uid, modseq) =
            view.internal.mailbox.append(msg, None, &flags[..]).await?;
        //let unsollicited = view.update(UpdateParameters::default()).await?;

        Ok((view, uidvalidity, uid, modseq))
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
                        || (wildcard[j - 1] == '%' && name[i - 1] != MBX_HIER_DELIM_RAW)));
        }
    }

    matches[name.len()][wildcard.len()]
}

#[derive(Error, Debug)]
pub enum CommandError {
    #[error("Mailbox not found")]
    MailboxNotFound,
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
