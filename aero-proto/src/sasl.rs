use std::net::SocketAddr;

use anyhow::{anyhow, bail, Result};
use futures::stream::{FuturesUnordered, StreamExt};
use tokio::io::BufStream;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio_util::bytes::BytesMut;

use aero_sasl::{decode::client_command, encode::Encode, flow::State};
use aero_user::config::AuthConfig;
use aero_user::login::ArcLoginProvider;

pub struct AuthServer {
    login_provider: ArcLoginProvider,
    bind_addr: SocketAddr,
}

impl AuthServer {
    pub fn new(config: AuthConfig, login_provider: ArcLoginProvider) -> Self {
        Self {
            bind_addr: config.bind_addr,
            login_provider,
        }
    }

    pub async fn run(self: Self, mut must_exit: watch::Receiver<bool>) -> Result<()> {
        let tcp = TcpListener::bind(self.bind_addr).await?;
        tracing::info!(
            "SASL Authentication Protocol listening on {:#}",
            self.bind_addr
        );

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

            tracing::info!("AUTH: accepted connection from {}", remote_addr);
            let conn = tokio::spawn(
                NetLoop::new(socket, self.login_provider.clone(), must_exit.clone()).run_error(),
            );

            connections.push(conn);
        }
        drop(tcp);

        tracing::info!("AUTH server shutting down, draining remaining connections...");
        while connections.next().await.is_some() {}

        Ok(())
    }
}

struct NetLoop {
    login: ArcLoginProvider,
    stream: BufStream<TcpStream>,
    stop: watch::Receiver<bool>,
    state: State,
    read_buf: Vec<u8>,
    write_buf: BytesMut,
}

impl NetLoop {
    fn new(stream: TcpStream, login: ArcLoginProvider, stop: watch::Receiver<bool>) -> Self {
        Self {
            login,
            stream: BufStream::new(stream),
            state: State::Init,
            stop,
            read_buf: Vec::new(),
            write_buf: BytesMut::new(),
        }
    }

    async fn run_error(self) {
        match self.run().await {
            Ok(()) => tracing::info!("Auth session succeeded"),
            Err(e) => tracing::error!(err=?e, "Auth session failed"),
        }
    }

    async fn run(mut self) -> Result<()> {
        loop {
            tokio::select! {
                read_res = self.stream.read_until(b'\n', &mut self.read_buf) => {
                    // Detect EOF / socket close
                    let bread = read_res?;
                    if bread == 0 {
                        tracing::info!("Reading buffer empty, connection has been closed. Exiting AUTH session.");
                        return Ok(())
                    }

                    // Parse command
                    let (_, cmd) = client_command(&self.read_buf).map_err(|_| anyhow!("Unable to parse command"))?;
                    tracing::trace!(cmd=?cmd, "Received command");

                    // Make some progress in our local state
                    let login = async |user: String, pass: String| self.login.login(user.as_str(), pass.as_str()).await.is_ok();
                    self.state.progress(cmd, login).await;
                    if matches!(self.state, State::Error) {
                        bail!("Internal state is in error, previous logs explain what went wrong");
                    }

                    // Build response
                    let srv_cmds = self.state.response();
                    srv_cmds.iter().try_for_each(|r| {
                        tracing::trace!(cmd=?r, "Sent command");
                        r.encode(&mut self.write_buf)
                    })?;

                    // Send responses if at least one command response has been generated
                    if !srv_cmds.is_empty() {
                        self.stream.write_all(&self.write_buf).await?;
                        self.stream.flush().await?;
                    }

                    // Reset buffers
                    self.read_buf.clear();
                    self.write_buf.clear();
                },
                _ = self.stop.changed() => {
                    tracing::debug!("Server is stopping, quitting this runner");
                    return Ok(())
                }
            }
        }
    }
}
