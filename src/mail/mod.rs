pub mod mail_ident;
pub mod mailbox;
pub mod uidindex;
pub mod user;

use std::convert::TryFrom;

use anyhow::Result;
use k2v_client::K2vClient;
use rusoto_s3::S3Client;

use crate::bayou::Bayou;
use crate::cryptoblob::Key;
use crate::login::Credentials;
use crate::mail::mail_ident::*;
use crate::mail::uidindex::*;

// Internet Message Format
// aka RFC 822 - RFC 2822 - RFC 5322
pub struct IMF<'a> {
    raw: &'a [u8],
    parsed: mail_parser::Message<'a>,
}
