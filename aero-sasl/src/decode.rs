use base64::Engine;
use nom::{
    branch::alt,
    bytes::complete::{tag, tag_no_case, take, take_while, take_while1},
    character::complete::{tab, u16, u64},
    combinator::{map, opt, recognize, rest, value},
    error::{Error, ErrorKind},
    multi::{many1, separated_list0},
    sequence::{pair, preceded, tuple},
    IResult,
};

use super::types::*;

pub fn client_command<'a>(input: &'a [u8]) -> IResult<&'a [u8], ClientCommand> {
    alt((version_command, cpid_command, auth_command, cont_command))(input)
}

/*
fn server_command(buf: &u8) -> IResult<&u8, ServerCommand> {
    unimplemented!();
}
*/

// ---------------------

fn version_command<'a>(input: &'a [u8]) -> IResult<&'a [u8], ClientCommand> {
    let mut parser = tuple((tag_no_case(b"VERSION"), tab, u64, tab, u64));

    let (input, (_, _, major, _, minor)) = parser(input)?;
    Ok((input, ClientCommand::Version(Version { major, minor })))
}

pub fn cpid_command<'a>(input: &'a [u8]) -> IResult<&'a [u8], ClientCommand> {
    preceded(
        pair(tag_no_case(b"CPID"), tab),
        map(u64, |v| ClientCommand::Cpid(v)),
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
    recognize(many1(alt((take_while1(is_not_tab_or_esc_or_lf), is_esc))))(input)
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
    preceded(tag_no_case("service="), parameter_str)(input)
}

fn auth_option<'a>(input: &'a [u8]) -> IResult<&'a [u8], AuthOption> {
    use AuthOption::*;
    alt((
        alt((
            value(Debug, tag_no_case(b"debug")),
            value(NoPenalty, tag_no_case(b"no-penalty")),
            value(ClientId, tag_no_case(b"client_id")),
            value(NoLogin, tag_no_case(b"nologin")),
            map(preceded(tag_no_case(b"session="), u64), |id| Session(id)),
            map(preceded(tag_no_case(b"lip="), parameter_str), |ip| {
                LocalIp(ip)
            }),
            map(preceded(tag_no_case(b"rip="), parameter_str), |ip| {
                RemoteIp(ip)
            }),
            map(preceded(tag_no_case(b"lport="), u16), |port| {
                LocalPort(port)
            }),
            map(preceded(tag_no_case(b"rport="), u16), |port| {
                RemotePort(port)
            }),
            map(preceded(tag_no_case(b"real_rip="), parameter_str), |ip| {
                RealRemoteIp(ip)
            }),
            map(preceded(tag_no_case(b"real_lip="), parameter_str), |ip| {
                RealLocalIp(ip)
            }),
            map(preceded(tag_no_case(b"real_lport="), u16), |port| {
                RealLocalPort(port)
            }),
            map(preceded(tag_no_case(b"real_rport="), u16), |port| {
                RealRemotePort(port)
            }),
        )),
        alt((
            map(
                preceded(tag_no_case(b"local_name="), parameter_str),
                |name| LocalName(name),
            ),
            map(
                preceded(tag_no_case(b"forward_views="), parameter),
                |views| ForwardViews(views.into()),
            ),
            map(preceded(tag_no_case(b"secured="), parameter_str), |info| {
                Secured(Some(info))
            }),
            value(Secured(None), tag_no_case(b"secured")),
            value(CertUsername, tag_no_case(b"cert_username")),
            map(preceded(tag_no_case(b"transport="), parameter_str), |ts| {
                Transport(ts)
            }),
            map(
                preceded(tag_no_case(b"tls_cipher="), parameter_str),
                |cipher| TlsCipher(cipher),
            ),
            map(
                preceded(tag_no_case(b"tls_cipher_bits="), parameter_str),
                |bits| TlsCipherBits(bits),
            ),
            map(preceded(tag_no_case(b"tls_pfs="), parameter_str), |pfs| {
                TlsPfs(pfs)
            }),
            map(
                preceded(tag_no_case(b"tls_protocol="), parameter_str),
                |proto| TlsProtocol(proto),
            ),
            map(
                preceded(tag_no_case(b"valid-client-cert="), parameter_str),
                |cert| ValidClientCert(cert),
            ),
        )),
        alt((
            map(preceded(tag_no_case(b"resp="), base64), |data| Resp(data)),
            map(
                tuple((parameter_name, tag(b"="), parameter)),
                |(n, _, v)| UnknownPair(n, v.into()),
            ),
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
        map(opt(preceded(tab, separated_list0(tab, auth_option))), |o| {
            o.unwrap_or(vec![])
        }),
    ));
    let (input, (_, _, id, _, mech, _, service, options)) = parser(input)?;
    Ok((
        input,
        ClientCommand::Auth {
            id,
            mech,
            service,
            options,
        },
    ))
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
    let (input, (b64, _)) = tuple((take_while1(is_base64_core), take_while(is_base64_pad)))(input)?;

    let data = base64::engine::general_purpose::STANDARD_NO_PAD
        .decode(b64)
        .map_err(|_| nom::Err::Failure(Error::new(input, ErrorKind::TakeWhile1)))?;

    Ok((input, data))
}

/// @FIXME Dovecot does not say if base64 content must be padded or not
fn cont_command<'a>(input: &'a [u8]) -> IResult<&'a [u8], ClientCommand> {
    let mut parser = tuple((tag_no_case(b"CONT"), tab, u64, tab, base64));

    let (input, (_, _, id, _, data)) = parser(input)?;
    Ok((input, ClientCommand::Cont { id, data }))
}

// -----------------------------------------------------------------
//
// SASL DECODING
//
// -----------------------------------------------------------------

fn not_null(c: u8) -> bool {
    c != 0x0
}

// impersonated user, login, password
pub fn auth_plain<'a>(input: &'a [u8]) -> IResult<&'a [u8], (&'a [u8], &'a [u8], &'a [u8])> {
    map(
        tuple((
            take_while(not_null),
            take(1usize),
            take_while(not_null),
            take(1usize),
            rest,
        )),
        |(imp, _, user, _, pass)| (imp, user, pass),
    )(input)
}
