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
    },
    Test,
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
        Command::Test => {
            use std::collections::HashMap;
            use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
            println!("--- toml ---\n{}\n--- end  ---\n", toml::to_string(&Config {
                lmtp: None,
                imap: Some(ImapConfig { bind_addr: SocketAddr::new(IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1)), 8080) }),
                login_ldap: None,
                login_static: Some(HashMap::from([
                    ("alice".into(), LoginStaticUser {
                        password: "hash".into(),
                        user_secret: "hello".into(),
                        alternate_user_secrets: vec![],
                        email_addresses: vec![],
                        master_key: None,
                        secret_key: None,
                        storage: StaticStorage::Garage(StaticGarageConfig {
                            s3_endpoint: "http://".into(),
                            k2v_endpoint: "http://".into(),
                            aws_region: "garage".into(),
                            aws_access_key_id: "GK...".into(),
                            aws_secret_access_key: "xxx".into(),
                            bucket: "aerogramme".into(),
                        }),
                    })
                ])),
            }).unwrap()); 
        }
    }

    Ok(())
}

