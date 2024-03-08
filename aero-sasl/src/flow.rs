use futures::Future;
use rand::prelude::*;

use super::types::*;
use super::decode::auth_plain;

#[derive(Debug)]
pub enum AuthRes {
    Success(String),
    Failed(Option<String>, Option<FailCode>),
}

#[derive(Debug)]
pub enum State {
    Error,
    Init,
    HandshakePart(Version),
    HandshakeDone,
    AuthPlainProgress { id: u64 },
    AuthDone { id: u64, res: AuthRes },
}

const SERVER_MAJOR: u64 = 1;
const SERVER_MINOR: u64 = 2;
const EMPTY_AUTHZ: &[u8] = &[];
impl State {
    pub fn new() -> Self {
        Self::Init
    }

    async fn try_auth_plain<X, F>(&self, data: &[u8], login: X) -> AuthRes
    where 
        X: FnOnce(String, String) -> F, 
        F: Future<Output=bool>,
     {
        // Check that we can extract user's login+pass
        let (ubin, pbin) = match auth_plain(&data) {
            Ok(([], (authz, user, pass))) if authz == user || authz == EMPTY_AUTHZ => (user, pass),
            Ok(_) => {
                tracing::error!("Impersonating user is not supported");
                return AuthRes::Failed(None, None);
            }
            Err(e) => {
                tracing::error!(err=?e, "Could not parse the SASL PLAIN data chunk");
                return AuthRes::Failed(None, None);
            }
        };

        // Try to convert it to UTF-8
        let (user, password) = match (std::str::from_utf8(ubin), std::str::from_utf8(pbin)) {
            (Ok(u), Ok(p)) => (u, p),
            _ => {
                tracing::error!("Username or password contain invalid UTF-8 characters");
                return AuthRes::Failed(None, None);
            }
        };

        // Try to connect user
        match login(user.to_string(), password.to_string()).await {
            true => AuthRes::Success(user.to_string()),
            false => {
                tracing::warn!("login failed");
                AuthRes::Failed(Some(user.to_string()), None)
            }
        }
    }

    pub async fn progress<F,X>(&mut self, cmd: ClientCommand, login: X)
    where 
        X: FnOnce(String, String) -> F, 
        F: Future<Output=bool>,
    {
        let new_state = 'state: {
            match (std::mem::replace(self, State::Error), cmd) {
                (Self::Init, ClientCommand::Version(v)) => Self::HandshakePart(v),
                (Self::HandshakePart(version), ClientCommand::Cpid(_cpid)) => {
                    if version.major != SERVER_MAJOR {
                        tracing::error!(
                            client_major = version.major,
                            server_major = SERVER_MAJOR,
                            "Unsupported client major version"
                        );
                        break 'state Self::Error;
                    }

                    Self::HandshakeDone
                }
                (
                    Self::HandshakeDone { .. },
                    ClientCommand::Auth {
                        id, mech, options, ..
                    },
                )
                | (
                    Self::AuthDone { .. },
                    ClientCommand::Auth {
                        id, mech, options, ..
                    },
                ) => {
                    if mech != Mechanism::Plain {
                        tracing::error!(mechanism=?mech, "Unsupported Authentication Mechanism");
                        break 'state Self::AuthDone {
                            id,
                            res: AuthRes::Failed(None, None),
                        };
                    }

                    match options.last() {
                        Some(AuthOption::Resp(data)) => Self::AuthDone {
                            id,
                            res: self.try_auth_plain(&data, login).await,
                        },
                        _ => Self::AuthPlainProgress { id },
                    }
                }
                (Self::AuthPlainProgress { id }, ClientCommand::Cont { id: cid, data }) => {
                    // Check that ID matches
                    if cid != id {
                        tracing::error!(
                            auth_id = id,
                            cont_id = cid,
                            "CONT id does not match AUTH id"
                        );
                        break 'state Self::AuthDone {
                            id,
                            res: AuthRes::Failed(None, None),
                        };
                    }

                    Self::AuthDone {
                        id,
                        res: self.try_auth_plain(&data, login).await,
                    }
                }
                _ => {
                    tracing::error!("This command is not valid in this context");
                    Self::Error
                }
            }
        };
        tracing::debug!(state=?new_state, "Made progress");
        *self = new_state;
    }

    pub fn response(&self) -> Vec<ServerCommand> {
        let mut srv_cmd: Vec<ServerCommand> = Vec::new();

        match self {
            Self::HandshakeDone { .. } => {
                srv_cmd.push(ServerCommand::Version(Version {
                    major: SERVER_MAJOR,
                    minor: SERVER_MINOR,
                }));

                srv_cmd.push(ServerCommand::Mech {
                    kind: Mechanism::Plain,
                    parameters: vec![MechanismParameters::PlainText],
                });

                srv_cmd.push(ServerCommand::Spid(15u64));
                srv_cmd.push(ServerCommand::Cuid(19350u64));

                let mut cookie = [0u8; 16];
                thread_rng().fill(&mut cookie);
                srv_cmd.push(ServerCommand::Cookie(cookie));

                srv_cmd.push(ServerCommand::Done);
            }
            Self::AuthPlainProgress { id } => {
                srv_cmd.push(ServerCommand::Cont {
                    id: *id,
                    data: None,
                });
            }
            Self::AuthDone {
                id,
                res: AuthRes::Success(user),
            } => {
                srv_cmd.push(ServerCommand::Ok {
                    id: *id,
                    user_id: Some(user.to_string()),
                    extra_parameters: vec![],
                });
            }
            Self::AuthDone {
                id,
                res: AuthRes::Failed(maybe_user, maybe_failcode),
            } => {
                srv_cmd.push(ServerCommand::Fail {
                    id: *id,
                    user_id: maybe_user.clone(),
                    code: maybe_failcode.clone(),
                    extra_parameters: vec![],
                });
            }
            _ => (),
        };

        srv_cmd
    }
}
