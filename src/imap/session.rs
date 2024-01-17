use anyhow::anyhow;
use crate::imap::capability::{ClientCapability, ServerCapability};
use crate::imap::command::{anonymous, authenticated, selected};
use crate::imap::flow;
use crate::imap::request::Request;
use crate::imap::response::{Response, ResponseOrIdle};
use crate::login::ArcLoginProvider;
use imap_codec::imap_types::command::Command;

//-----
pub struct Instance {
    pub login_provider: ArcLoginProvider,
    pub server_capabilities: ServerCapability,
    pub client_capabilities: ClientCapability,
    pub state: flow::State,
}
impl Instance {
    pub fn new(login_provider: ArcLoginProvider, cap: ServerCapability) -> Self {
        let client_cap = ClientCapability::new(&cap);
        Self {
            login_provider,
            state: flow::State::NotAuthenticated,
            server_capabilities: cap,
            client_capabilities: client_cap,
        }
    }

    pub async fn request(&mut self, req: Request) -> ResponseOrIdle {
        match req {
            Request::Idle => ResponseOrIdle::Response(self.idle().await),
            Request::ImapCommand(cmd) => self.command(cmd).await,
        }
    }

    pub async fn idle(&mut self) -> Response<'static> {
        let (user, mbx, perm, stop) = match &mut self.state {
            flow::State::Idle(ref user, ref mut mailbox, ref perm, ref stop) => (user, mailbox, perm, stop),
            _ => unreachable!(),
        };

        unimplemented!();
    }


    pub async fn command(&mut self, cmd: Command<'static>) -> ResponseOrIdle {
        // Command behavior is modulated by the state.
        // To prevent state error, we handle the same command in separate code paths.
        let (resp, tr) = match &mut self.state {
            flow::State::NotAuthenticated => {
                let ctx = anonymous::AnonymousContext {
                    req: &cmd,
                    login_provider: &self.login_provider,
                    server_capabilities: &self.server_capabilities,
                };
                anonymous::dispatch(ctx).await
            }
            flow::State::Authenticated(ref user) => {
                let ctx = authenticated::AuthenticatedContext {
                    req: &cmd,
                    server_capabilities: &self.server_capabilities,
                    client_capabilities: &mut self.client_capabilities,
                    user,
                };
                authenticated::dispatch(ctx).await
            }
            flow::State::Selected(ref user, ref mut mailbox, ref perm) => {
                let ctx = selected::SelectedContext {
                    req: &cmd,
                    server_capabilities: &self.server_capabilities,
                    client_capabilities: &mut self.client_capabilities,
                    user,
                    mailbox,
                    perm,
                };
                selected::dispatch(ctx).await
            }
            flow::State::Idle(..) => Err(anyhow!("can not receive command while idling")),
            flow::State::Logout => Response::build()
                .tag(cmd.tag.clone())
                .message("No commands are allowed in the LOGOUT state.")
                .bad()
                .map(|r| (r, flow::Transition::None)),
        }
        .unwrap_or_else(|err| {
            tracing::error!("Command error {:?} occured while processing {:?}", err, cmd);
            (
                Response::build()
                    .to_req(&cmd)
                    .message("Internal error while processing command")
                    .bad()
                    .unwrap(),
                flow::Transition::None,
            )
        });

        if let Err(e) = self.state.apply(tr) {
            tracing::error!(
                "Transition error {:?} occured while processing on command {:?}",
                e,
                cmd
            );
            return ResponseOrIdle::Response(Response::build()
                .to_req(&cmd)
                .message(
                    "Internal error, processing command triggered an illegal IMAP state transition",
                )
                .bad()
                .unwrap());
        }

        match self.state {
            flow::State::Idle(..) => ResponseOrIdle::StartIdle,
            _ => ResponseOrIdle::Response(resp),
        }
    }
}
