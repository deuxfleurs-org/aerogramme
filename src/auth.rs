use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use futures::stream::{FuturesUnordered, StreamExt};
use tokio::io::BufStream;
use tokio::io::AsyncBufReadExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;

use crate::config::AuthConfig;
use crate::login::ArcLoginProvider;

/// Seek compatibility with the Dovecot Authentication Protocol
///
/// ## Trace
///
/// ```text
/// S: VERSION	1	2
/// S: MECH	PLAIN	plaintext
/// S: MECH	LOGIN	plaintext
/// S: SPID	15
/// S: CUID	17654
/// S: COOKIE	f56692bee41f471ed01bd83520025305
/// S: DONE
/// C: VERSION	1	2
/// C: CPID	1
/// C: AUTH	2	PLAIN	service=smtp	
/// S: CONT	2	
/// C: CONT	2   base64stringFollowingRFC4616==	
/// S: OK	2	user=alice@example.tld
/// ```
///
/// ## RFC References
///
/// PLAIN SASL - https://datatracker.ietf.org/doc/html/rfc4616
/// 
///
/// ## Dovecot References
///
/// https://doc.dovecot.org/developer_manual/design/auth_protocol/
/// https://doc.dovecot.org/configuration_manual/authentication/authentication_mechanisms/#authentication-authentication-mechanisms
/// https://doc.dovecot.org/configuration_manual/howto/simple_virtual_install/#simple-virtual-install-smtp-auth
/// https://doc.dovecot.org/configuration_manual/howto/postfix_and_dovecot_sasl/#howto-postfix-and-dovecot-sasl
pub struct AuthServer {
    login_provider: ArcLoginProvider,
    bind_addr: SocketAddr,
}


impl AuthServer {
    pub fn new(
        config: AuthConfig, 
        login_provider: ArcLoginProvider,
    ) -> Self {
        Self {
            bind_addr: config.bind_addr,
            login_provider,
        }
    }


    pub async fn run(self: &Arc<Self>, mut must_exit: watch::Receiver<bool>) -> Result<()> {
        let tcp = TcpListener::bind(self.bind_addr).await?;
        tracing::info!("SASL Authentication Protocol listening on {:#}", self.bind_addr);

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
            let conn = tokio::spawn(NetLoop::new(socket).run());


            connections.push(conn);
        }
        drop(tcp);

        tracing::info!("AUTH server shutting down, draining remaining connections...");
        while connections.next().await.is_some() {}

        Ok(())
    }
}

struct NetLoop {
    stream: BufStream<TcpStream>,
}

impl NetLoop {
    fn new(stream: TcpStream) -> Self{
        Self {
            stream: BufStream::new(stream),
        }
    }

    async fn run(self) -> Result<()> {
        let mut lines = self.stream.lines();
        while let Some(line) = lines.next_line().await? {
        }

        Ok(())
    }
}

#[derive(Debug)]
enum Mechanism {
    Plain,
    Login,
}


#[derive(Debug)]
enum AuthOptions {
    /// Unique session ID. Mainly used for logging.
    Session(u64),
    /// Local IP connected to by the client. In standard string format, e.g. 127.0.0.1 or ::1.
    LocalIp(String),
    /// Remote client IP
    RemoteIp(String),
    /// Local port connected to by the client.
    LocalPort(u16),
    /// Remote client port
    RemotePort(u16),
    /// When Dovecot proxy is used, the real_rip/real_port are the proxy’s IP/port and real_lip/real_lport are the backend’s IP/port where the proxy was connected to.
    RealRemoteIp(String), 
    RealLocalIp(String), 
    RealLocalPort(u16), 
    RealRemotePort(u16),
    /// TLS SNI name
    LocalName(String),
    /// Enable debugging for this lookup.
    Debug,
    /// List of fields that will become available via %{forward_*} variables. The list is double-tab-escaped, like: tab_escaped[tab_escaped(key=value)[<TAB>...]
    /// Note: we do not unescape the tabulation, and thus we don't parse the data
    ForwardViews(Vec<u8>),
    /// Remote user has secured transport to auth client (e.g. localhost, SSL, TLS).
    Secured(String),
    /// The value can be “insecure”, “trusted” or “TLS”.
    Transport(String),
    /// TLS cipher being used.
    TlsCipher(String),
    /// The number of bits in the TLS cipher.
    /// @FIXME: I don't know how if it's a string or an integer
    TlsCipherBits(String),
    /// TLS perfect forward secrecy algorithm (e.g. DH, ECDH)
    TlsPfs(String),
    /// TLS protocol name (e.g. SSLv3, TLSv1.2)
    TlsProtocol(String),
    /// Remote user has presented a valid SSL certificate.
    ValidClientCert(String),
    /// Ignore auth penalty tracking for this request
    NoPenalty,
    /// Username taken from client’s SSL certificate.
    CertUsername,
    /// IMAP ID string
    ClientId,
    /// Initial response for authentication mechanism. 
    /// NOTE: This must be the last parameter. Everything after it is ignored. 
    /// This is to avoid accidental security holes if user-given data is directly put to base64 string without filtering out tabs.
    /// @FIXME: I don't understand this parameter
    Resp(Vec<u8>),
}

#[derive(Debug)]
enum ClientCommands {
    /// Both client and server should check that they support the same major version number. If they don’t, the other side isn’t expected to be talking the same protocol and should be disconnected. Minor version can be ignored. This document specifies the version number 1.2.
    Version {
        major: u64,
        minor: u64,
    },
    /// CPID finishes the handshake from client.
    Cpid(u64),
    Auth {
        /// ID is a connection-specific unique request identifier. It must be a 32bit number, so typically you’d just increment it by one.
        id: u64,
        /// A SASL mechanism (eg. LOGIN, PLAIN, etc.)
        /// See: https://doc.dovecot.org/configuration_manual/authentication/authentication_mechanisms/#authentication-authentication-mechanisms
        mechanism: Mechanism,
        /// Service is the service requesting authentication, eg. pop3, imap, smtp.
        service: String,
        /// All the optional parameters
        options: Vec<AuthOptions>,

    },
    Cont {
        /// The <id> must match the <id> of the AUTH command.
        id: u64,
        /// Data that will be serialized to / deserialized from base64
        data: Vec<u8>,
    }
}

#[derive(Debug)]
enum MechanismParameters {
    /// Anonymous authentication
    Anonymous,
    /// Transfers plaintext passwords
    PlainText,
    /// Subject to passive (dictionary) attack
    Dictionary,
    /// Subject to active (non-dictionary) attack
    Active,
    /// Provides forward secrecy between sessions
    ForwardSecrecy,
    /// Provides mutual authentication
    MutualAuth,
    /// Don’t advertise this as available SASL mechanism (eg. APOP)
    Private,
}

#[derive(Debug)]
enum FailCode {
    /// This is a temporary internal failure, e.g. connection was lost to SQL database.
    TempFail,
    /// Authentication succeeded, but authorization failed (master user’s password was ok, but destination user was not ok).
    AuthzFail,
    /// User is disabled (password may or may not have been correct)
    UserDisabled,
    /// User’s password has expired.
    PassExpired,
}

#[derive(Debug)]
enum ServerCommands {
    /// Both client and server should check that they support the same major version number. If they don’t, the other side isn’t expected to be talking the same protocol and should be disconnected. Minor version can be ignored. This document specifies the version number 1.2.
    Version {
        major: u64,
        minor: u64,
    },
    /// CPID and SPID specify client and server Process Identifiers (PIDs). They should be unique identifiers for the specific process. UNIX process IDs are good choices.
    /// SPID can be used by authentication client to tell master which server process handled the authentication.
    Spid(u64),
    /// CUID is a server process-specific unique connection identifier. It’s different each time a connection is established for the server.
    /// CUID is currently useful only for APOP authentication.
    Cuid(u64),
    Mech {
        kind: Mechanism,
        parameters: Vec<MechanismParameters>,
    },
    /// COOKIE returns connection-specific 128 bit cookie in hex. It must be given to REQUEST command. (Protocol v1.1+ / Dovecot v2.0+)
    Cookie([u8;16]),
    /// DONE finishes the handshake from server. 
    Done,

    Fail {
        id: u64,
        user_id: Option<String>,
        code: FailCode,
    },
    Cont {
        id: u64,
        data: Vec<u8>,
    },
    /// FAIL and OK may contain multiple unspecified parameters which authentication client may handle specially. 
    /// The only one specified here is user=<userid> parameter, which should always be sent if the userid is known.
    Ok {
        id: u64,
        user_id: Option<String>,
        parameters: Vec<u8>,
    },
}
