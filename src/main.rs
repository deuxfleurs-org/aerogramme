mod bayou;
mod command;
mod config;
mod cryptoblob;
mod login;
mod mailbox;
mod mailstore;
mod server;
mod service;
mod session;
mod time;
mod uidindex;

use std::path::PathBuf;

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use rand::prelude::*;

use rusoto_signature::Region;

use config::*;
use cryptoblob::*;
use login::{static_provider::*, *};
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
        #[clap(short, long, env = "CONFIG_FILE", default_value = "mailrage.toml")]
        config_file: PathBuf,
    },
    /// Initializes key pairs for a user and adds a key decryption password
    FirstLogin {
        #[clap(flatten)]
        creds: StorageCredsArgs,
        #[clap(flatten)]
        user_secrets: UserSecretsArgs,
    },
    /// Initializes key pairs for a user and dumps keys to stdout for usage with static
    /// login provider
    InitializeLocalKeys {
        #[clap(flatten)]
        creds: StorageCredsArgs,
    },
    /// Adds a key decryption password for a user
    AddPassword {
        #[clap(flatten)]
        creds: StorageCredsArgs,
        #[clap(flatten)]
        user_secrets: UserSecretsArgs,
        /// Automatically generate password
        #[clap(short, long)]
        gen: bool,
    },
    /// Deletes a key decription password for a user
    DeletePassword {
        #[clap(flatten)]
        creds: StorageCredsArgs,
        #[clap(flatten)]
        user_secrets: UserSecretsArgs,
        /// Allow to delete all passwords
        #[clap(long)]
        allow_delete_all: bool,
    },
    /// Dumps all encryption keys for user
    ShowKeys {
        #[clap(flatten)]
        creds: StorageCredsArgs,
        #[clap(flatten)]
        user_secrets: UserSecretsArgs,
    },
}

#[derive(Parser, Debug)]
struct StorageCredsArgs {
    /// Name of the region to use
    #[clap(short = 'r', long, env = "AWS_REGION")]
    region: String,
    /// Url of the endpoint to connect to for K2V
    #[clap(short = 'k', long, env = "K2V_ENDPOINT")]
    k2v_endpoint: String,
    /// Url of the endpoint to connect to for S3
    #[clap(short = 's', long, env = "S3_ENDPOINT")]
    s3_endpoint: String,
    /// Access key ID
    #[clap(short = 'A', long, env = "AWS_ACCESS_KEY_ID")]
    aws_access_key_id: String,
    /// Access key ID
    #[clap(short = 'S', long, env = "AWS_SECRET_ACCESS_KEY")]
    aws_secret_access_key: String,
    /// Bucket name
    #[clap(short = 'b', long, env = "BUCKET")]
    bucket: String,
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
        std::env::set_var("RUST_LOG", "main=info,mailrage=info,k2v_client=info")
    }

    tracing_subscriber::fmt::init();

    let args = Args::parse();

    match args.command {
        Command::Server { config_file } => {
            let config = read_config(config_file)?;

            let server = Server::new(config).await?;
            server.run().await?;
        }
        Command::FirstLogin {
            creds,
            user_secrets,
        } => {
            let creds = make_storage_creds(creds);
            let user_secrets = make_user_secrets(user_secrets);

            println!("Please enter your password for key decryption.");
            println!("If you are using LDAP login, this must be your LDAP password.");
            println!("If you are using the static login provider, enter any password, and this will also become your password for local IMAP access.");
            let password = rpassword::prompt_password("Enter password: ")?;
            let password_confirm = rpassword::prompt_password("Confirm password: ")?;
            if password != password_confirm {
                bail!("Passwords don't match.");
            }

            CryptoKeys::init(&creds, &user_secrets, &password).await?;

            println!("");
            println!("Cryptographic key setup is complete.");
            println!("");
            println!("If you are using the static login provider, add the following section to your .toml configuration file:");
            println!("");
            dump_config(&password, &creds);
        }
        Command::InitializeLocalKeys { creds } => {
            let creds = make_storage_creds(creds);

            println!("Please enter a password for local IMAP access.");
            println!("This password is not used for key decryption, your keys will be printed below (do not lose them!)");
            println!(
                "If you plan on using LDAP login, stop right here and use `first-login` instead"
            );
            let password = rpassword::prompt_password("Enter password: ")?;
            let password_confirm = rpassword::prompt_password("Confirm password: ")?;
            if password != password_confirm {
                bail!("Passwords don't match.");
            }

            let master = gen_key();
            let (_, secret) = gen_keypair();
            let keys = CryptoKeys::init_without_password(&creds, &master, &secret).await?;

            println!("");
            println!("Cryptographic key setup is complete.");
            println!("");
            println!("Add the following section to your .toml configuration file:");
            println!("");
            dump_config(&password, &creds);
            dump_keys(&keys);
        }
        Command::AddPassword {
            creds,
            user_secrets,
            gen,
        } => {
            let creds = make_storage_creds(creds);
            let user_secrets = make_user_secrets(user_secrets);

            let existing_password =
                rpassword::prompt_password("Enter existing password to decrypt keys: ")?;
            let new_password = if gen {
                let password = base64::encode_config(
                    &u128::to_be_bytes(thread_rng().gen())[..10],
                    base64::URL_SAFE_NO_PAD,
                );
                println!("Your new password: {}", password);
                println!("Keep it safe!");
                password
            } else {
                let password = rpassword::prompt_password("Enter new password: ")?;
                let password_confirm = rpassword::prompt_password("Confirm new password: ")?;
                if password != password_confirm {
                    bail!("Passwords don't match.");
                }
                password
            };

            let keys = CryptoKeys::open(&creds, &user_secrets, &existing_password).await?;
            keys.add_password(&creds, &user_secrets, &new_password)
                .await?;
            println!("");
            println!("New password added successfully.");
        }
        Command::DeletePassword {
            creds,
            user_secrets,
            allow_delete_all,
        } => {
            let creds = make_storage_creds(creds);
            let user_secrets = make_user_secrets(user_secrets);

            let existing_password = rpassword::prompt_password("Enter password to delete: ")?;

            let keys = match allow_delete_all {
                true => Some(CryptoKeys::open(&creds, &user_secrets, &existing_password).await?),
                false => None,
            };

            CryptoKeys::delete_password(&creds, &existing_password, allow_delete_all).await?;

            println!("");
            println!("Password was deleted successfully.");

            if let Some(keys) = keys {
                println!("As a reminder, here are your cryptographic keys:");
                dump_keys(&keys);
            }
        }
        Command::ShowKeys {
            creds,
            user_secrets,
        } => {
            let creds = make_storage_creds(creds);
            let user_secrets = make_user_secrets(user_secrets);

            let existing_password = rpassword::prompt_password("Enter key decryption password: ")?;

            let keys = CryptoKeys::open(&creds, &user_secrets, &existing_password).await?;
            dump_keys(&keys);
        }
    }

    Ok(())
}

fn make_storage_creds(c: StorageCredsArgs) -> StorageCredentials {
    let s3_region = Region::Custom {
        name: c.region.clone(),
        endpoint: c.s3_endpoint,
    };
    let k2v_region = Region::Custom {
        name: c.region,
        endpoint: c.k2v_endpoint,
    };
    StorageCredentials {
        k2v_region,
        s3_region,
        aws_access_key_id: c.aws_access_key_id,
        aws_secret_access_key: c.aws_secret_access_key,
        bucket: c.bucket,
    }
}

fn make_user_secrets(c: UserSecretsArgs) -> UserSecrets {
    UserSecrets {
        user_secret: c.user_secret,
        alternate_user_secrets: c
            .alternate_user_secrets
            .split(",")
            .map(|x| x.trim())
            .filter(|x| !x.is_empty())
            .map(|x| x.to_string())
            .collect(),
    }
}

fn dump_config(password: &str, creds: &StorageCredentials) {
    println!("[login_static.users.<username>]");
    println!(
        "password = \"{}\"",
        hash_password(password).expect("unable to hash password")
    );
    println!("aws_access_key_id = \"{}\"", creds.aws_access_key_id);
    println!(
        "aws_secret_access_key = \"{}\"",
        creds.aws_secret_access_key
    );
}

fn dump_keys(keys: &CryptoKeys) {
    println!("master_key = \"{}\"", base64::encode(&keys.master));
    println!("secret_key = \"{}\"", base64::encode(&keys.secret));
}
