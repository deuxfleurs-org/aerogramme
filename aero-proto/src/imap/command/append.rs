use anyhow::{anyhow, Result};
use imap_codec::imap_types::command::Command;
use imap_codec::imap_types::core::Literal;
use imap_codec::imap_types::datetime::DateTime;
use imap_codec::imap_types::flag::Flag;
use imap_codec::imap_types::mailbox::Mailbox as MailboxCodec;
use imap_codec::imap_types::response::{Code, CodeOther};

use aero_collections::user::User;
use aero_collections::mail::IMF;

use crate::imap::capability::ClientCapability;
use crate::imap::command::{
    authenticated::AuthenticatedContext,
    selected::SelectedContext,
    MailboxName,
};
use crate::imap::flow;
use crate::imap::mailbox_view::MailboxView;
use crate::imap::response::Response;

// APPEND is a somewhat special case: it can be called both in the selected and
// authenticated state, with slightly different behavior: in the selected state,
// if the mailbox in which the message is appended is the currently selected
// mailbox, some extra update message must be sent.
//
// To factor these two cases, we define the append logic in this file.

pub(crate) struct AppendContext<'a> {
    pub req: &'a Command<'static>,
    pub client_capabilities: &'a mut ClientCapability,
    pub mailbox_selected: Option<&'a mut MailboxView>,
    pub user: &'a User,
}

impl<'a> From<AuthenticatedContext<'a>> for AppendContext<'a> {
    fn from(ctx: AuthenticatedContext<'a>) -> Self {
        Self {
            req: ctx.req,
            client_capabilities: ctx.client_capabilities,
            mailbox_selected: None,
            user: ctx.user,
        }
    }
}

impl<'a> From<SelectedContext<'a>> for AppendContext<'a> {
    fn from(ctx: SelectedContext<'a>) -> Self {
        Self {
            req: ctx.req,
            client_capabilities: ctx.client_capabilities,
            mailbox_selected: Some(ctx.mailbox),
            user: ctx.user,
        }
    }
}

impl<'a> AppendContext<'a> {
    pub(crate) async fn append(
        self,
        mailbox: &MailboxCodec<'a>,
        flags: &[Flag<'a>],
        date: &Option<DateTime>,
        message: &Literal<'a>,
    ) -> Result<(Response<'static>, flow::Transition)> {
        let name: &str = MailboxName(mailbox).try_into()?;

        let mut mbox = match self.user.mailboxes.open(name).await? {
            None => return Ok((
                Response::build()
                    .to_req(self.req)
                    .message("Mailbox does not exist")
                    .code(Code::TryCreate)
                    .no()?,
                flow::Transition::None,
            )),
            Some(mb) => MailboxView::new(mb, self.client_capabilities.condstore.is_enabled())
        };
        // use the selected mailbox instead if it matches
        let (mbox, on_selected) = match self.mailbox_selected {
            Some(selected) if selected.id() == mbox.id() => (selected, true),
            _ => (&mut mbox, false),
        };
                
        // FIXME?
        if date.is_some() {
            tracing::warn!("Cannot set date when appending message");
        }
        let msg = 
            IMF::try_from(message.data()).map_err(|_| anyhow!("Could not parse e-mail message"))?;
        let flags = flags.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        // TODO: filter allowed flags? ping @Quentin
        
        match mbox.append(msg, &flags).await {
            Ok((uid, uidvalidity, updates)) => {
                let mut resp = Response::build()
                    .to_req(self.req)
                    .message("APPEND completed")
                    .code(Code::Other(CodeOther::unvalidated(
                        format!("APPENDUID {} {}", uidvalidity, uid).into_bytes(),
                    )));
                if on_selected {
                    resp = resp.set_body(updates);
                }
                Ok((resp.ok()?, flow::Transition::None))
            },
            Err(e) => Ok((
                Response::build()
                    .to_req(self.req)
                    .message(e.to_string())
                    .no()?,
                flow::Transition::None,
            )),
        }
    }
}
