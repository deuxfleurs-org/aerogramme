use std::sync::Arc;

use boitalettres::proto::{Request, Response};
use boitalettres::errors::Error as BalError;
use imap_codec::types::command::CommandBody;
use tokio::sync::{oneshot,mpsc};
use tokio::sync::mpsc::error::TrySendError;
use futures::future::BoxFuture;
use futures::future::FutureExt;

use crate::command;
use crate::login::Credentials;
use crate::mailstore::Mailstore;
use crate::mailbox::Mailbox;

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
    pub fn new(mailstore: Arc<Mailstore>) -> Self {
        let (tx, mut rx) = mpsc::channel(MAX_PIPELINED_COMMANDS);
        tokio::spawn(async move { 
            let mut instance = Instance::new(mailstore, rx);
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
            Err(TrySendError::Full(_)) => return async { Response::bad("Too fast! Send less pipelined requests!") }.boxed(),
            Err(TrySendError::Closed(_)) => return async { Response::bad("The session task has exited") }.boxed(),
        };

        // @FIXME add a timeout, handle a session that fails.
        async {
            match rx.await {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!("Got error {:#?}", e);
                    Response::bad("No response from the session handler")
                },
            }
        }.boxed()
    }
}

pub struct Instance {
    rx: mpsc::Receiver<Message>,

    pub mailstore: Arc<Mailstore>, 
    pub creds: Option<Credentials>,
    pub selected: Option<Mailbox>,
    pub username: Option<String>,
}
impl Instance {
    fn new(mailstore: Arc<Mailstore>, rx: mpsc::Receiver<Message>) -> Self {
        Self { mailstore, rx, creds: None, selected: None, username: None, }
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
                CommandBody::Lsub { reference, mailbox_wildcard } => cmd.lsub(reference, mailbox_wildcard).await,
                CommandBody::List { reference, mailbox_wildcard } => cmd.list(reference, mailbox_wildcard).await,
                CommandBody::Select { mailbox } => cmd.select(mailbox).await,
                CommandBody::Fetch { sequence_set, attributes, uid } => cmd.fetch(sequence_set, attributes, uid).await,
                _ => Response::bad("Error in IMAP command received by server."),
            };

            //@FIXME I think we should quit this thread on error and having our manager watch it,
            // and then abort the session as it is corrupted.
            msg.tx.send(res).unwrap_or_else(|e| tracing::warn!("failed to send imap response to manager: {:#?}", e));
        }

        //@FIXME add more info about the runner
        tracing::debug!("exiting runner");
    }
}
