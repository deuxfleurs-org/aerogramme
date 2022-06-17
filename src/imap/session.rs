use std::sync::Arc;

use anyhow::Error;
use boitalettres::errors::Error as BalError;
use boitalettres::proto::{Request, Response};
use futures::future::BoxFuture;
use futures::future::FutureExt;
use imap_codec::types::command::CommandBody;
use imap_codec::types::response::{Capability, Code, Data, Response as ImapRes, Status};
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{mpsc, oneshot};

use crate::command::{anonymous,authenticated,selected};
use crate::login::Credentials;
use crate::mailbox::Mailbox;
use crate::LoginProvider;

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
            let mut instance = Instance::new(login_provider, rx);
            instance.start().await;
        });
        Self { tx }
    }

    pub fn process(&self, req: Request) -> BoxFuture<'static, Result<Response, BalError>> {
        let (tx, rx) = oneshot::channel();
        let tag = req.tag.clone();
        let msg = Message { req, tx };

        // We use try_send on a bounded channel to protect the daemons from DoS.
        // Pipelining requests in IMAP are a special case: they should not occure often
        // and in a limited number (like 3 requests). Someone filling the channel
        // will probably be malicious so we "rate limit" them.
        match self.tx.try_send(msg) {
            Ok(()) => (),
            Err(TrySendError::Full(_)) => {
                return async {
                    Status::bad(Some(tag), None, "Too fast! Send less pipelined requests!")
                        .map(|s| vec![ImapRes::Status(s)])
                        .map_err(|e| BalError::Text(e.to_string()))
                }
                .boxed()
            }
            Err(TrySendError::Closed(_)) => {
                return async {
                    Status::bad(Some(tag), None, "The session task has exited")
                        .map(|s| vec![ImapRes::Status(s)])
                        .map_err(|e| BalError::Text(e.to_string()))
                }
                .boxed()
            }
        };

        // @FIXME add a timeout, handle a session that fails.
        async {
            match rx.await {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!("Got error {:#?}", e);
                    Status::bad(Some(tag), None, "No response from the session handler")
                        .map(|s| vec![ImapRes::Status(s)])
                        .map_err(|e| BalError::Text(e.to_string()))
                }
            }
        }
        .boxed()
    }
}

//-----

pub struct Context<'a> {
    req: &'a Request,
    state: &'a mut flow::State,
    login: ArcLoginProvider,
}

pub struct Instance {
    rx: mpsc::Receiver<Message>,

    pub login_provider: ArcLoginProvider,
    pub state: flow::State,
}
impl Instance {
    fn new(
        login_provider: ArcLoginProvider,
        rx: mpsc::Receiver<Message>,
    ) -> Self {
        Self {
            login_provider,
            rx,
            state: flow::State::NotAuthenticated,
        }
    }

    //@FIXME add a function that compute the runner's name from its local info
    // to ease debug
    // fn name(&self) -> String { }

    async fn start(&mut self) {
        //@FIXME add more info about the runner
        tracing::debug!("starting runner");

        while let Some(msg) = self.rx.recv().await {
            let ctx = Context { req: &msg.req, state: &mut self.state, login: self.login_provider };

            // Command behavior is modulated by the state.
            // To prevent state error, we handle the same command in separate code path depending
            // on the State.
            let cmd_res =  match self.state {
                flow::State::NotAuthenticated => anonymous::dispatch(ctx).await, 
                flow::State::Authenticated(user) => authenticated::dispatch(ctx).await,
                flow::State::Selected(user, mailbox) => selected::dispatch(ctx).await,
                flow::State::Logout => Status::bad(Some(ctx.req.tag.clone()), None, "No commands are allowed in the LOGOUT state.")
                    .map(|s| vec![ImapRes::Status(s)])
                    .map_err(Error::msg),
            };

/*

                match req.body {
                    CommandBody::Capability => anonymous::capability().await,
                    CommandBody::Login { username, password } => anonymous::login(self.login_provider, username, password).await.and_then(|(user, response)| {
                        self.state.authenticate(user)?;
                        Ok(response)
                    },
                    _ => Status::no(Some(msg.req.tag.clone()), None, "This command is not available in the ANONYMOUS state.")
                        .map(|s| vec![ImapRes::Status(s)])
                        .map_err(Error::msg),

                },
                flow::State::Authenticated(user) => match req.body {
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
                flow::State::Selected(user, mailbox) => match req.body {
                    CommandBody::Capability => anonymous::capability().await, // we use the same implem for now
                    CommandBody::Fetch { sequence_set, attributes, uid, } => selected::fetch(sequence_set, attributes, uid).await,
                    _ => Status::no(Some(msg.req.tag.clone()), None, "This command is not available in the SELECTED state.")
                        .map(|s| vec![ImapRes::Status(s)])
                        .map_err(Error::msg),
                },
                flow::State::Logout => Status::bad(Some(msg.req.tag.clone()), None, "No commands are allowed in the LOGOUT state.")
                    .map(|s| vec![ImapRes::Status(s)])
                    .map_err(Error::msg),
            }
            */

            let imap_res = match cmd_res {
                Ok(new_state, imap_res) => {
                    self.state = new_state;
                    Ok(imap_res)
                },
                Err(e) if Ok(be) = e.downcast::<BalError>() => Err(be),
                Err(e) => {
                    tracing::warn!(error=%e, "internal.error");
                    Ok(Status::bad(Some(msg.req.tag.clone()), None, "Internal error")
                        .map(|s| vec![ImapRes::Status(s)])
                        .map_err(|e| BalError::Text(e.to_string())))
                }
            };

            //@FIXME I think we should quit this thread on error and having our manager watch it,
            // and then abort the session as it is corrupted.
            msg.tx.send(imap_res).unwrap_or_else(|e| {
                tracing::warn!("failed to send imap response to manager: {:#?}", e)
            });
        }

        //@FIXME add more info about the runner
        tracing::debug!("exiting runner");
    }
}
