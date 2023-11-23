#![feature(async_fn_in_trait)]

mod timestamp;
mod bayou;
mod config;
mod cryptoblob;
mod imap;
mod k2v_util;
mod lmtp;
mod login;
mod mail;
mod server;
mod storage;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use config::*;
use server::Server;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Runs the IMAP+LMTP server daemon
    Server {
        #[clap(short, long, env = "CONFIG_FILE", default_value = "aerogramme.toml")]
        config_file: PathBuf,
    }
}

#[derive(Parser, Debug)]
struct UserSecretsArgs {
    /// User secret
    #[clap(short = 'U', long, env = "USER_SECRET")]
    user_secret: String,
    /// Alternate user secrets (comma-separated list of strings)
    #[clap(long, env = "ALTERNATE_USER_SECRETS", default_value = "")]
    alternate_user_secrets: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "main=info,aerogramme=info,k2v_client=info")
    }

    // Abort on panic (same behavior as in Go)
    std::panic::set_hook(Box::new(|panic_info| {
        eprintln!("{}", panic_info);
        eprintln!("{:?}", backtrace::Backtrace::new());
        std::process::abort();
    }));

    tracing_subscriber::fmt::init();

    let args = Args::parse();

    match args.command {
        Command::Server { config_file } => {
            let config = read_config(config_file)?;

            let server = Server::new(config).await?;
            server.run().await?;
        }
    }

    Ok(())
}

