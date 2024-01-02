use crate::imap::command::{anonymous, authenticated, examined, selected};
use crate::imap::flow;
use crate::imap::response::Response;
use crate::login::ArcLoginProvider;
use imap_codec::imap_types::command::Command;

//-----
pub struct Instance {
    pub login_provider: ArcLoginProvider,
    pub state: flow::State,
}
impl Instance {
    pub fn new(login_provider: ArcLoginProvider) -> Self {
        Self {
            login_provider,
            state: flow::State::NotAuthenticated,
        }
    }

    pub async fn command(&mut self, cmd: Command<'static>) -> Response<'static> {
        // Command behavior is modulated by the state.
        // To prevent state error, we handle the same command in separate code paths.
        let (resp, tr) = match &mut self.state {
            flow::State::NotAuthenticated => {
                let ctx = anonymous::AnonymousContext {
                    req: &cmd,
                    login_provider: &self.login_provider,
                };
                anonymous::dispatch(ctx).await
            }
            flow::State::Authenticated(ref user) => {
                let ctx = authenticated::AuthenticatedContext { req: &cmd, user };
                authenticated::dispatch(ctx).await
            }
            flow::State::Selected(ref user, ref mut mailbox) => {
                let ctx = selected::SelectedContext {
                    req: &cmd,
                    user,
                    mailbox,
                };
                selected::dispatch(ctx).await
            }
            flow::State::Examined(ref user, ref mut mailbox) => {
                let ctx = examined::ExaminedContext {
                    req: &cmd,
                    user,
                    mailbox,
                };
                examined::dispatch(ctx).await
            }
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
            return Response::build()
                .to_req(&cmd)
                .message(
                    "Internal error, processing command triggered an illegal IMAP state transition",
                )
                .bad()
                .unwrap();
        }

        resp
    }
}
