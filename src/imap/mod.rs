mod command;
mod flow;
mod mailbox_view;
mod response;
mod session;

use std::net::SocketAddr;

use anyhow::Result;
use futures::stream::{FuturesUnordered, StreamExt};

use tokio::net::TcpListener;
use tokio::sync::watch;

use imap_codec::imap_types::response::Greeting;
use imap_flow::server::{ServerFlow, ServerFlowEvent, ServerFlowOptions};
use imap_flow::stream::AnyStream;

use crate::config::ImapConfig;
use crate::login::ArcLoginProvider;

/// Server is a thin wrapper to register our Services in BàL
pub struct Server {
    bind_addr: SocketAddr,
    login_provider: ArcLoginProvider,
}

struct ClientContext {
    stream: AnyStream,
    addr: SocketAddr,
    login_provider: ArcLoginProvider,
    must_exit: watch::Receiver<bool>,
}

pub fn new(config: ImapConfig, login: ArcLoginProvider) -> Server {
    Server {
        bind_addr: config.bind_addr,
        login_provider: login,
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

            let client = ClientContext {
                stream: AnyStream::new(socket),
                addr: remote_addr.clone(),
                login_provider: self.login_provider.clone(),
                must_exit: must_exit.clone(),
            };
            let conn = tokio::spawn(client_wrapper(client));
            connections.push(conn);
        }
        drop(tcp);

        tracing::info!("IMAP server shutting down, draining remaining connections...");
        while connections.next().await.is_some() {}

        Ok(())
    }
}

async fn client_wrapper(ctx: ClientContext) {
    let addr = ctx.addr.clone();
    match client(ctx).await {
        Ok(()) => {
            tracing::info!("closing successful session for {:?}", addr);
        }
        Err(e) => {
            tracing::error!("closing errored session for {:?}: {}", addr, e);
        }
    }
}

async fn client(mut ctx: ClientContext) -> Result<()> {
    // Send greeting
    let (mut server, _) = ServerFlow::send_greeting(
        ctx.stream,
        ServerFlowOptions::default(),
        Greeting::ok(None, "Aerogramme").unwrap(),
    )
    .await?;

    use crate::imap::response::{Body, Response as MyResponse};
    use crate::imap::session::Instance;
    use imap_codec::imap_types::command::Command;
    use imap_codec::imap_types::response::{Response, Status};

    use tokio::sync::mpsc;
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<Command<'static>>(10);
    let (resp_tx, mut resp_rx) = mpsc::unbounded_channel::<MyResponse<'static>>();

    let bckgrnd = tokio::spawn(async move {
        let mut session = Instance::new(ctx.login_provider);
        loop {
            let cmd = match cmd_rx.recv().await {
                None => break,
                Some(cmd_recv) => cmd_recv,
            };

            let maybe_response = session.command(cmd).await;

            match resp_tx.send(maybe_response) {
                Err(_) => break,
                Ok(_) => (),
            };
        }
        tracing::info!("runner is quitting");
    });

    // Main loop
    loop {
        tokio::select! {
            // Managing imap_flow stuff
            srv_evt = server.progress() =>  match srv_evt? {
                ServerFlowEvent::ResponseSent { handle: _handle, response } => {
                    match response {
                        Response::Status(Status::Bye(_)) => break,
                        _ => tracing::trace!("sent to {} content {:?}", ctx.addr, response),
                    }
                },
                ServerFlowEvent::CommandReceived { command } => {
                    match cmd_tx.try_send(command) {
                        Ok(_) => (),
                        Err(mpsc::error::TrySendError::Full(_)) => {
                            server.enqueue_status(Status::bye(None, "Too fast").unwrap());
                            tracing::error!("client {:?} is sending commands too fast, closing.", ctx.addr);
                        }
                        _ => {
                            server.enqueue_status(Status::bye(None, "Internal session exited").unwrap());
                            tracing::error!("session task exited for {:?}, quitting", ctx.addr);
                        }
                    }
                },
            },

            // Managing response generated by Aerogramme
            maybe_msg = resp_rx.recv() => {
                let response = match maybe_msg {
                    None => {
                        server.enqueue_status(Status::bye(None, "Internal session exited").unwrap());
                        tracing::error!("session task exited for {:?}, quitting", ctx.addr);
                        continue
                    },
                    Some(r) => r,
                };

                for body_elem in response.body.into_iter() {
                    let _handle = match body_elem {
                        Body::Data(d) => server.enqueue_data(d),
                        Body::Status(s) => server.enqueue_status(s),
                    };
                }
                server.enqueue_status(response.completion);
            },

            // When receiving a CTRL+C
            _ = ctx.must_exit.changed() => {
                server.enqueue_status(Status::bye(None, "Server is being shutdown").unwrap());
            },
        };
    }

    drop(cmd_tx);
    bckgrnd.await?;
    Ok(())
}
