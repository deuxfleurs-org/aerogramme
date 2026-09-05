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

use std::collections::HashSet;
use std::net::SocketAddr;

use anyhow::{anyhow, bail, Result};
use futures::stream::{FuturesUnordered, StreamExt};
use imap_codec::imap_types::response::{Code, CommandContinuationRequest, Status};
use imap_codec::imap_types::{core::Text, response::Greeting};
use imap_flow::server::{ServerFlow, ServerFlowEvent, ServerFlowOptions, ServerFlowResponseHandle};
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

impl Server {
    pub fn new(config: ImapConfig, login: ArcLoginProvider) -> Result<Self> {
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
    
        Ok(Self {
            bind_addr: config.bind_addr,
            login_provider: login,
            capabilities: ServerCapability::default(),
            tls: Some(acceptor),
        })
    }
    
    pub fn new_unsecure(config: ImapUnsecureConfig, login: ArcLoginProvider) -> Self {
        Self {
            bind_addr: config.bind_addr,
            login_provider: login,
            capabilities: ServerCapability::default(),
            tls: None,
        }
    }

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
    pending_responses: HashSet<ServerFlowResponseHandle>,
    shutting_down: bool,
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
            pending_responses: HashSet::new(),
            shutting_down: false,
        })
    }

    /// Coms with the background session
    ///
    /// This function receives imap commands and processes them (running
    /// aerogramme's core imap business logic). We want this to be done
    /// concurrently from the main loop in `core()` which handles low-level
    /// request/responses at the IMAP protocol-level: `session()` runs in a
    /// separate tokio task and communicates with `core()` using channels.
    async fn session(
        ctx: ClientContext,
        mut cmd_rx: Receiver<Request>,
        resp_tx: UnboundedSender<ResponseOrIdle>,
    ) -> () {
        let mut session = Instance::new(ctx.login_provider, ctx.server_capabilities);
        loop {
            // Exit the session after entering LOGOUT state. We propagate
            // whether to include an additional BYE status message. In some
            // cases (the LOGOUT command) a BYE was already sent as part of the
            // command; in other cases (commands that trigger an immediate
            // shutdown) an extra BYE is needed.
            if let flow::State::Logout { needs_bye } = session.state {
                tracing::debug!(sock=%ctx.addr, "entered LOGOUT state, closing session");
                let _ = resp_tx.send(ResponseOrIdle::CloseSession { needs_bye });
                break
            }

            // `recv()` and `send()` on channels can only return `None` if the
            // whole NetLoop has exited, which means the session has shutdown
            // and we just need to exit this task.
            let cmd = match cmd_rx.recv().await {
                None => break,
                Some(cmd_recv) => cmd_recv,
            };
            tracing::debug!(cmd=?cmd, sock=%ctx.addr, "command");
            let response = session.request(cmd).await;
            tracing::debug!(cmd=?response, sock=%ctx.addr, "response");

            match resp_tx.send(response) {
                Err(_) => break,
                Ok(_) => (),
            };
        }
        tracing::info!("runner is quitting");
        // we drop our channel handles when quitting, which closes the other end
        // of the channel and signals the netloop to terminate the client
        // connection.
    }

    /// Send BYE and initiate shutdown. Used internally by `core()` to account
    /// for low-level error cases. Shutdown events that corresponds to normal
    /// "imap business logic" are first initiated by `session()` sending a
    /// `CloseSession` response to `core()`.
    fn shutdown(&mut self, msg: &'static str) {
        let handle = self.server.enqueue_status(Status::bye(None, msg).unwrap());
        self.pending_responses.insert(handle);
        self.shutting_down = true;
    }

    async fn core(&mut self) -> Result<()> {
        let mut maybe_idle: Option<Arc<Notify>> = None;
        loop {
            if self.shutting_down && self.pending_responses.is_empty() {
                return Ok(())
            }

            tokio::select! {
                // Managing imap_flow stuff
                srv_evt = self.server.progress() =>  match srv_evt? {
                    ServerFlowEvent::ResponseSent { handle, response } => {
                        tracing::trace!("sent to {} content {:?}", self.ctx.addr, response);
                        self.pending_responses.remove(&handle);
                    },
                    ServerFlowEvent::CommandReceived { command } => {
                        match self.cmd_tx.try_send(Request::ImapCommand(command)) {
                            Ok(_) => (),
                            Err(mpsc::error::TrySendError::Full(_)) => {
                                self.shutdown("Too fast");
                                tracing::error!("client {:?} is sending commands too fast, closing.", self.ctx.addr);
                            }
                            Err(mpsc::error::TrySendError::Closed(_)) => {
                                self.shutdown("Internal session exited");
                                tracing::error!("session task exited for {:?}, quitting", self.ctx.addr);
                            }
                        }
                    },
                    ServerFlowEvent::IdleCommandReceived { tag } => {
                        match self.cmd_tx.try_send(Request::IdleStart(tag)) {
                            Ok(_) => (),
                            Err(mpsc::error::TrySendError::Full(_)) => {
                                self.shutdown("Too fast");
                                tracing::error!("client {:?} is sending commands too fast, closing.", self.ctx.addr);
                            }
                            Err(mpsc::error::TrySendError::Closed(_)) => {
                                self.shutdown("Internal session exited");
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
                        self.shutdown("Unsupported server flow event");
                        tracing::error!("session task exited for {:?} due to unsupported flow {:?}", self.ctx.addr, flow);
                    }
                },

                // Managing response generated by Aerogramme
                maybe_msg = self.resp_rx.recv(), if !self.shutting_down => match maybe_msg {
                    Some(ResponseOrIdle::CloseSession { needs_bye }) => {
                        tracing::trace!("Closing session as required");
                        if needs_bye {
                            self.shutdown("bye");
                        }
                        self.shutting_down = true;
                    },
                    Some(ResponseOrIdle::Response(response)) => {
                        tracing::trace!("Interactive, server has a response for the client");
                        for body_elem in response.body.into_iter() {
                            let handle = match body_elem {
                                Body::Data(d) => self.server.enqueue_data(d),
                                Body::Status(s) => self.server.enqueue_status(s),
                            };
                            self.pending_responses.insert(handle);
                        }
                        let handle = self.server.enqueue_status(response.completion);
                        self.pending_responses.insert(handle);
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
                            let handle = match body_elem {
                                Body::Data(d) => self.server.enqueue_data(d),
                                Body::Status(s) => self.server.enqueue_status(s),
                            };
                            self.pending_responses.insert(handle);
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
                        tracing::info!("session task exited");
                        // The channel has been closed, which means the session
                        // task has exited and we are ongoing shutdown. The
                        // `session()` task already arranged for required BYE
                        // messages to be sent. There is nothing to do, continue
                        // to send pending responses before exiting.
                    },
                },

                // When receiving a CTRL+C
                _ = self.ctx.must_exit.changed() => {
                    tracing::trace!("Interactive, CTRL+C, exiting");
                    self.shutdown("Server is being shutdown");
                },
            };
        }
    }
}
