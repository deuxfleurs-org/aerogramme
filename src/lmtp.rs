use std::collections::HashMap;
use std::net::SocketAddr;
use std::{pin::Pin, sync::Arc};

use anyhow::Result;
use async_trait::async_trait;
use duplexify::Duplex;
use futures::{io, AsyncRead, AsyncReadExt, AsyncWrite};
use futures::{stream, stream::FuturesUnordered, StreamExt};
use log::*;
use rusoto_s3::{PutObjectRequest, S3};
use tokio::net::TcpListener;
use tokio::select;
use tokio::sync::watch;
use tokio_util::compat::*;

use smtp_message::{Email, EscapedDataReader, Reply, ReplyCode};
use smtp_server::{reply, Config, ConnectionMetadata, Decision, MailMetadata};

use crate::config::*;
use crate::cryptoblob::*;
use crate::login::*;
use crate::mail::mail_ident::*;

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

            let conn = tokio::spawn(smtp_server::interact(
                socket.compat(),
                smtp_server::IsAlreadyTls::No,
                Conn { remote_addr },
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

pub struct Conn {
    remote_addr: SocketAddr,
}

pub struct Message {
    to: Vec<PublicCredentials>,
}

#[async_trait]
impl Config for LmtpServer {
    type Protocol = smtp_server::protocol::Lmtp;

    type ConnectionUserMeta = Conn;
    type MailUserMeta = Message;

    fn hostname(&self, _conn_meta: &ConnectionMetadata<Conn>) -> &str {
        &self.hostname
    }

    async fn new_mail(&self, _conn_meta: &mut ConnectionMetadata<Conn>) -> Message {
        Message { to: vec![] }
    }

    async fn tls_accept<IO>(
        &self,
        _io: IO,
        _conn_meta: &mut ConnectionMetadata<Conn>,
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
        _conn_meta: &mut ConnectionMetadata<Conn>,
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
        _conn_meta: &mut ConnectionMetadata<Conn>,
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
        _conn_meta: &'resp mut ConnectionMetadata<Conn>,
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
        if reader.read_to_end(&mut text).await.is_err() {
            return err_response_stream(meta, "io error".into());
        }
        reader.complete();

        let encrypted_message = match EncryptedMessage::new(text) {
            Ok(x) => Arc::new(x),
            Err(e) => return err_response_stream(meta, e.to_string()),
        };

        Box::pin(stream::iter(meta.user.to.into_iter()).then(move |creds| {
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
        }))
    }
}

// ----

struct EncryptedMessage {
    key: Key,
    encrypted_body: Vec<u8>,
}

impl EncryptedMessage {
    fn new(body: Vec<u8>) -> Result<Self> {
        let key = gen_key();
        let encrypted_body = seal(&body, &key)?;
        Ok(Self {
            key,
            encrypted_body,
        })
    }

    async fn deliver_to(self: Arc<Self>, creds: PublicCredentials) -> Result<()> {
        let s3_client = creds.storage.s3_client()?;

        let encrypted_key =
            sodiumoxide::crypto::sealedbox::seal(self.key.as_ref(), &creds.public_key);
        let key_header = base64::encode(&encrypted_key);

        let mut por = PutObjectRequest::default();
        por.bucket = creds.storage.bucket.clone();
        por.key = format!("incoming/{}", gen_ident().to_string());
        por.metadata = Some(
            [("Message-Key".to_string(), key_header)]
                .into_iter()
                .collect::<HashMap<_, _>>(),
        );
        por.body = Some(self.encrypted_body.clone().into());
        s3_client.put_object(por).await?;

        Ok(())
    }
}
