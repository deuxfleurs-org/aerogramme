use anyhow::{anyhow, bail, Context, Result};
use imap_codec::imap_types::{command::Command, core::Tag};

use aero_user::login::ArcLoginProvider;

use crate::imap::capability::{ClientCapability, ServerCapability};
use crate::imap::command::{anonymous, authenticated, selected};
use crate::imap::flow;
use crate::imap::request::Request;
use crate::imap::response::{Response, ResponseOrIdle};

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
            Request::IdleStart(tag) => self.idle_init(tag),
            Request::IdlePoll => self.idle_poll().await,
            Request::ImapCommand(cmd) => self.command(cmd).await,
        }
    }

    pub fn idle_init(&mut self, tag: Tag<'static>) -> ResponseOrIdle {
        // Build transition
        //@FIXME the notifier should be hidden inside the state and thus not part of the transition!
        let transition = flow::Transition::Idle(tag.clone(), tokio::sync::Notify::new());

        // Try to apply the transition and get the stop notifier
        let maybe_stop = self
            .state
            .apply(transition)
            .context("IDLE transition failed")
            .and_then(|_| {
                self.state
                    .notify()
                    .ok_or(anyhow!("IDLE state has no Notify object"))
            });

        // Build an appropriate response
        match maybe_stop {
            Ok(stop) => ResponseOrIdle::IdleAccept(stop),
            Err(e) => {
                tracing::error!(err=?e, "unable to init idle due to a transition error");
                //ResponseOrIdle::IdleReject(tag)
                let no = Response::build()
                    .tag(tag)
                    .message(
                        "Internal error, processing command triggered an illegal IMAP state transition",
                    )
                    .no()
                    .unwrap();
                ResponseOrIdle::IdleReject(no)
            }
        }
    }

    pub async fn idle_poll(&mut self) -> ResponseOrIdle {
        match self.idle_poll_happy().await {
            Ok(r) => r,
            Err(e) => {
                tracing::error!(err=?e, "something bad happened in idle");
                ResponseOrIdle::Response(Response::bye().unwrap())
            }
        }
    }

    pub async fn idle_poll_happy(&mut self) -> Result<ResponseOrIdle> {
        let (mbx, tag, stop) = match &mut self.state {
            flow::State::Idle(_, ref mut mbx, _, tag, stop) => (mbx, tag.clone(), stop.clone()),
            _ => bail!("Invalid session state, can't idle"),
        };

        tokio::select! {
            _ = stop.notified() => {
                self.state.apply(flow::Transition::UnIdle)?;
                return Ok(ResponseOrIdle::Response(Response::build()
                    .tag(tag.clone())
                    .message("IDLE completed")
                    .ok()?))
            },
            change = mbx.idle_sync() => {
                tracing::debug!("idle event");
                return Ok(ResponseOrIdle::IdleEvent(change?));
            }
        }
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
        ResponseOrIdle::Response(resp)

        /*match &self.state {
            flow::State::Idle(_, _, _, _, n) => ResponseOrIdle::StartIdle(n.clone()),
            _ => ResponseOrIdle::Response(resp),
        }*/
    }
}
