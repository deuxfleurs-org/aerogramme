use std::net::SocketAddr;
use std::{pin::Pin, sync::Arc};

use anyhow::Result;
use async_trait::async_trait;
use duplexify::Duplex;
use futures::{io, AsyncRead, AsyncReadExt, AsyncWrite};
use futures::{
    stream,
    stream::{FuturesOrdered, FuturesUnordered},
    StreamExt,
};
use log::*;
use tokio::net::TcpListener;
use tokio::select;
use tokio::sync::watch;
use tokio_util::compat::*;

use smtp_message::{DataUnescaper, Email, EscapedDataReader, Reply, ReplyCode};
use smtp_server::{reply, Config, ConnectionMetadata, Decision, MailMetadata};

use crate::config::*;
use crate::login::*;
use crate::mail::incoming::EncryptedMessage;

pub struct LmtpServer {
    bind_addr: SocketAddr,
    hostname: String,
    login_provider: Arc<dyn LoginProvider + Send + Sync>,
}

impl LmtpServer {
    pub fn new(
        config: LmtpConfig,
        login_provider: Arc<dyn LoginProvider + Send + Sync>,
    ) -> Arc<Self> {
        Arc::new(Self {
            bind_addr: config.bind_addr,
            hostname: config.hostname,
            login_provider,
        })
    }

    pub async fn run(self: &Arc<Self>, mut must_exit: watch::Receiver<bool>) -> Result<()> {
        let tcp = TcpListener::bind(self.bind_addr).await?;
        info!("LMTP server listening on {:#}", self.bind_addr);

        let mut connections = FuturesUnordered::new();

        while !*must_exit.borrow() {
            let wait_conn_finished = async {
                if connections.is_empty() {
                    futures::future::pending().await
                } else {
                    connections.next().await
                }
            };
            let (socket, remote_addr) = select! {
                a = tcp.accept() => a?,
                _ = wait_conn_finished => continue,
                _ = must_exit.changed() => continue,
            };
            info!("LMTP: accepted connection from {}", remote_addr);

            let conn = tokio::spawn(smtp_server::interact(
                socket.compat(),
                smtp_server::IsAlreadyTls::No,
                (),
                self.clone(),
            ));

            connections.push(conn);
        }
        drop(tcp);

        info!("LMTP server shutting down, draining remaining connections...");
        while connections.next().await.is_some() {}

        Ok(())
    }
}

// ----

pub struct Message {
    to: Vec<PublicCredentials>,
}

#[async_trait]
impl Config for LmtpServer {
    type Protocol = smtp_server::protocol::Lmtp;

    type ConnectionUserMeta = ();
    type MailUserMeta = Message;

    fn hostname(&self, _conn_meta: &ConnectionMetadata<()>) -> &str {
        &self.hostname
    }

    async fn new_mail(&self, _conn_meta: &mut ConnectionMetadata<()>) -> Message {
        Message { to: vec![] }
    }

    async fn tls_accept<IO>(
        &self,
        _io: IO,
        _conn_meta: &mut ConnectionMetadata<()>,
    ) -> io::Result<Duplex<Pin<Box<dyn Send + AsyncRead>>, Pin<Box<dyn Send + AsyncWrite>>>>
    where
        IO: Send + AsyncRead + AsyncWrite,
    {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "TLS not implemented for LMTP server",
        ))
    }

    async fn filter_from(
        &self,
        from: Option<Email>,
        _meta: &mut MailMetadata<Message>,
        _conn_meta: &mut ConnectionMetadata<()>,
    ) -> Decision<Option<Email>> {
        Decision::Accept {
            reply: reply::okay_from().convert(),
            res: from,
        }
    }

    async fn filter_to(
        &self,
        to: Email,
        meta: &mut MailMetadata<Message>,
        _conn_meta: &mut ConnectionMetadata<()>,
    ) -> Decision<Email> {
        let to_str = match to.hostname.as_ref() {
            Some(h) => format!("{}@{}", to.localpart, h),
            None => to.localpart.to_string(),
        };
        match self.login_provider.public_login(&to_str).await {
            Ok(creds) => {
                meta.user.to.push(creds);
                Decision::Accept {
                    reply: reply::okay_to().convert(),
                    res: to,
                }
            }
            Err(e) => Decision::Reject {
                reply: Reply {
                    code: ReplyCode::POLICY_REASON,
                    ecode: None,
                    text: vec![smtp_message::MaybeUtf8::Utf8(e.to_string())],
                },
            },
        }
    }

    async fn handle_mail<'resp, R>(
        &'resp self,
        reader: &mut EscapedDataReader<'_, R>,
        meta: MailMetadata<Message>,
        _conn_meta: &'resp mut ConnectionMetadata<()>,
    ) -> Pin<Box<dyn futures::Stream<Item = Decision<()>> + Send + 'resp>>
    where
        R: Send + Unpin + AsyncRead,
    {
        let err_response_stream = |meta: MailMetadata<Message>, msg: String| {
            Box::pin(
                stream::iter(meta.user.to.into_iter()).map(move |_| Decision::Reject {
                    reply: Reply {
                        code: ReplyCode::POLICY_REASON,
                        ecode: None,
                        text: vec![smtp_message::MaybeUtf8::Utf8(msg.clone())],
                    },
                }),
            )
        };

        let mut text = Vec::new();
        if let Err(e) = reader.read_to_end(&mut text).await {
            return err_response_stream(meta, format!("io error: {}", e));
        }
        reader.complete();
        let raw_size = text.len();

        // Unescape email, shrink it also to remove last dot
        let unesc_res = DataUnescaper::new(true).unescape(&mut text);
        text.truncate(unesc_res.written);
        tracing::debug!(prev_sz = raw_size, new_sz = text.len(), "unescaped");

        let encrypted_message = match EncryptedMessage::new(text) {
            Ok(x) => Arc::new(x),
            Err(e) => return err_response_stream(meta, e.to_string()),
        };

        Box::pin(
            meta.user
                .to
                .into_iter()
                .map(move |creds| {
                    let encrypted_message = encrypted_message.clone();
                    async move {
                        match encrypted_message.deliver_to(creds).await {
                            Ok(()) => Decision::Accept {
                                reply: reply::okay_mail().convert(),
                                res: (),
                            },
                            Err(e) => Decision::Reject {
                                reply: Reply {
                                    code: ReplyCode::POLICY_REASON,
                                    ecode: None,
                                    text: vec![smtp_message::MaybeUtf8::Utf8(e.to_string())],
                                },
                            },
                        }
                    }
                })
                .collect::<FuturesOrdered<_>>(),
        )
    }
}
