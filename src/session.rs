use std::sync::Arc;

use boitalettres::errors::Error as BalError;
use boitalettres::proto::{Request, Response};
use futures::future::BoxFuture;
use futures::future::FutureExt;
use imap_codec::types::command::CommandBody;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{mpsc, oneshot};

use crate::command;
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

pub struct Manager {
    tx: mpsc::Sender<Message>,
}

//@FIXME we should garbage collect the Instance when the Manager is destroyed.
impl Manager {
    pub fn new(login_provider: Arc<dyn LoginProvider + Send + Sync>) -> Self {
        let (tx, rx) = mpsc::channel(MAX_PIPELINED_COMMANDS);
        tokio::spawn(async move {
            let mut instance = Instance::new(login_provider, rx);
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
                return async { Response::bad("Too fast! Send less pipelined requests!") }.boxed()
            }
            Err(TrySendError::Closed(_)) => {
                return async { Response::bad("The session task has exited") }.boxed()
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

pub struct User {
    pub name: String,
    pub creds: Credentials,
}

pub struct Instance {
    rx: mpsc::Receiver<Message>,

    pub login_provider: Arc<dyn LoginProvider + Send + Sync>,
    pub selected: Option<Mailbox>,
    pub user: Option<User>,
}
impl Instance {
    fn new(
        login_provider: Arc<dyn LoginProvider + Send + Sync>,
        rx: mpsc::Receiver<Message>,
    ) -> Self {
        Self {
            login_provider,
            rx,
            selected: None,
            user: None,
        }
    }

    //@FIXME add a function that compute the runner's name from its local info
    // to ease debug
    // fn name(&self) -> String { }

    async fn start(&mut self) {
        //@FIXME add more info about the runner
        tracing::debug!("starting runner");

        while let Some(msg) = self.rx.recv().await {
            let mut cmd = command::Command::new(msg.req.tag, self);
            let res = match msg.req.body {
                CommandBody::Capability => cmd.capability().await,
                CommandBody::Login { username, password } => cmd.login(username, password).await,
                CommandBody::Lsub {
                    reference,
                    mailbox_wildcard,
                } => cmd.lsub(reference, mailbox_wildcard).await,
                CommandBody::List {
                    reference,
                    mailbox_wildcard,
                } => cmd.list(reference, mailbox_wildcard).await,
                CommandBody::Select { mailbox } => cmd.select(mailbox).await,
                CommandBody::Fetch {
                    sequence_set,
                    attributes,
                    uid,
                } => cmd.fetch(sequence_set, attributes, uid).await,
                _ => Response::bad("Error in IMAP command received by server.")
                    .map_err(anyhow::Error::new),
            };

            let wrapped_res = res.or_else(|e| match e.downcast::<BalError>() {
                Ok(be) => Err(be),
                Err(ae) => {
                    tracing::warn!(error=%ae, "internal.error");
                    Response::bad("Internal error")
                }
            });

            //@FIXME I think we should quit this thread on error and having our manager watch it,
            // and then abort the session as it is corrupted.
            msg.tx.send(wrapped_res).unwrap_or_else(|e| {
                tracing::warn!("failed to send imap response to manager: {:#?}", e)
            });
        }

        //@FIXME add more info about the runner
        tracing::debug!("exiting runner");
    }
}
