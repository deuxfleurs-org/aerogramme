use anyhow::{bail, Context, Result};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::{thread, time};

mod common;

static SMALL_DELAY: time::Duration = time::Duration::from_millis(200);
static EMAIL1: &[u8] = b"Date: Sat, 8 Jul 2023 07:14:29 +0200\r
From: Bob Robert <bob@example.tld>\r
To: Alice Malice <alice@example.tld>\r
CC: =?ISO-8859-1?Q?Andr=E9?= Pirard <PIRARD@vm1.ulg.ac.be>\r
Subject: =?ISO-8859-1?B?SWYgeW91IGNhbiByZWFkIHRoaXMgeW8=?=\r
    =?ISO-8859-2?B?dSB1bmRlcnN0YW5kIHRoZSBleGFtcGxlLg==?=\r
X-Unknown: something something\r
Bad entry\r
  on multiple lines\r
Message-ID: <NTAxNzA2AC47634Y366BAMTY4ODc5MzQyODY0ODY5@www.grrrndzero.org>\r
MIME-Version: 1.0\r
Content-Type: multipart/alternative;\r
 boundary=\"b1_e376dc71bafc953c0b0fdeb9983a9956\"\r
Content-Transfer-Encoding: 7bit\r
\r
This is a multi-part message in MIME format.\r
\r
--b1_e376dc71bafc953c0b0fdeb9983a9956\r
Content-Type: text/plain; charset=utf-8\r
Content-Transfer-Encoding: quoted-printable\r
\r
GZ\r
OoOoO\r
oOoOoOoOo\r
oOoOoOoOoOoOoOoOo\r
oOoOoOoOoOoOoOoOoOoOoOo\r
oOoOoOoOoOoOoOoOoOoOoOoOoOoOo\r
OoOoOoOoOoOoOoOoOoOoOoOoOoOoOoOoO\r
\r
--b1_e376dc71bafc953c0b0fdeb9983a9956\r
Content-Type: text/html; charset=us-ascii\r
\r
<div style=\"text-align: center;\"><strong>GZ</strong><br />\r
OoOoO<br />\r
oOoOoOoOo<br />\r
oOoOoOoOoOoOoOoOo<br />\r
oOoOoOoOoOoOoOoOoOoOoOo<br />\r
oOoOoOoOoOoOoOoOoOoOoOoOoOoOo<br />\r
OoOoOoOoOoOoOoOoOoOoOoOoOoOoOoOoO<br />\r
</div>\r
\r
--b1_e376dc71bafc953c0b0fdeb9983a9956--\r
";

static EMAIL2: &[u8] = b"From: alice@example.com\r
To: alice@example.tld\r
Subject: Test\r
\r
Hello world!\r
";

fn main() {
    common::aerogramme_provider_daemon_dev(|imap_socket, lmtp_socket| {
        connect(imap_socket).context("server says hello")?;
        capability(imap_socket).context("check server capabilities")?;
        login(imap_socket).context("login test")?;
        create_mailbox(imap_socket).context("created mailbox archive")?;
        // UNSUBSCRIBE IS NOT IMPLEMENTED YET
        //unsubscribe_mailbox(imap_socket).context("unsubscribe from archive")?;
        select_inbox(imap_socket).context("select inbox")?;
        check(imap_socket).context("check must run")?;
        status_mailbox(imap_socket).context("status of archive from inbox")?;
        lmtp_handshake(lmtp_socket).context("handshake lmtp done")?;
        lmtp_deliver_email(lmtp_socket, EMAIL1).context("mail delivered successfully")?;
        noop_exists(imap_socket).context("noop loop must detect a new email")?;
        fetch_rfc822(imap_socket, EMAIL1).context("fetch rfc822 message")?;
        copy_email(imap_socket).context("copy message to the archive mailbox")?;
        append_email(imap_socket, EMAIL2).context("insert email in INBOX")?;
        // SEARCH IS NOT IMPLEMENTED YET
        //search(imap_socket).expect("search should return something");
        add_flags_email(imap_socket).context("should add delete and important flags to the email")?;
        expunge(imap_socket).context("expunge emails")?;
        rename_mailbox(imap_socket).context("archive mailbox is renamed my-archives")?;
        delete_mailbox(imap_socket).context("my-archives mailbox is deleted")?;
        Ok(())
    }).expect("test fully run");
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

    Ok(())
}

fn login(imap: &mut TcpStream) -> Result<()> {
    let mut buffer: [u8; 1500] = [0; 1500];

    imap.write(&b"10 login alice hunter2\r\n"[..])?;

    let read = read_lines(imap, &mut buffer, None)?;
    assert_eq!(&read[..5], &b"10 OK"[..]);

    Ok(())
}

fn create_mailbox(imap: &mut TcpStream) -> Result<()> {
    let mut buffer: [u8; 1500] = [0; 1500];

    imap.write(&b"15 create archive\r\n"[..])?;
    let read = read_lines(imap, &mut buffer, None)?;
    assert_eq!(&read[..12], &b"15 OK CREATE"[..]);

    Ok(())
}

#[allow(dead_code)]
fn unsubscribe_mailbox(imap: &mut TcpStream) -> Result<()> {
    let mut buffer: [u8; 6000] = [0; 6000];

    imap.write(&b"16 lsub \"\" *\r\n"[..])?;
    let read = read_lines(imap, &mut buffer, Some(&b"16 OK LSUB"[..]))?;
    let srv_msg = std::str::from_utf8(read)?;
    assert!(srv_msg.contains(" INBOX\r\n"));
    assert!(srv_msg.contains(" archive\r\n"));

    imap.write(&b"17 unsubscribe archive\r\n"[..])?;
    let read = read_lines(imap, &mut buffer, None)?;
    assert_eq!(&read[..5], &b"17 OK"[..]);

    imap.write(&b"18 lsub \"\" *\r\n"[..])?;
    let read = read_lines(imap, &mut buffer, Some(&b"18 OK LSUB"[..]))?;
    let srv_msg = std::str::from_utf8(read)?;
    assert!(srv_msg.contains(" INBOX\r\n"));
    assert!(!srv_msg.contains(" archive\r\n"));

    Ok(())
}

fn select_inbox(imap: &mut TcpStream) -> Result<()> {
    let mut buffer: [u8; 6000] = [0; 6000];

    imap.write(&b"20 select inbox\r\n"[..])?;
    let _read = read_lines(imap, &mut buffer, Some(&b"20 OK"[..]))?;

    Ok(())
}

fn check(imap: &mut TcpStream) -> Result<()> {
    let mut buffer: [u8; 1500] = [0; 1500];

    imap.write(&b"21 check\r\n"[..])?;
    let _read = read_lines(imap, &mut buffer, Some(&b"21 OK"[..]))?;

    Ok(())
}

fn status_mailbox(imap: &mut TcpStream) -> Result<()> {
    imap.write(&b"25 STATUS archive (UIDNEXT MESSAGES)\r\n"[..])?;
    let mut buffer: [u8; 6000] = [0; 6000];
    let _read = read_lines(imap, &mut buffer, Some(&b"25 OK"[..]))?;

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

fn fetch_rfc822(imap: &mut TcpStream, ref_mail: &[u8]) -> Result<()> {
    let mut buffer: [u8; 65535] = [0; 65535];
    imap.write(&b"40 fetch 1 rfc822\r\n"[..])?;

    let read = read_lines(imap, &mut buffer, Some(&b"40 OK FETCH"[..]))?;
    let srv_msg = std::str::from_utf8(read)?;
    let orig_email = std::str::from_utf8(ref_mail)?;
    assert!(srv_msg.contains(orig_email));

    Ok(())
}

fn copy_email(imap: &mut TcpStream) -> Result<()> {
    let mut buffer: [u8; 65535] = [0; 65535];
    imap.write(&b"45 copy 1 archive\r\n"[..])?;
    let read = read_lines(imap, &mut buffer, None)?;
    assert_eq!(&read[..5], &b"45 OK"[..]);

    Ok(())
}

fn append_email(imap: &mut TcpStream, ref_mail: &[u8]) -> Result<()> {
    let mut buffer: [u8; 6000] = [0; 6000];
    assert_ne!(ref_mail.len(), 0);
    let append_cmd = format!("47 append inbox (\\Seen) {{{}}}\r\n", ref_mail.len());
    println!("append cmd: {}", append_cmd);
    imap.write(append_cmd.as_bytes())?;

    // wait for continuation
    let read = read_lines(imap, &mut buffer, None)?;
    assert_eq!(read[0], b'+');

    // write our stuff
    imap.write(ref_mail)?;
    imap.write(&b"\r\n"[..])?;
    let read = read_lines(imap, &mut buffer, None)?;
    assert_eq!(&read[..5], &b"47 OK"[..]);

    // noop to force a sync
    imap.write(&b"48 NOOP\r\n"[..])?;
    let _read = read_lines(imap, &mut buffer, Some(&b"48 OK NOOP"[..]))?;

    // check it is stored successfully
    imap.write(&b"49 fetch 2 rfc822.size\r\n"[..])?;
    let read = read_lines(imap, &mut buffer, Some(&b"49 OK"[..]))?;
    let expected = format!("* 2 FETCH (RFC822.SIZE {})", ref_mail.len());
    let expbytes = expected.as_bytes();
    assert_eq!(&read[..expbytes.len()], expbytes);

    Ok(())
}

fn add_flags_email(imap: &mut TcpStream) -> Result<()> {
    let mut buffer: [u8; 1500] = [0; 1500];
    imap.write(&b"50 store 1 +FLAGS (\\Deleted \\Important)\r\n"[..])?;
    let _read = read_lines(imap, &mut buffer, Some(&b"50 OK STORE"[..]))?;

    Ok(())
}

#[allow(dead_code)]
/// Not yet implemented
fn search(imap: &mut TcpStream) -> Result<()> {
    imap.write(&b"55 search text \"OoOoO\"\r\n"[..])?;
    let mut buffer: [u8; 1500] = [0; 1500];
    let _read = read_lines(imap, &mut buffer, Some(&b"55 OK SEARCH"[..]))?;
    Ok(())
}

fn expunge(imap: &mut TcpStream) -> Result<()> {
    imap.write(&b"60 expunge\r\n"[..])?;
    let mut buffer: [u8; 1500] = [0; 1500];
    let _read = read_lines(imap, &mut buffer, Some(&b"60 OK EXPUNGE"[..]))?;

    Ok(())
}

fn rename_mailbox(imap: &mut TcpStream) -> Result<()> {
    imap.write(&b"70 rename archive my-archives\r\n"[..])?;
    let mut buffer: [u8; 1500] = [0; 1500];
    let read = read_lines(imap, &mut buffer, None)?;
    assert_eq!(&read[..5], &b"70 OK"[..]);

    imap.write(&b"71 list \"\" *\r\n"[..])?;
    let read = read_lines(imap, &mut buffer, Some(&b"71 OK LIST"[..]))?;
    let srv_msg = std::str::from_utf8(read)?;
    assert!(!srv_msg.contains(" archive\r\n"));
    assert!(srv_msg.contains(" INBOX\r\n"));
    assert!(srv_msg.contains(" my-archives\r\n"));

    Ok(())
}

fn delete_mailbox(imap: &mut TcpStream) -> Result<()> {
    imap.write(&b"80 delete my-archives\r\n"[..])?;
    let mut buffer: [u8; 1500] = [0; 1500];
    let read = read_lines(imap, &mut buffer, None)?;
    assert_eq!(&read[..5], &b"80 OK"[..]);

    imap.write(&b"81 list \"\" *\r\n"[..])?;
    let read = read_lines(imap, &mut buffer, Some(&b"81 OK LIST"[..]))?;
    let srv_msg = std::str::from_utf8(read)?;
    assert!(!srv_msg.contains(" archive\r\n"));
    assert!(!srv_msg.contains(" my-archives\r\n"));
    assert!(srv_msg.contains(" INBOX\r\n"));

    Ok(())
}

fn read_lines<'a, F: Read>(
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
