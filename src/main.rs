mod bayou;
mod config;
mod cryptoblob;
mod login;
mod mailbox;
mod time;
mod uidindex;
mod server;

use anyhow::{bail, Result};
use std::sync::Arc;

use rusoto_signature::Region;

use config::*;
use login::{ldap_provider::*, static_provider::*, *};
use mailbox::Mailbox;
use server::Server;

#[tokio::main]
async fn main() {
    if let Err(e) = main2().await {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

async fn main2() -> Result<()> {
    let config = read_config("mailrage.toml".into())?;

    let server = Server::new(config)?;
    server.run().await
}

