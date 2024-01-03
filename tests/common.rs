use anyhow::{bail, Context, Result};
use std::net::{Shutdown, TcpStream};
use std::process::Command;
use std::{thread, time};

static SMALL_DELAY: time::Duration = time::Duration::from_millis(200);

pub fn aerogramme_provider_daemon_dev(mut fx: impl FnMut(&mut TcpStream, &mut TcpStream) -> Result<()>) -> Result<()> {
    let mut daemon = Command::new(env!("CARGO_BIN_EXE_aerogramme"))
        .arg("--dev")
        .arg("provider")
        .arg("daemon")
        .spawn()?;

    let mut max_retry = 20;
    let mut imap_socket = loop {
        max_retry -= 1;
        match (TcpStream::connect("[::1]:1143"), max_retry) {
            (Err(e), 0) => bail!("no more retry, last error is: {}", e),
            (Err(e), _) => {
                println!("unable to connect: {} ; will retry in 1 sec", e);
            }
            (Ok(v), _) => break v,
        }
        thread::sleep(SMALL_DELAY);
    };

    let mut lmtp_socket = TcpStream::connect("[::1]:1025").context("lmtp socket must be connected")?;

    println!("-- ready to test imap features --");
    let result = fx(&mut imap_socket, &mut lmtp_socket);
    println!("-- test teardown --");

    imap_socket
        .shutdown(Shutdown::Both)
        .context("closing imap socket at the end of the test")?;
    lmtp_socket
        .shutdown(Shutdown::Both)
        .context("closing lmtp socket at the end of the test")?;
    daemon.kill().context("daemon should be killed")?;

    result.context("all tests passed")
}
