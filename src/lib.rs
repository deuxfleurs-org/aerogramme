#![feature(type_alias_impl_trait)]
#![feature(async_fn_in_trait)]
#![feature(async_closure)]
#![feature(trait_alias)]

pub mod auth;
pub mod bayou;
pub mod config;
pub mod cryptoblob;
pub mod dav;
pub mod imap;
pub mod k2v_util;
pub mod lmtp;
pub mod login;
pub mod mail;
pub mod server;
pub mod storage;
pub mod timestamp;
pub mod user;
