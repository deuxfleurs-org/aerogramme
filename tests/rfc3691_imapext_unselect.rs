use anyhow::{bail, Context, Result};
use std::io::Write;
use std::net::TcpStream;
use std::{thread, time};

mod common;
use crate::common::read_lines;

static SMALL_DELAY: time::Duration = time::Duration::from_millis(200);
static EMAIL: &[u8] = b"From: alice@example.com\r
To: alice@example.tld\r
Subject: Test\r
\r
Hello world!\r
";

fn main() {
    common::aerogramme_provider_daemon_dev(|imap_socket, lmtp_socket| {
        lmtp_handshake(lmtp_socket).context("handshake lmtp done")?;
        lmtp_deliver_email(lmtp_socket, EMAIL).context("mail delivered successfully")?;

        connect(imap_socket).context("server says hello")?;
        capability(imap_socket).context("check server capabilities")?;
        login(imap_socket).context("login test")?;
        select_inbox(imap_socket).context("select inbox")?;
        noop_exists(imap_socket).context("noop loop must detect a new email")?;
        add_flags_email(imap_socket).context("add delete flags to the email")?;
        unselect(imap_socket)
            .context("unselect inbox while preserving email with the \\Delete flag")?;
        select_inbox_one_msg(imap_socket).context("select inbox again")?;
        fetch_rfc822(imap_socket, EMAIL).context("message is still present")?;
        close(imap_socket).context("close inbox and expunge message")?;
        select_inbox_zero_msg(imap_socket).context("select inbox again and check it's empty")?;

        Ok(())
    })
    .expect("test fully run");
}

fn connect(imap: &mut TcpStream) -> Result<()> {
    let mut buffer: [u8; 1500] = [0; 1500];

    let read = read_lines(imap, &mut buffer, None)?;
    assert_eq!(&read[..4], &b"* OK"[..]);

    Ok(())
}

fn capability(imap: &mut TcpStream) -> Result<()> {
    imap.write(&b"5 capability\r\n"[..])?;

    let mut buffer: [u8; 1500] = [0; 1500];
    let read = read_lines(imap, &mut buffer, Some(&b"5 OK"[..]))?;
    let srv_msg = std::str::from_utf8(read)?;
    assert!(srv_msg.contains("IMAP4REV1"));
    assert!(srv_msg.contains("UNSELECT"));

    Ok(())
}

fn login(imap: &mut TcpStream) -> Result<()> {
    let mut buffer: [u8; 1500] = [0; 1500];

    imap.write(&b"10 login alice hunter2\r\n"[..])?;

    let read = read_lines(imap, &mut buffer, None)?;
    assert_eq!(&read[..5], &b"10 OK"[..]);

    Ok(())
}

fn select_inbox_one_msg(imap: &mut TcpStream) -> Result<()> {
    let mut buffer: [u8; 6000] = [0; 6000];

    imap.write(&b"22 select inbox\r\n"[..])?;
    let read = read_lines(imap, &mut buffer, Some(&b"22 OK"[..]))?;
    let srv_msg = std::str::from_utf8(read)?;
    assert!(srv_msg.contains("* 1 EXISTS"));

    Ok(())
}

fn select_inbox_zero_msg(imap: &mut TcpStream) -> Result<()> {
    let mut buffer: [u8; 6000] = [0; 6000];

    imap.write(&b"21 select inbox\r\n"[..])?;
    let read = read_lines(imap, &mut buffer, Some(&b"21 OK"[..]))?;
    let srv_msg = std::str::from_utf8(read)?;
    assert!(srv_msg.contains("* 0 EXISTS"));

    Ok(())
}

fn select_inbox(imap: &mut TcpStream) -> Result<()> {
    let mut buffer: [u8; 6000] = [0; 6000];

    imap.write(&b"20 select inbox\r\n"[..])?;
    let _read = read_lines(imap, &mut buffer, Some(&b"20 OK"[..]))?;

    Ok(())
}

fn noop_exists(imap: &mut TcpStream) -> Result<()> {
    let mut buffer: [u8; 6000] = [0; 6000];

    let mut max_retry = 20;
    loop {
        max_retry -= 1;
        imap.write(&b"30 NOOP\r\n"[..])?;
        let read = read_lines(imap, &mut buffer, Some(&b"30 OK NOOP"[..]))?;
        let srv_msg = std::str::from_utf8(read)?;

        match (max_retry, srv_msg.contains("* 1 EXISTS")) {
            (_, true) => break,
            (0, _) => bail!("no more retry"),
            _ => (),
        }

        thread::sleep(SMALL_DELAY);
    }

    Ok(())
}

fn lmtp_handshake(lmtp: &mut TcpStream) -> Result<()> {
    let mut buffer: [u8; 1500] = [0; 1500];

    let _read = read_lines(lmtp, &mut buffer, None)?;
    assert_eq!(&buffer[..4], &b"220 "[..]);

    lmtp.write(&b"LHLO example.tld\r\n"[..])?;
    let _read = read_lines(lmtp, &mut buffer, Some(&b"250 "[..]))?;

    Ok(())
}

fn lmtp_deliver_email(lmtp: &mut TcpStream, email: &[u8]) -> Result<()> {
    let mut buffer: [u8; 1500] = [0; 1500];

    lmtp.write(&b"MAIL FROM:<bob@example.tld>\r\n"[..])?;
    let _read = read_lines(lmtp, &mut buffer, Some(&b"250 2.0.0"[..]))?;

    lmtp.write(&b"RCPT TO:<alice@example.tld>\r\n"[..])?;
    let _read = read_lines(lmtp, &mut buffer, Some(&b"250 2.1.5"[..]))?;

    lmtp.write(&b"DATA\r\n"[..])?;
    let _read = read_lines(lmtp, &mut buffer, Some(&b"354 "[..]))?;

    lmtp.write(email)?;
    lmtp.write(&b"\r\n.\r\n"[..])?;
    let _read = read_lines(lmtp, &mut buffer, Some(&b"250 2.0.0"[..]))?;

    Ok(())
}

fn fetch_rfc822(imap: &mut TcpStream, ref_mail: &[u8]) -> Result<()> {
    let mut buffer: [u8; 65535] = [0; 65535];
    imap.write(&b"40 fetch 1 rfc822\r\n"[..])?;

    let read = read_lines(imap, &mut buffer, Some(&b"40 OK FETCH"[..]))?;
    let srv_msg = std::str::from_utf8(read)?;
    let orig_email = std::str::from_utf8(ref_mail)?;
    assert!(srv_msg.contains(orig_email));

    Ok(())
}

fn add_flags_email(imap: &mut TcpStream) -> Result<()> {
    let mut buffer: [u8; 1500] = [0; 1500];
    imap.write(&b"50 store 1 +FLAGS (\\Deleted)\r\n"[..])?;
    let _read = read_lines(imap, &mut buffer, Some(&b"50 OK STORE"[..]))?;

    Ok(())
}

fn close(imap: &mut TcpStream) -> Result<()> {
    imap.write(&b"60 close\r\n"[..])?;
    let mut buffer: [u8; 1500] = [0; 1500];
    let _read = read_lines(imap, &mut buffer, Some(&b"60 OK"[..]))?;

    Ok(())
}

fn unselect(imap: &mut TcpStream) -> Result<()> {
    imap.write(&b"70 unselect\r\n"[..])?;
    let mut buffer: [u8; 1500] = [0; 1500];
    let _read = read_lines(imap, &mut buffer, Some(&b"70 OK"[..]))?;

    Ok(())
}
