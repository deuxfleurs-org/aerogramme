use anyhow::Error;
use boitalettres::errors::Error as BalError;
use boitalettres::proto::{Request, Response};
use futures::future::BoxFuture;
use futures::future::FutureExt;

use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{mpsc, oneshot};

use crate::imap::command::{anonymous, authenticated, examined, selected};
use crate::imap::flow;
use crate::login::ArcLoginProvider;

/* This constant configures backpressure in the system,
 * or more specifically, how many pipelined messages are allowed
 * before refusing them
 */
const MAX_PIPELINED_COMMANDS: usize = 10;

struct Message {
    req: Request,
    tx: oneshot::Sender<Result<Response, BalError>>,
}

//-----

pub struct Manager {
    tx: mpsc::Sender<Message>,
}

impl Manager {
    pub fn new(login_provider: ArcLoginProvider) -> Self {
        let (tx, rx) = mpsc::channel(MAX_PIPELINED_COMMANDS);
        tokio::spawn(async move {
            let instance = Instance::new(login_provider, rx);
            instance.start().await;
        });
        Self { tx }
    }

    pub fn process(&self, req: Request) -> BoxFuture<'static, Result<Response, BalError>> {
        let (tx, rx) = oneshot::channel();
        let msg = Message { req, tx };

        // We use try_send on a bounded channel to protect the daemons from DoS.
        // Pipelining requests in IMAP are a special case: they should not occure often
        // and in a limited number (like 3 requests). Someone filling the channel
        // will probably be malicious so we "rate limit" them.
        match self.tx.try_send(msg) {
            Ok(()) => (),
            Err(TrySendError::Full(_)) => {
                return async { Response::bad("Too fast! Send less pipelined requests.") }.boxed()
            }
            Err(TrySendError::Closed(_)) => {
                return async { Err(BalError::Text("Terminated session".to_string())) }.boxed()
            }
        };

        // @FIXME add a timeout, handle a session that fails.
        async {
            match rx.await {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!("Got error {:#?}", e);
                    Response::bad("No response from the session handler")
                }
            }
        }
        .boxed()
    }
}

//-----

pub struct Instance {
    rx: mpsc::Receiver<Message>,

    pub login_provider: ArcLoginProvider,
    pub state: flow::State,
}
impl Instance {
    fn new(login_provider: ArcLoginProvider, rx: mpsc::Receiver<Message>) -> Self {
        Self {
            login_provider,
            rx,
            state: flow::State::NotAuthenticated,
        }
    }

    //@FIXME add a function that compute the runner's name from its local info
    // to ease debug
    // fn name(&self) -> String { }

    async fn start(mut self) {
        //@FIXME add more info about the runner
        tracing::debug!("starting runner");

        while let Some(msg) = self.rx.recv().await {
            // Command behavior is modulated by the state.
            // To prevent state error, we handle the same command in separate code paths.
            let ctrl = match &mut self.state {
                flow::State::NotAuthenticated => {
                    let ctx = anonymous::AnonymousContext {
                        req: &msg.req,
                        login_provider: Some(&self.login_provider),
                    };
                    anonymous::dispatch(ctx).await
                }
                flow::State::Authenticated(ref user) => {
                    let ctx = authenticated::AuthenticatedContext {
                        req: &msg.req,
                        user,
                    };
                    authenticated::dispatch(ctx).await
                }
                flow::State::Selected(ref user, ref mut mailbox) => {
                    let ctx = selected::SelectedContext {
                        req: &msg.req,
                        user,
                        mailbox,
                    };
                    selected::dispatch(ctx).await
                }
                flow::State::Examined(ref user, ref mut mailbox) => {
                    let ctx = examined::ExaminedContext {
                        req: &msg.req,
                        user,
                        mailbox,
                    };
                    examined::dispatch(ctx).await
                }
                flow::State::Logout => {
                    Response::bad("No commands are allowed in the LOGOUT state.")
                        .map(|r| (r, flow::Transition::None))
                        .map_err(Error::msg)
                }
            };

            // Process result
            let res = match ctrl {
                Ok((res, tr)) => {
                    //@FIXME remove unwrap
                    self.state = match self.state.apply(tr) {
                        Ok(new_state) => new_state,
                        Err(e) => {
                            tracing::error!("Invalid transition: {}, exiting", e);
                            break;
                        }
                    };

                    //@FIXME enrich here the command with some global status

                    Ok(res)
                }
                // Cast from anyhow::Error to Bal::Error
                // @FIXME proper error handling would be great
                Err(e) => match e.downcast::<BalError>() {
                    Ok(be) => Err(be),
                    Err(e) => {
                        tracing::warn!(error=%e, "internal.error");
                        Response::bad("Internal error")
                    }
                },
            };

            //@FIXME I think we should quit this thread on error and having our manager watch it,
            // and then abort the session as it is corrupted.
            msg.tx.send(res).unwrap_or_else(|e| {
                tracing::warn!("failed to send imap response to manager: {:#?}", e)
            });

            if let flow::State::Logout = &self.state {
                break;
            }
        }

        //@FIXME add more info about the runner
        tracing::debug!("exiting runner");
    }
}
