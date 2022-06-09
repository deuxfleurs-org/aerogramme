use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use anyhow::Result;
use boitalettres::server::accept::addr::AddrStream;
use boitalettres::errors::Error as BalError;
use boitalettres::proto::{Request, Response};
use futures::future::BoxFuture;
use futures::future::FutureExt;
use imap_codec::types::command::CommandBody;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;
use tower::Service;

use crate::command;
use crate::login::Credentials;
use crate::mailstore::Mailstore;
use crate::mailbox::Mailbox;

const MAX_PIPELINED_COMMANDS: usize = 10;

pub struct Instance {
    pub mailstore: Arc<Mailstore>,
}
impl Instance {
    pub fn new(mailstore: Arc<Mailstore>) -> Self {
        Self { mailstore }
    }
}
impl<'a> Service<&'a AddrStream> for Instance {
    type Response = Connection;
    type Error = anyhow::Error;
    type Future = BoxFuture<'static, Result<Self::Response>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, addr: &'a AddrStream) -> Self::Future {
        tracing::info!(remote_addr = %addr.remote_addr, local_addr = %addr.local_addr, "accept");
        let ms = self.mailstore.clone();
        Box::pin(async { Ok(Connection::new(ms)) })
    }
}

pub struct Connection {
    pub tx: mpsc::Sender<Request>,
}
impl Connection {
    pub fn new(mailstore: Arc<Mailstore>) -> Self {
        let (tx, mut rx) = mpsc::channel(MAX_PIPELINED_COMMANDS);
        tokio::spawn(async move { 
            let mut session = Session::new(mailstore, rx);
            session.run().await; 
        });
        Self { tx }
    }
}
impl Service<Request> for Connection {
    type Response = Response;
    type Error = BalError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request) -> Self::Future {
        tracing::debug!("Got request: {:#?}", req);
        match self.tx.try_send(req) {
            Ok(()) => return async { Response::ok("Ok") }.boxed(),
            Err(TrySendError::Full(_)) => return async { Response::bad("Too fast! Send less pipelined requests!") }.boxed(),
            Err(TrySendError::Closed(_)) => return async { Response::bad("The session task has exited") }.boxed(),
        }

        // send a future that await here later a oneshot command
    }
}

pub struct Session {
    pub mailstore: Arc<Mailstore>,
    pub creds: Option<Credentials>,
    pub selected: Option<Mailbox>,
    rx: mpsc::Receiver<Request>,
}

impl Session {
    pub fn new(mailstore: Arc<Mailstore>, rx: mpsc::Receiver<Request>) -> Self {
        Self { mailstore, rx, creds: None, selected: None, }
    }

    pub async fn run(&mut self) {
        while let Some(req) = self.rx.recv().await {
             let mut cmd = command::Command::new(req.tag, self);
             let _ = match req.body {
                CommandBody::Capability => cmd.capability().await,
                CommandBody::Login { username, password } => cmd.login(username, password).await,
                CommandBody::Lsub { reference, mailbox_wildcard } => cmd.lsub(reference, mailbox_wildcard).await,
                CommandBody::List { reference, mailbox_wildcard } => cmd.list(reference, mailbox_wildcard).await,
                CommandBody::Select { mailbox } => cmd.select(mailbox).await,
                CommandBody::Fetch { sequence_set, attributes, uid } => cmd.fetch(sequence_set, attributes, uid).await,
               _ => Response::bad("Error in IMAP command received by server."),
            };
        }
    }
}

