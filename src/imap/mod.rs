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

use anyhow::{bail, Result};
use futures::stream::{FuturesUnordered, StreamExt};

use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::sync::watch;

use imap_codec::imap_types::response::{Code, CommandContinuationRequest, Response, Status};
use imap_codec::imap_types::{core::Text, response::Greeting};
use imap_flow::server::{ServerFlow, ServerFlowEvent, ServerFlowOptions};
use imap_flow::stream::AnyStream;
use rustls_pemfile::{certs, private_key};
use tokio_rustls::TlsAcceptor;

use crate::config::{ImapConfig, ImapUnsecureConfig};
use crate::imap::capability::ServerCapability;
use crate::imap::request::Request;
use crate::imap::response::{Body, ResponseOrIdle};
use crate::imap::session::Instance;
use crate::login::ArcLoginProvider;

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
use tokio_util::bytes::BytesMut;

const PIPELINABLE_COMMANDS: usize = 64;

#[derive(Debug)]
enum LoopMode {
    Quit,
    Interactive,
    Idle(BytesMut, Arc<Notify>),
}

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

        let nl = match Self::new(ctx, sock).await {
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

    async fn core(mut self) -> Result<()> {
        tracing::trace!("Starting the core loop");
        let mut mode = LoopMode::Interactive;
        loop {
            tracing::trace!(mode=?mode, "Core loop iter");
            mode = match mode {
                LoopMode::Interactive => self.interactive_mode().await?,
                LoopMode::Idle(buff, stop) => self.idle_mode(buff, stop).await?,
                LoopMode::Quit => break,
            }
        }
        Ok(())
    }

    async fn interactive_mode(&mut self) -> Result<LoopMode> {
        tokio::select! {
            // Managing imap_flow stuff
            srv_evt = self.server.progress() =>  match srv_evt? {
                ServerFlowEvent::ResponseSent { handle: _handle, response } => {
                    match response {
                        Response::Status(Status::Bye(_)) => return Ok(LoopMode::Quit),
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
                Some(ResponseOrIdle::StartIdle(stop)) => {
                    tracing::trace!("Interactive, server agreed to switch in idle mode");
                    let cr = CommandContinuationRequest::basic(None, "Idling")?;
                    self.server.enqueue_continuation(cr);
                    self.cmd_tx.try_send(Request::Idle)?;
                    return Ok(LoopMode::Idle(BytesMut::new(), stop))
                },
                None => {
                    self.server.enqueue_status(Status::bye(None, "Internal session exited").unwrap());
                    tracing::error!("session task exited for {:?}, quitting", self.ctx.addr);
                },
                Some(_) => unreachable!(),

            },

            // When receiving a CTRL+C
            _ = self.ctx.must_exit.changed() => {
                tracing::trace!("Interactive, CTRL+C, exiting");
                self.server.enqueue_status(Status::bye(None, "Server is being shutdown").unwrap());
            },
        };
        Ok(LoopMode::Interactive)
    }

    async fn idle_mode(&mut self, mut buff: BytesMut, stop: Arc<Notify>) -> Result<LoopMode> {
        // Flush send
        loop {
            tracing::trace!("flush server send");
            match self.server.progress_send().await? {
                Some(..) => continue,
                None => break,
            }
        }

        tokio::select! {
            // Receiving IDLE event from background
            maybe_msg = self.resp_rx.recv() => match maybe_msg {
                // Session decided idle is terminated
                Some(ResponseOrIdle::Response(response)) => {
                    tracing::trace!("server imap session said idle is done, sending response done, switching to interactive");
                    for body_elem in response.body.into_iter() {
                        let _handle = match body_elem {
                            Body::Data(d) => self.server.enqueue_data(d),
                            Body::Status(s) => self.server.enqueue_status(s),
                        };
                    }
                    self.server.enqueue_status(response.completion);
                    return Ok(LoopMode::Interactive)
                },
                // Session has some information for user
                Some(ResponseOrIdle::IdleEvent(elems)) => {
                    tracing::trace!("server imap session has some change to communicate to the client");
                    for body_elem in elems.into_iter() {
                        let _handle = match body_elem {
                            Body::Data(d) => self.server.enqueue_data(d),
                            Body::Status(s) => self.server.enqueue_status(s),
                        };
                    }
                    self.cmd_tx.try_send(Request::Idle)?;
                    return Ok(LoopMode::Idle(buff, stop))
                },

                // Session crashed
                None => {
                    self.server.enqueue_status(Status::bye(None, "Internal session exited").unwrap());
                    tracing::error!("session task exited for {:?}, quitting", self.ctx.addr);
                    return Ok(LoopMode::Interactive)
                },

                // Session can't start idling while already idling, it's a logic error!
                Some(ResponseOrIdle::StartIdle(..)) => bail!("can't start idling while already idling!"),
            },

            // User is trying to interact with us
            read_client_result = self.server.stream.read(&mut buff) =>  {
                let _bytes_read = read_client_result?;
                use imap_codec::decode::Decoder;
                let codec = imap_codec::IdleDoneCodec::new();
                tracing::trace!("client sent some data for the server IMAP session");
                match codec.decode(&buff) {
                    Ok(([], imap_codec::imap_types::extensions::idle::IdleDone)) => {
                        // Session will be informed that it must stop idle
                        // It will generate the "done" message and change the loop mode
                        tracing::trace!("client sent DONE and want to stop IDLE");
                        stop.notify_one()
                    },
                    Err(_) => {
                        tracing::trace!("Unable to decode DONE, maybe not enough data were sent?");
                    },
                    _ => bail!("Client sent data after terminating the continuation without waiting for the server. This is an unsupported behavior and bug in Aerogramme, quitting."),
                };

                return Ok(LoopMode::Idle(buff, stop))
            },

            // When receiving a CTRL+C
            _ = self.ctx.must_exit.changed() => {
                tracing::trace!("CTRL+C sent, aborting IDLE for this session");
                self.server.enqueue_status(Status::bye(None, "Server is being shutdown").unwrap());
                return Ok(LoopMode::Interactive)
            },
        };
    }
}
