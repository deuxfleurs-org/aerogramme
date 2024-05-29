use anyhow::Result;
use base64::Engine;
use tokio_util::bytes::{BufMut, BytesMut};

use super::types::*;

pub trait Encode {
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
            Self::Version(Version { major, minor }) => {
                out.put(&b"VERSION"[..]);
                tab_enc(out);
                out.put(major.to_string().as_bytes());
                tab_enc(out);
                out.put(minor.to_string().as_bytes());
                lf_enc(out);
            }
            Self::Spid(pid) => {
                out.put(&b"SPID"[..]);
                tab_enc(out);
                out.put(pid.to_string().as_bytes());
                lf_enc(out);
            }
            Self::Cuid(pid) => {
                out.put(&b"CUID"[..]);
                tab_enc(out);
                out.put(pid.to_string().as_bytes());
                lf_enc(out);
            }
            Self::Cookie(cval) => {
                out.put(&b"COOKIE"[..]);
                tab_enc(out);
                out.put(hex::encode(cval).as_bytes());
                lf_enc(out);
            }
            Self::Mech { kind, parameters } => {
                out.put(&b"MECH"[..]);
                tab_enc(out);
                kind.encode(out)?;
                for p in parameters.iter() {
                    tab_enc(out);
                    p.encode(out)?;
                }
                lf_enc(out);
            }
            Self::Done => {
                out.put(&b"DONE"[..]);
                lf_enc(out);
            }
            Self::Cont { id, data } => {
                out.put(&b"CONT"[..]);
                tab_enc(out);
                out.put(id.to_string().as_bytes());
                tab_enc(out);
                if let Some(rdata) = data {
                    let b64 = base64::engine::general_purpose::STANDARD.encode(rdata);
                    out.put(b64.as_bytes());
                }
                lf_enc(out);
            }
            Self::Ok {
                id,
                user_id,
                extra_parameters,
            } => {
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
            }
            Self::Fail {
                id,
                user_id,
                code,
                extra_parameters,
            } => {
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
            }
        }
        Ok(())
    }
}
