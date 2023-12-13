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
    /// A daemon to be run by the end user, on a personal device
    Companion(CompanionCommand),

    #[clap(subcommand)]
    /// A daemon to be run by the service provider, on a server
    Provider(ProviderCommand),

    #[clap(subcommand)]
    /// Specific tooling, should not be part of a normal workflow, for debug & experimenting only
    Tools(ToolsCommand),
    //Test,
}

#[derive(Subcommand, Debug)]
enum ToolsCommand {
    /// Manage crypto roots
    #[clap(subcommand)]
    CryptoRoot(CryptoRootCommand),
}

#[derive(Subcommand, Debug)]
enum CryptoRootCommand {
    /// Generate a new crypto-root protected with a password
    New {
        #[clap(env = "AEROGRAMME_PASSWORD")]
        maybe_password: Option<String>,
    },
    /// Generate a new clear text crypto-root, store it securely!
    NewClearText,
    /// Change the password of a crypto key
    ChangePassword {
        #[clap(env = "AEROGRAMME_OLD_PASSWORD")]
        maybe_old_password: Option<String>,

        #[clap(env = "AEROGRAMME_NEW_PASSWORD")]
        maybe_new_password: Option<String>,

        #[clap(short, long, env = "AEROGRAMME_CRYPTO_ROOT")]
        crypto_root: String,
    },
    /// From a given crypto-key, derive one containing only the public key
    DeriveIncoming {
        #[clap(short, long, env = "AEROGRAMME_CRYPTO_ROOT")]
        crypto_root: String,
    },
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
        #[clap(env = "AEROGRAMME_OLD_PASSWORD")]
        maybe_old_password: Option<String>,

        #[clap(env = "AEROGRAMME_NEW_PASSWORD")]
        maybe_new_password: Option<String>,

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
            CompanionCommand::Reload { pid: _pid } => {
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
        (Command::Tools(subcommand), _) => match subcommand {
            ToolsCommand::CryptoRoot(crcommand) => {
                match crcommand {
                    CryptoRootCommand::New { maybe_password } => {
                        let password = match maybe_password {
                            Some(pwd) => pwd.clone(),
                            None => {
                                let password = rpassword::prompt_password("Enter password: ")?;
                                let password_confirm = rpassword::prompt_password("Confirm password: ")?;
                                if password != password_confirm {
                                    bail!("Passwords don't match.");
                                }
                                password
                            }
                        };
                        let crypto_keys = CryptoKeys::init();
                        let cr =  CryptoRoot::create_pass(&password, &crypto_keys)?;
                        println!("{}", cr.0);
                    },
                    CryptoRootCommand::NewClearText => {
                        let crypto_keys = CryptoKeys::init();
                        let cr = CryptoRoot::create_cleartext(&crypto_keys);
                        println!("{}", cr.0);
                    },
                    CryptoRootCommand::ChangePassword { maybe_old_password, maybe_new_password, crypto_root } => {
                        let old_password = match maybe_old_password {
                            Some(pwd) => pwd.to_string(),
                            None => rpassword::prompt_password("Enter old password: ")?,
                        };

                        let new_password = match maybe_new_password {
                            Some(pwd) => pwd.to_string(),
                            None => {
                                let password = rpassword::prompt_password("Enter new password: ")?;
                                let password_confirm = rpassword::prompt_password("Confirm new password: ")?;
                                if password != password_confirm {
                                    bail!("Passwords don't match.");
                                }
                                password
                            }
                        };

                        let keys = CryptoRoot(crypto_root.to_string()).crypto_keys(&old_password)?;
                        let cr = CryptoRoot::create_pass(&new_password, &keys)?;
                        println!("{}", cr.0);
                    },
                    CryptoRootCommand::DeriveIncoming { crypto_root } => {
                        let pubkey = CryptoRoot(crypto_root.to_string()).public_key()?;
                        let cr = CryptoRoot::create_incoming(&pubkey);
                        println!("{}", cr.0);
                    },
                }
            },
        }
    }

    Ok(())
}

fn account_management(root: &Command, cmd: &AccountManagement, users: PathBuf) -> Result<()> {
    let mut ulist: UserList = read_config(users.clone()).context(format!("'{:?}' must be a user database", users))?;

    match cmd {
        AccountManagement::Add { login, setup } => {
            tracing::debug!(user=login, "will-create");
            let stp: SetupEntry = read_config(setup.clone()).context(format!("'{:?}' must be a setup file", setup))?;
            tracing::debug!(user=login, "loaded setup entry");

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

            let crypto_keys = CryptoKeys::init();
            let crypto_root = match root {
                Command::Provider(_) => CryptoRoot::create_pass(&password, &crypto_keys)?,
                Command::Companion(_) => CryptoRoot::create_cleartext(&crypto_keys),
                _ => unreachable!(),
            };

            let hash = hash_password(password.as_str()).context("unable to hash password")?;

            ulist.insert(login.clone(), UserEntry {
                email_addresses: stp.email_addresses,
                password: hash,
                crypto_root: crypto_root.0,
                storage: stp.storage,
            });

            write_config(users.clone(), &ulist)?;
        },
        AccountManagement::Delete { login } => {
            tracing::debug!(user=login, "will-delete");
            ulist.remove(login);
            write_config(users.clone(), &ulist)?;
        },
        AccountManagement::ChangePassword { maybe_old_password, maybe_new_password, login } => {
            let mut user = ulist.remove(login).context("user must exist first")?;

            let old_password = match maybe_old_password {
                Some(pwd) => pwd.to_string(),
                None => rpassword::prompt_password("Enter old password: ")?,
            };

            if !verify_password(&old_password, &user.password)? {
                bail!(format!("invalid password for login {}", login));
            }

            let crypto_keys = CryptoRoot(user.crypto_root).crypto_keys(&old_password)?;

            let new_password = match maybe_new_password {
                Some(pwd) => pwd.to_string(),
                None => {
                    let password = rpassword::prompt_password("Enter new password: ")?;
                    let password_confirm = rpassword::prompt_password("Confirm new password: ")?;
                    if password != password_confirm {
                        bail!("Passwords don't match.");
                    }
                    password
                }
            };  
            let new_hash = hash_password(&new_password)?;
            let new_crypto_root = CryptoRoot::create_pass(&new_password, &crypto_keys)?;
            
            user.password = new_hash;
            user.crypto_root = new_crypto_root.0;

            ulist.insert(login.clone(), user);
            write_config(users.clone(), &ulist)?;
        },
    };

    Ok(())
}
