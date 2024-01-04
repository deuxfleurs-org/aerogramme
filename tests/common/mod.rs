#![allow(dead_code)]
pub mod constants;
pub mod fragments;

use anyhow::{bail, Context, Result};
use std::io::Read;
use std::net::{Shutdown, TcpStream};
use std::process::Command;
use std::thread;

use constants::SMALL_DELAY;

pub fn aerogramme_provider_daemon_dev(
    mut fx: impl FnMut(&mut TcpStream, &mut TcpStream) -> Result<()>,
) -> Result<()> {
    // Check port is not used (= free) before starting the test
    let mut max_retry = 20;
    loop {
        max_retry -= 1;
        match (TcpStream::connect("[::1]:1143"), max_retry) {
            (Ok(_), 0) => bail!("something is listening on [::1]:1143 and prevent the test from starting"),
            (Ok(_), _) => println!("something is listening on [::1]:1143, maybe a previous daemon quitting, retrying soon..."),
            (Err(_), _) => {
                println!("test ready to start, [::1]:1143 is free!");
                break
            }
        }
        thread::sleep(SMALL_DELAY);
    }

    // Start daemon
    let mut daemon = Command::new(env!("CARGO_BIN_EXE_aerogramme"))
        .arg("--dev")
        .arg("provider")
        .arg("daemon")
        .spawn()?;

    // Check that our daemon is correctly listening on the free port
    let mut max_retry = 20;
    let mut imap_socket = loop {
        max_retry -= 1;
        match (TcpStream::connect("[::1]:1143"), max_retry) {
            (Err(e), 0) => bail!("no more retry, last error is: {}", e),
            (Err(e), _) => {
                println!("unable to connect: {} ; will retry soon...", e);
            }
            (Ok(v), _) => break v,
        }
        thread::sleep(SMALL_DELAY);
    };

    // Assuming now it's safe to open a LMTP socket
    let mut lmtp_socket =
        TcpStream::connect("[::1]:1025").context("lmtp socket must be connected")?;

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

pub fn read_lines<'a, F: Read>(
    reader: &mut F,
    buffer: &'a mut [u8],
    stop_marker: Option<&[u8]>,
) -> Result<&'a [u8]> {
    let mut nbytes = 0;
    loop {
        nbytes += reader.read(&mut buffer[nbytes..])?;
        //println!("partial read: {}", std::str::from_utf8(&buffer[..nbytes])?);
        let pre_condition = match stop_marker {
            None => true,
            Some(mark) => buffer[..nbytes].windows(mark.len()).any(|w| w == mark),
        };
        if pre_condition && &buffer[nbytes - 2..nbytes] == &b"\r\n"[..] {
            break;
        }
    }
    println!("read: {}", std::str::from_utf8(&buffer[..nbytes])?);
    Ok(&buffer[..nbytes])
}
