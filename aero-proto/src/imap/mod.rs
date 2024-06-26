mod attributes;
mod capability;
mod command;
mod flags;
mod flow;
mod imf_view;
mod index;
mod mail_view;
mod mailbox_view;
mod mime_view;
mod request;
mod response;
mod search;
mod session;

use std::net::SocketAddr;

use anyhow::{anyhow, bail, Result};
use futures::stream::{FuturesUnordered, StreamExt};
use imap_codec::imap_types::response::{Code, CommandContinuationRequest, Response, Status};
use imap_codec::imap_types::{core::Text, response::Greeting};
use imap_flow::server::{ServerFlow, ServerFlowEvent, ServerFlowOptions};
use imap_flow::stream::AnyStream;
use rustls_pemfile::{certs, private_key};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::sync::watch;
use tokio_rustls::TlsAcceptor;

use aero_user::config::{ImapConfig, ImapUnsecureConfig};
use aero_user::login::ArcLoginProvider;

use crate::imap::capability::ServerCapability;
use crate::imap::request::Request;
use crate::imap::response::{Body, ResponseOrIdle};
use crate::imap::session::Instance;

/// Server is a thin wrapper to register our Services in BàL
pub struct Server {
    bind_addr: SocketAddr,
    login_provider: ArcLoginProvider,
    capabilities: ServerCapability,
    tls: Option<TlsAcceptor>,
}

#[derive(Clone)]
struct ClientContext {
    addr: SocketAddr,
    login_provider: ArcLoginProvider,
    must_exit: watch::Receiver<bool>,
    server_capabilities: ServerCapability,
}

pub fn new(config: ImapConfig, login: ArcLoginProvider) -> Result<Server> {
    let loaded_certs = certs(&mut std::io::BufReader::new(std::fs::File::open(
        config.certs,
    )?))
    .collect::<Result<Vec<_>, _>>()?;
    let loaded_key = private_key(&mut std::io::BufReader::new(std::fs::File::open(
        config.key,
    )?))?
    .unwrap();

    let tls_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(loaded_certs, loaded_key)?;
    let acceptor = TlsAcceptor::from(Arc::new(tls_config));

    Ok(Server {
        bind_addr: config.bind_addr,
        login_provider: login,
        capabilities: ServerCapability::default(),
        tls: Some(acceptor),
    })
}

pub fn new_unsecure(config: ImapUnsecureConfig, login: ArcLoginProvider) -> Server {
    Server {
        bind_addr: config.bind_addr,
        login_provider: login,
        capabilities: ServerCapability::default(),
        tls: None,
    }
}

impl Server {
    pub async fn run(self: Self, mut must_exit: watch::Receiver<bool>) -> Result<()> {
        let tcp = TcpListener::bind(self.bind_addr).await?;
        tracing::info!("IMAP server listening on {:#}", self.bind_addr);

        let mut connections = FuturesUnordered::new();

        while !*must_exit.borrow() {
            let wait_conn_finished = async {
                if connections.is_empty() {
                    futures::future::pending().await
                } else {
                    connections.next().await
                }
            };
            let (socket, remote_addr) = tokio::select! {
                a = tcp.accept() => a?,
                _ = wait_conn_finished => continue,
                _ = must_exit.changed() => continue,
            };
            tracing::info!("IMAP: accepted connection from {}", remote_addr);
            let stream = match self.tls.clone() {
                Some(acceptor) => {
                    let stream = match acceptor.accept(socket).await {
                        Ok(v) => v,
                        Err(e) => {
                            tracing::error!(err=?e, "TLS negociation failed");
                            continue;
                        }
                    };
                    AnyStream::new(stream)
                }
                None => AnyStream::new(socket),
            };

            let client = ClientContext {
                addr: remote_addr.clone(),
                login_provider: self.login_provider.clone(),
                must_exit: must_exit.clone(),
                server_capabilities: self.capabilities.clone(),
            };
            let conn = tokio::spawn(NetLoop::handler(client, stream));
            connections.push(conn);
        }
        drop(tcp);

        tracing::info!("IMAP server shutting down, draining remaining connections...");
        while connections.next().await.is_some() {}

        Ok(())
    }
}

use std::sync::Arc;
use tokio::sync::mpsc::*;
use tokio::sync::Notify;

const PIPELINABLE_COMMANDS: usize = 64;

// @FIXME a full refactor of this part of the code will be needed sooner or later
struct NetLoop {
    ctx: ClientContext,
    server: ServerFlow,
    cmd_tx: Sender<Request>,
    resp_rx: UnboundedReceiver<ResponseOrIdle>,
}

impl NetLoop {
    async fn handler(ctx: ClientContext, sock: AnyStream) {
        let addr = ctx.addr.clone();

        let mut nl = match Self::new(ctx, sock).await {
            Ok(nl) => {
                tracing::debug!(addr=?addr, "netloop successfully initialized");
                nl
            }
            Err(e) => {
                tracing::error!(addr=?addr, err=?e, "netloop can not be initialized, closing session");
                return;
            }
        };

        match nl.core().await {
            Ok(()) => {
                tracing::debug!("closing successful netloop core for {:?}", addr);
            }
            Err(e) => {
                tracing::error!("closing errored netloop core for {:?}: {}", addr, e);
            }
        }
    }

    async fn new(ctx: ClientContext, sock: AnyStream) -> Result<Self> {
        let mut opts = ServerFlowOptions::default();
        opts.crlf_relaxed = false;
        opts.literal_accept_text = Text::unvalidated("OK");
        opts.literal_reject_text = Text::unvalidated("Literal rejected");

        // Send greeting
        let (server, _) = ServerFlow::send_greeting(
            sock,
            opts,
            Greeting::ok(
                Some(Code::Capability(ctx.server_capabilities.to_vec())),
                "Aerogramme",
            )
            .unwrap(),
        )
        .await?;

        // Start a mailbox session in background
        let (cmd_tx, cmd_rx) = mpsc::channel::<Request>(PIPELINABLE_COMMANDS);
        let (resp_tx, resp_rx) = mpsc::unbounded_channel::<ResponseOrIdle>();
        tokio::spawn(Self::session(ctx.clone(), cmd_rx, resp_tx));

        // Return the object
        Ok(NetLoop {
            ctx,
            server,
            cmd_tx,
            resp_rx,
        })
    }

    /// Coms with the background session
    async fn session(
        ctx: ClientContext,
        mut cmd_rx: Receiver<Request>,
        resp_tx: UnboundedSender<ResponseOrIdle>,
    ) -> () {
        let mut session = Instance::new(ctx.login_provider, ctx.server_capabilities);
        loop {
            let cmd = match cmd_rx.recv().await {
                None => break,
                Some(cmd_recv) => cmd_recv,
            };

            tracing::debug!(cmd=?cmd, sock=%ctx.addr, "command");
            let maybe_response = session.request(cmd).await;
            tracing::debug!(cmd=?maybe_response, sock=%ctx.addr, "response");

            match resp_tx.send(maybe_response) {
                Err(_) => break,
                Ok(_) => (),
            };
        }
        tracing::info!("runner is quitting");
    }

    async fn core(&mut self) -> Result<()> {
        let mut maybe_idle: Option<Arc<Notify>> = None;
        loop {
            tokio::select! {
                // Managing imap_flow stuff
                srv_evt = self.server.progress() =>  match srv_evt? {
                    ServerFlowEvent::ResponseSent { handle: _handle, response } => {
                        match response {
                            Response::Status(Status::Bye(_)) => return Ok(()),
                            _ => tracing::trace!("sent to {} content {:?}", self.ctx.addr, response),
                        }
                    },
                    ServerFlowEvent::CommandReceived { command } => {
                        match self.cmd_tx.try_send(Request::ImapCommand(command)) {
                            Ok(_) => (),
                            Err(mpsc::error::TrySendError::Full(_)) => {
                                self.server.enqueue_status(Status::bye(None, "Too fast").unwrap());
                                tracing::error!("client {:?} is sending commands too fast, closing.", self.ctx.addr);
                            }
                            _ => {
                                self.server.enqueue_status(Status::bye(None, "Internal session exited").unwrap());
                                tracing::error!("session task exited for {:?}, quitting", self.ctx.addr);
                            }
                        }
                    },
                    ServerFlowEvent::IdleCommandReceived { tag } => {
                        match self.cmd_tx.try_send(Request::IdleStart(tag)) {
                            Ok(_) => (),
                            Err(mpsc::error::TrySendError::Full(_)) => {
                                self.server.enqueue_status(Status::bye(None, "Too fast").unwrap());
                                tracing::error!("client {:?} is sending commands too fast, closing.", self.ctx.addr);
                            }
                            _ => {
                                self.server.enqueue_status(Status::bye(None, "Internal session exited").unwrap());
                                tracing::error!("session task exited for {:?}, quitting", self.ctx.addr);
                            }
                        }
                    }
                    ServerFlowEvent::IdleDoneReceived => {
                        tracing::trace!("client sent DONE and want to stop IDLE");
                        maybe_idle.ok_or(anyhow!("Received IDLE done but not idling currently"))?.notify_one();
                        maybe_idle = None;
                    }
                    flow => {
                        self.server.enqueue_status(Status::bye(None, "Unsupported server flow event").unwrap());
                        tracing::error!("session task exited for {:?} due to unsupported flow {:?}", self.ctx.addr, flow);
                    }
                },

                // Managing response generated by Aerogramme
                maybe_msg = self.resp_rx.recv() => match maybe_msg {
                    Some(ResponseOrIdle::Response(response)) => {
                        tracing::trace!("Interactive, server has a response for the client");
                        for body_elem in response.body.into_iter() {
                            let _handle = match body_elem {
                                Body::Data(d) => self.server.enqueue_data(d),
                                Body::Status(s) => self.server.enqueue_status(s),
                            };
                        }
                        self.server.enqueue_status(response.completion);
                    },
                    Some(ResponseOrIdle::IdleAccept(stop)) => {
                        tracing::trace!("Interactive, server agreed to switch in idle mode");
                        let cr = CommandContinuationRequest::basic(None, "Idling")?;
                        self.server.idle_accept(cr).or(Err(anyhow!("refused continuation for idle accept")))?;
                        self.cmd_tx.try_send(Request::IdlePoll)?;
                        if maybe_idle.is_some() {
                            bail!("Can't start IDLE if already idling");
                        }
                        maybe_idle = Some(stop);
                    },
                    Some(ResponseOrIdle::IdleEvent(elems)) => {
                        tracing::trace!("server imap session has some change to communicate to the client");
                        for body_elem in elems.into_iter() {
                            let _handle = match body_elem {
                                Body::Data(d) => self.server.enqueue_data(d),
                                Body::Status(s) => self.server.enqueue_status(s),
                            };
                        }
                        self.cmd_tx.try_send(Request::IdlePoll)?;
                    },
                    Some(ResponseOrIdle::IdleReject(response)) => {
                        tracing::trace!("inform client that session rejected idle");
                        self.server
                            .idle_reject(response.completion)
                            .or(Err(anyhow!("wrong reject command")))?;
                    },
                    None => {
                        self.server.enqueue_status(Status::bye(None, "Internal session exited").unwrap());
                        tracing::error!("session task exited for {:?}, quitting", self.ctx.addr);
                    },
                },

                // When receiving a CTRL+C
                _ = self.ctx.must_exit.changed() => {
                    tracing::trace!("Interactive, CTRL+C, exiting");
                    self.server.enqueue_status(Status::bye(None, "Server is being shutdown").unwrap());
                },
            };
        }
    }
}
