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

use anyhow::{bail, Result, Context};
use clap::{Parser, Subcommand};

use config::*;
use server::Server;
use login::{static_provider::*, *};

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(subcommand)]
    command: Command,

    #[clap(short, long, env = "CONFIG_FILE", default_value = "aerogramme.toml")]
    config_file: PathBuf,
}

#[derive(Subcommand, Debug)]
enum Command {
    #[clap(subcommand)]
    Companion(CompanionCommand),

    #[clap(subcommand)]
    Provider(ProviderCommand),
    //Test,
}

#[derive(Subcommand, Debug)]
enum CompanionCommand {
    /// Runs the IMAP proxy
    Daemon,
    Reload {
        #[clap(short, long, env = "AEROGRAMME_PID")]
        pid: Option<u64>,
    },
    Wizard,
    #[clap(subcommand)]
    Account(AccountManagement),
}

#[derive(Subcommand, Debug)]
enum ProviderCommand {
    /// Runs the IMAP+LMTP server daemon
    Daemon,
    /// Reload the daemon
    Reload,
    /// Manage static accounts
    #[clap(subcommand)]
    Account(AccountManagement),
}

#[derive(Subcommand, Debug)]
enum AccountManagement {
    /// Add an account
    Add {
        #[clap(short, long)]
        login: String,
        #[clap(short, long)]
        setup: PathBuf,
    },
    /// Delete an account
    Delete {
        #[clap(short, long)]
        login: String,
    },
    /// Change password for a given account
    ChangePassword {
        #[clap(short, long)]
        login: String
    },
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
    let any_config = read_config(args.config_file)?;

    match (&args.command, any_config) {
        (Command::Companion(subcommand), AnyConfig::Companion(config)) => match subcommand {
            CompanionCommand::Daemon => {
                let server = Server::from_companion_config(config).await?;
                server.run().await?;
            },
            CompanionCommand::Reload { pid } => {
                unimplemented!();
            },
            CompanionCommand::Wizard => {
                unimplemented!();
            },
            CompanionCommand::Account(cmd) => {
                let user_file = config.users.user_list;
                account_management(&args.command, cmd, user_file)?;
            }
        },
        (Command::Provider(subcommand), AnyConfig::Provider(config)) => match subcommand {
            ProviderCommand::Daemon => {
                let server = Server::from_provider_config(config).await?;
                server.run().await?;
            },
            ProviderCommand::Reload => {
                unimplemented!();
            },
            ProviderCommand::Account(cmd) => {
                let user_file = match config.users {
                    UserManagement::Static(conf) => conf.user_list,
                    UserManagement::Ldap(_) => panic!("LDAP account management is not supported from Aerogramme.")
                };
                account_management(&args.command, cmd, user_file)?;
            }
        },
        (Command::Provider(_), AnyConfig::Companion(_)) => {
            panic!("Your want to run a 'Provider' command but your configuration file has role 'Companion'.");
        },
        (Command::Companion(_), AnyConfig::Provider(_)) => {
            panic!("Your want to run a 'Companion' command but your configuration file has role 'Provider'.");
        },
    }

    Ok(())
}

fn account_management(root: &Command, cmd: &AccountManagement, users: PathBuf) -> Result<()> {
    let mut ulist: UserList = read_config(users.clone())?;

    match cmd {
        AccountManagement::Add { login, setup } => {
            tracing::debug!(user=login, "will-create");
            let stp: SetupEntry = read_config(setup.clone())?;
            tracing::debug!(user=login, "loaded setup entry");
            let crypto_root = match root {
                Command::Provider(_) => CryptographyRoot::PasswordProtected,
                Command::Companion(_) => {
                    // @TODO use keyring by default instead of inplace in the future
                    // @TODO generate keys
                    CryptographyRoot::InPlace {
                        master_key: "".to_string(),
                        secret_key: "".to_string(),
                    }
                }
            };

            let password = match stp.clear_password {
                Some(pwd) => pwd,
                None => {
                    let password = rpassword::prompt_password("Enter password: ")?;
                    let password_confirm = rpassword::prompt_password("Confirm password: ")?;
                    if password != password_confirm {
                        bail!("Passwords don't match.");
                    }
                    password
                }
            };
            let hash = hash_password(password.as_str()).context("unable to hash password")?;

            ulist.insert(login.clone(), UserEntry {
                email_addresses: stp.email_addresses,
                password: hash,
                crypto_root,
                storage: stp.storage,
            });

            write_config(users.clone(), &ulist)?;
        },
        AccountManagement::Delete { login } => {
            tracing::debug!(user=login, "will-delete");
            ulist.remove(&login);
            write_config(users.clone(), &ulist)?;
        },
        AccountManagement::ChangePassword { login } => {
            unimplemented!();
        },
    };

    Ok(())
}
