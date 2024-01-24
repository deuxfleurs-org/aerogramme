use std::net::SocketAddr;

use anyhow::{Result, anyhow, bail};
use futures::stream::{FuturesUnordered, StreamExt};
use tokio::io::BufStream;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
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
///
/// C: AUTH	2	PLAIN	service=smtp	
/// S: CONT	2	
/// C: CONT	2   base64stringFollowingRFC4616==	
/// S: OK	2	user=alice@example.tld
///
/// C: AUTH	42	LOGIN	service=smtp
/// S: CONT	42	VXNlcm5hbWU6
/// C: CONT	42	b64User
/// S: CONT	42	UGFzc3dvcmQ6
/// C: CONT	42	b64Pass
/// S: FAIL	42	user=alice
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


    pub async fn run(self: Self, mut must_exit: watch::Receiver<bool>) -> Result<()> {
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
            let conn = tokio::spawn(NetLoop::new(socket, self.login_provider.clone(), must_exit.clone()).run_error());


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
    stop:  watch::Receiver<bool>,
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
                    tracing::debug!(cmd=?cmd, "Received command");

                    // Make some progress in our local state
                    self.state.progress(cmd, &self.login).await;
                    if matches!(self.state, State::Error) {
                        bail!("Internal state is in error, previous logs explain what went wrong");
                    }

                    // Build response
                    let srv_cmds = self.state.response();
                    srv_cmds.iter().try_for_each(|r| r.encode(&mut self.write_buf))?;

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

// -----------------------------------------------------------------
//
// BUSINESS LOGIC
//
// -----------------------------------------------------------------
use rand::prelude::*;

#[derive(Debug)]
enum AuthRes {
    Success(String),
    Failed(Option<String>, Option<FailCode>),
}

#[derive(Debug)]
enum State {
    Error,
    Init,
    HandshakePart(Version),
    HandshakeDone,
    AuthPlainProgress {
        id: u64, 
    },
    AuthDone {
        id: u64,
        res: AuthRes
    },
}

const SERVER_MAJOR: u64 = 1;
const SERVER_MINOR: u64 = 2;
impl State {
    async fn progress(&mut self, cmd: ClientCommand, login: &ArcLoginProvider) {

        let new_state = 'state: {
            match (std::mem::replace(self, State::Error), cmd) {
                (Self::Init, ClientCommand::Version(v)) => Self::HandshakePart(v),
                (Self::HandshakePart(version), ClientCommand::Cpid(_cpid)) => {
                    if version.major != SERVER_MAJOR {
                        tracing::error!(client_major=version.major, server_major=SERVER_MAJOR, "Unsupported client major version");
                        break 'state Self::Error
                    }
             
                    Self::HandshakeDone
                },
                (Self::HandshakeDone { .. }, ClientCommand::Auth { id, mech, .. }) |
                    (Self::AuthDone { .. }, ClientCommand::Auth { id, mech, ..}) => {
                    if mech != Mechanism::Plain {
                        tracing::error!(mechanism=?mech, "Unsupported Authentication Mechanism");
                        break 'state Self::AuthDone { id, res: AuthRes::Failed(None, None) }
                    }

                    Self::AuthPlainProgress { id } 
                },
                (Self::AuthPlainProgress { id }, ClientCommand::Cont { id: cid, data }) => {
                    // Check that ID matches
                    if cid != id {
                        tracing::error!(auth_id=id, cont_id=cid, "CONT id does not match AUTH id");
                        break 'state Self::AuthDone { id, res: AuthRes::Failed(None, None) }
                    }

                    // Check that we can extract user's login+pass
                    let (ubin, pbin) = match auth_plain(&data) {
                        Ok(([], ([], user, pass))) => (user, pass),
                        Ok(_) => {
                            tracing::error!("Impersonating user is not supported");
                            break 'state Self::AuthDone { id, res: AuthRes::Failed(None, None) }
                        }
                        Err(e) => {
                            tracing::error!(err=?e, "Could not parse the SASL PLAIN data chunk");
                            break 'state Self::AuthDone { id, res: AuthRes::Failed(None, None) }
                        },
                    };

                    // Try to convert it to UTF-8
                    let (user, password) = match (std::str::from_utf8(ubin), std::str::from_utf8(pbin))  {
                        (Ok(u), Ok(p)) => (u, p),
                        _ => {
                            tracing::error!("Username or password contain invalid UTF-8 characters");
                            break 'state Self::AuthDone { id, res: AuthRes::Failed(None, None) }
                        }
                    };

                    // Try to connect user
                    match login.login(user, password).await {
                        Ok(_) => Self::AuthDone { id, res: AuthRes::Success(user.to_string())},
                        Err(e) => {
                            tracing::warn!(err=?e, "login failed");
                            Self::AuthDone { id, res: AuthRes::Failed(Some(user.to_string()), None) }
                        }
                    }
                },
                _ => {
                    tracing::error!("This command is not valid in this context");
                    Self::Error
                },
            }
        };
        tracing::debug!(state=?new_state, "Made progress");
        *self = new_state;
    }

    fn response(&self) -> Vec<ServerCommand> {
        let mut srv_cmd: Vec<ServerCommand> = Vec::new();

        match self {
            Self::HandshakeDone { .. } => {
                srv_cmd.push(ServerCommand::Version(Version { major: SERVER_MAJOR, minor: SERVER_MINOR }));
                srv_cmd.push(ServerCommand::Spid(1u64));
                srv_cmd.push(ServerCommand::Cuid(1u64));

                let mut cookie = [0u8; 16];
                thread_rng().fill(&mut cookie);
                srv_cmd.push(ServerCommand::Cookie(cookie));

                srv_cmd.push(ServerCommand::Mech {
                    kind: Mechanism::Plain,
                    parameters: vec![MechanismParameters::PlainText],
                });
                srv_cmd.push(ServerCommand::Done);
            },
            Self::AuthPlainProgress { id } => {
                srv_cmd.push(ServerCommand::Cont { id: *id, data: None });
            },
            Self::AuthDone { id, res: AuthRes::Success(user) } => {
                srv_cmd.push(ServerCommand::Ok { id: *id, user_id: Some(user.to_string()), extra_parameters: vec![]});
            },
            Self::AuthDone { id, res: AuthRes::Failed(maybe_user, maybe_failcode) } => {
                srv_cmd.push(ServerCommand::Fail { id: *id, user_id: maybe_user.clone(), code: maybe_failcode.clone(), extra_parameters: vec![]});
            },
            _ => (),
        };

        srv_cmd
    }
}


// -----------------------------------------------------------------
//
// DOVECOT AUTH TYPES
//
// -----------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Mechanism {
    Plain,
    Login,
}


#[derive(Clone, Debug)]
enum AuthOption {
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
    Secured(Option<String>),
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
    /// An unknown key
    UnknownPair(String, Vec<u8>),
    UnknownBool(Vec<u8>),
    /// Initial response for authentication mechanism. 
    /// NOTE: This must be the last parameter. Everything after it is ignored. 
    /// This is to avoid accidental security holes if user-given data is directly put to base64 string without filtering out tabs.
    /// @FIXME: I don't understand this parameter
    Resp(Vec<u8>),
}

#[derive(Debug, Clone)]
struct Version {
    major: u64,
    minor: u64,
}

#[derive(Debug)]
enum ClientCommand {
    /// Both client and server should check that they support the same major version number. If they don’t, the other side isn’t expected to be talking the same protocol and should be disconnected. Minor version can be ignored. This document specifies the version number 1.2.
    Version(Version),
    /// CPID finishes the handshake from client.
    Cpid(u64),
    Auth {
        /// ID is a connection-specific unique request identifier. It must be a 32bit number, so typically you’d just increment it by one.
        id: u64,
        /// A SASL mechanism (eg. LOGIN, PLAIN, etc.)
        /// See: https://doc.dovecot.org/configuration_manual/authentication/authentication_mechanisms/#authentication-authentication-mechanisms
        mech: Mechanism,
        /// Service is the service requesting authentication, eg. pop3, imap, smtp.
        service: String,
        /// All the optional parameters
        options: Vec<AuthOption>,

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

#[derive(Debug, Clone)]
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
enum ServerCommand {
    /// Both client and server should check that they support the same major version number. If they don’t, the other side isn’t expected to be talking the same protocol and should be disconnected. Minor version can be ignored. This document specifies the version number 1.2.
    Version(Version),
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
        code: Option<FailCode>,
        extra_parameters: Vec<Vec<u8>>,
    },
    Cont {
        id: u64,
        data: Option<Vec<u8>>,
    },
    /// FAIL and OK may contain multiple unspecified parameters which authentication client may handle specially. 
    /// The only one specified here is user=<userid> parameter, which should always be sent if the userid is known.
    Ok {
        id: u64,
        user_id: Option<String>,
        extra_parameters: Vec<Vec<u8>>,
    },
}

// -----------------------------------------------------------------
//
// DOVECOT AUTH DECODING
//
// ------------------------------------------------------------------

use nom::{
  IResult,
  branch::alt,
  error::{ErrorKind, Error},
  character::complete::{tab, u64, u16},
  bytes::complete::{is_not, tag, tag_no_case, take, take_while, take_while1},
  multi::{many1, separated_list0},
  combinator::{map, opt, recognize, value, rest},
  sequence::{pair, preceded, tuple},
};
use base64::Engine;

fn version_command<'a>(input: &'a [u8]) -> IResult<&'a [u8], ClientCommand> {
    let mut parser = tuple((
        tag_no_case(b"VERSION"),
        tab,
        u64,
        tab,
        u64
    ));

    let (input, (_, _, major, _, minor)) = parser(input)?;
    Ok((input, ClientCommand::Version(Version { major, minor })))
}

fn cpid_command<'a>(input: &'a [u8]) -> IResult<&'a [u8], ClientCommand> {
    preceded(
        pair(tag_no_case(b"CPID"), tab),
        map(u64, |v| ClientCommand::Cpid(v))
    )(input)
}

fn mechanism<'a>(input: &'a [u8]) -> IResult<&'a [u8], Mechanism> {
    alt((
        value(Mechanism::Plain, tag_no_case(b"PLAIN")),
        value(Mechanism::Login, tag_no_case(b"LOGIN")),
    ))(input)
}

fn is_not_tab_or_esc_or_lf(c: u8) -> bool {
    c != 0x09 && c != 0x01 && c != 0x0a // TAB or 0x01 or LF
}

fn is_esc<'a>(input: &'a [u8]) -> IResult<&'a [u8], &[u8]> {
    preceded(tag(&[0x01]), take(1usize))(input)
}

fn parameter<'a>(input: &'a [u8]) -> IResult<&'a [u8], &[u8]> {
    recognize(many1(alt((
        take_while1(is_not_tab_or_esc_or_lf),
        is_esc
    ))))(input)
}

fn parameter_str(input: &[u8]) -> IResult<&[u8], String> {
    let (input, buf) = parameter(input)?;

    std::str::from_utf8(buf)
        .map(|v| (input, v.to_string()))
        .map_err(|_| nom::Err::Failure(Error::new(input, ErrorKind::TakeWhile1)))
}

fn is_param_name_char(c: u8) -> bool {
    is_not_tab_or_esc_or_lf(c) && c != 0x3d // =
}

fn parameter_name(input: &[u8]) -> IResult<&[u8], String> {
    let (input, buf) = take_while1(is_param_name_char)(input)?;

    std::str::from_utf8(buf)
        .map(|v| (input, v.to_string()))
        .map_err(|_| nom::Err::Failure(Error::new(input, ErrorKind::TakeWhile1)))
}

fn service<'a>(input: &'a [u8]) -> IResult<&'a [u8], String> {
    preceded(
        tag_no_case("service="),
        parameter_str
    )(input)
}

fn auth_option<'a>(input: &'a [u8]) -> IResult<&'a [u8], AuthOption> {
    use AuthOption::*;
    alt((
        alt((
            value(Debug, tag_no_case(b"debug")),
            value(NoPenalty, tag_no_case(b"no-penalty")),
            value(ClientId, tag_no_case(b"client_id")),
            map(preceded(tag_no_case(b"session="), u64), |id| Session(id)),
            map(preceded(tag_no_case(b"lip="), parameter_str), |ip| LocalIp(ip)),
            map(preceded(tag_no_case(b"rip="), parameter_str), |ip| RemoteIp(ip)),
            map(preceded(tag_no_case(b"lport="), u16), |port| LocalPort(port)),
            map(preceded(tag_no_case(b"rport="), u16), |port| RemotePort(port)),
            map(preceded(tag_no_case(b"real_rip="), parameter_str), |ip| RealRemoteIp(ip)),
            map(preceded(tag_no_case(b"real_lip="), parameter_str), |ip| RealLocalIp(ip)),
            map(preceded(tag_no_case(b"real_lport="), u16), |port| RealLocalPort(port)),
            map(preceded(tag_no_case(b"real_rport="), u16), |port| RealRemotePort(port)),
        )),
        alt((
            map(preceded(tag_no_case(b"local_name="), parameter_str), |name| LocalName(name)),
            map(preceded(tag_no_case(b"forward_views="), parameter), |views| ForwardViews(views.into())),
            map(preceded(tag_no_case(b"secured="), parameter_str), |info| Secured(Some(info))),
            value(Secured(None), tag_no_case(b"secured")),
            value(CertUsername, tag_no_case(b"cert_username")),
            map(preceded(tag_no_case(b"transport="), parameter_str), |ts| Transport(ts)),
            map(preceded(tag_no_case(b"tls_cipher="), parameter_str), |cipher| TlsCipher(cipher)),
            map(preceded(tag_no_case(b"tls_cipher_bits="), parameter_str), |bits| TlsCipherBits(bits)),
            map(preceded(tag_no_case(b"tls_pfs="), parameter_str), |pfs| TlsPfs(pfs)),
            map(preceded(tag_no_case(b"tls_protocol="), parameter_str), |proto| TlsProtocol(proto)),
            map(preceded(tag_no_case(b"valid-client-cert="), parameter_str), |cert| ValidClientCert(cert)),
        )),
        alt((
            map(preceded(tag_no_case(b"resp="), base64), |data| Resp(data)),
            map(tuple((parameter_name, tag(b"="), parameter)), |(n, _, v)| UnknownPair(n, v.into())),
            map(parameter, |v| UnknownBool(v.into())),
        )),
    ))(input)
}

fn auth_command<'a>(input: &'a [u8]) -> IResult<&'a [u8], ClientCommand> {
    let mut parser = tuple((
        tag_no_case(b"AUTH"),
        tab,
        u64,
        tab,
        mechanism,
        tab,
        service,
        map(
            opt(preceded(tab, separated_list0(tab, auth_option))),
            |o| o.unwrap_or(vec![])
        ), 
    ));
    let (input, (_, _, id, _, mech, _, service, options)) = parser(input)?;
    Ok((input, ClientCommand::Auth { id, mech, service, options }))
}

fn is_base64_core(c: u8) -> bool {
    c >= 0x30 && c <= 0x39 // 0-9 
        || c >= 0x41 && c <= 0x5a // A-Z
        || c >= 0x61 && c <= 0x7a // a-z
        || c == 0x2b // +
        || c == 0x2f // /
}

fn is_base64_pad(c: u8) -> bool {
    c == 0x3d // =
}

fn base64(input: &[u8]) -> IResult<&[u8], Vec<u8>> {
    let (input, (b64, _)) = tuple((
        take_while1(is_base64_core),
        take_while(is_base64_pad),
    ))(input)?;

    let data = base64::engine::general_purpose::STANDARD_NO_PAD
        .decode(b64)
        .map_err(|_| nom::Err::Failure(Error::new(input, ErrorKind::TakeWhile1)))?;

    Ok((input, data))
}

/// @FIXME Dovecot does not say if base64 content must be padded or not
fn cont_command<'a>(input: &'a [u8]) ->  IResult<&'a [u8], ClientCommand> {
    let mut parser = tuple((
        tag_no_case(b"CONT"),
        tab,
        u64,
        tab,
        base64
    ));

    let (input, (_, _, id, _, data)) = parser(input)?;
    Ok((input, ClientCommand::Cont { id, data }))
}

fn client_command<'a>(input: &'a [u8]) -> IResult<&'a [u8], ClientCommand> {
    alt((
        version_command,
        cpid_command,
        auth_command,
        cont_command,
    ))(input)
}

/*
fn server_command(buf: &u8) -> IResult<&u8, ServerCommand> {
    unimplemented!();
}
*/

// -----------------------------------------------------------------
//
// SASL DECODING
//
// -----------------------------------------------------------------

// impersonated user, login, password
fn auth_plain<'a>(input: &'a [u8]) -> IResult<&'a [u8], (&'a [u8], &'a [u8], &'a [u8])> {
    tuple((is_not([0x0]), is_not([0x0]), rest))(input)
}

// -----------------------------------------------------------------
//
// DOVECOT AUTH ENCODING
//
// ------------------------------------------------------------------
use tokio_util::bytes::{BufMut, BytesMut};
trait Encode {
    fn encode(&self, out: &mut BytesMut) -> Result<()>;
}

fn tab_enc(out: &mut BytesMut) {
    out.put(&[0x09][..])
}

fn lf_enc(out: &mut BytesMut) {
    out.put(&[0x0A][..])
}

impl Encode for Mechanism {
    fn encode(&self, out: &mut BytesMut) -> Result<()> {
        match self {
            Self::Plain => out.put(&b"PLAIN"[..]),
            Self::Login => out.put(&b"LOGIN"[..]),
        }
        Ok(())
    }
}

impl Encode for MechanismParameters {
    fn encode(&self, out: &mut BytesMut) -> Result<()> {
        match self {
            Self::Anonymous => out.put(&b"anonymous"[..]),
            Self::PlainText => out.put(&b"plaintext"[..]),
            Self::Dictionary => out.put(&b"dictionary"[..]),
            Self::Active => out.put(&b"active"[..]),
            Self::ForwardSecrecy => out.put(&b"forward-secrecy"[..]),
            Self::MutualAuth => out.put(&b"mutual-auth"[..]),
            Self::Private => out.put(&b"private"[..]),
        }
        Ok(())
    }
}


impl Encode for FailCode {
    fn encode(&self, out: &mut BytesMut) -> Result<()> {
        match self {
            Self::TempFail => out.put(&b"temp_fail"[..]),
            Self::AuthzFail => out.put(&b"authz_fail"[..]),
            Self::UserDisabled => out.put(&b"user_disabled"[..]),
            Self::PassExpired => out.put(&b"pass_expired"[..]),
        };
        Ok(())
    }
}

impl Encode for ServerCommand {
    fn encode(&self, out: &mut BytesMut) -> Result<()> {
        match self {
            Self::Version (Version { major, minor }) => {
                out.put(&b"VERSION"[..]);
                tab_enc(out);
                out.put(major.to_string().as_bytes());
                tab_enc(out);
                out.put(minor.to_string().as_bytes());
                lf_enc(out);
            },
            Self::Spid(pid) => {
                out.put(&b"SPID"[..]);
                tab_enc(out);
                out.put(pid.to_string().as_bytes());
                lf_enc(out);
            },
            Self::Cuid(pid) => {
                out.put(&b"CUID"[..]);
                tab_enc(out);
                out.put(pid.to_string().as_bytes());
                lf_enc(out);
            },
            Self::Cookie(cval) => {
                out.put(&b"COOKIE"[..]);
                tab_enc(out);
                out.put(hex::encode(cval).as_bytes()); 
                lf_enc(out);

            },
            Self::Mech { kind, parameters } => {
                out.put(&b"MECH"[..]);
                tab_enc(out);
                kind.encode(out)?;
                for p in parameters.iter() {
                    tab_enc(out);
                    p.encode(out)?;
                }
                lf_enc(out);
            },
            Self::Done => {
                out.put(&b"DONE"[..]);
                lf_enc(out);
            },
            Self::Cont { id, data } => {
                out.put(&b"CONT"[..]);
                tab_enc(out);
                out.put(id.to_string().as_bytes());
                if let Some(rdata) = data {
                    tab_enc(out);
                    let b64 = base64::engine::general_purpose::STANDARD.encode(rdata);
                    out.put(b64.as_bytes());
                }
                lf_enc(out);
            },
            Self::Ok { id, user_id, extra_parameters } => {
                out.put(&b"OK"[..]);
                tab_enc(out);
                out.put(id.to_string().as_bytes());
                if let Some(user) = user_id {
                    tab_enc(out);
                    out.put(&b"user="[..]);
                    out.put(user.as_bytes());
                }
                for p in extra_parameters.iter() {
                    tab_enc(out);
                    out.put(&p[..]);
                }
                lf_enc(out);
            },
            Self::Fail {id, user_id, code, extra_parameters } => {
                out.put(&b"FAIL"[..]);
                tab_enc(out);
                out.put(id.to_string().as_bytes());
                if let Some(user) = user_id {
                    tab_enc(out);
                    out.put(&b"user="[..]);
                    out.put(user.as_bytes());
                }
                if let Some(code_val) = code {
                    tab_enc(out);
                    out.put(&b"code="[..]);
                    code_val.encode(out)?;
                }
                for p in extra_parameters.iter() {
                    tab_enc(out);
                    out.put(&p[..]);
                }
                lf_enc(out);
            },
        }
        Ok(())
    }
}
