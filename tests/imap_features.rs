use std::process::Command;
use std::net::{Shutdown, TcpStream};
use std::{thread, time};
use anyhow::{bail, Result};
use std::io::{Write, Read};

static SMALL_DELAY: time::Duration = time::Duration::from_millis(200);
static EMAIL: &[u8] = b"Date: Sat, 8 Jul 2023 07:14:29 +0200\r
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


fn main() {
    let mut daemon = Command::new(env!("CARGO_BIN_EXE_aerogramme"))
        .arg("--dev")
        .arg("provider")
        .arg("daemon")
        .spawn()
        .expect("daemon should be started");

    let mut max_retry = 20;
    let mut imap_socket = loop {
        max_retry -= 1;
        match (TcpStream::connect("[::1]:1143"), max_retry) {
            (Err(e), 0) => panic!("no more retry, last error is: {}", e),
            (Err(e), _) => {
                println!("unable to connect: {} ; will retry in 1 sec", e);
            },
            (Ok(v), _) => break v,
        }
        thread::sleep(SMALL_DELAY);
    };

    let mut lmtp_socket = TcpStream::connect("[::1]:1025").expect("lmtp socket must be connected");

    println!("-- ready to test imap features --");
    login(&mut imap_socket).expect("login test");
    select_inbox(&mut imap_socket).expect("select inbox");
    lmtp_handshake(&mut lmtp_socket).expect("handshake lmtp done");
    lmtp_deliver_email(&mut lmtp_socket, EMAIL).expect("mail delivered successfully");
    noop_exists(&mut imap_socket).expect("noop loop must detect a new email");
    fetch_rfc822(&mut imap_socket, EMAIL).expect("fecth rfc822 message");

    println!("-- test teardown --");

    imap_socket.shutdown(Shutdown::Both).expect("closing imap socket at the end of the test");
    lmtp_socket.shutdown(Shutdown::Both).expect("closing lmtp socket at the end of the test");
    daemon.kill().expect("daemon should be killed");
}

fn login(imap: &mut TcpStream) -> Result<()> {
    let mut buffer: [u8; 1500] = [0; 1500];

    let read = read_lines(imap, &mut buffer, None)?;
    assert_eq!(&read[..4], &b"* OK"[..]);

    imap.write(&b"10 login alice hunter2\r\n"[..])?;

    let read = read_lines(imap, &mut buffer, None)?;
    assert_eq!(&read[..5], &b"10 OK"[..]);

    Ok(())
}

fn select_inbox(imap: &mut TcpStream) -> Result<()> {
    let mut buffer: [u8; 6000] = [0; 6000];

    imap.write(&b"20 select inbox\r\n"[..])?;
    let _read = read_lines(imap, &mut buffer, Some(&b"20 OK"[..]))?;

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
    };

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


fn read_lines<'a, F: Read>(reader: &mut F, buffer: &'a mut [u8], stop_marker: Option<&[u8]>) -> Result<&'a [u8]> {
    let mut nbytes = 0;
    loop {
        nbytes += reader.read(&mut buffer[nbytes..])?;
        let pre_condition = match stop_marker {
            None => true,
            Some(mark) => buffer[..nbytes]
                .windows(mark.len())
                .any(|w| w == mark)
        };
        if pre_condition && &buffer[nbytes-2..nbytes] == &b"\r\n"[..] {
            break
        }
    }
    println!("read: {}", std::str::from_utf8(&buffer[..nbytes])?);
    Ok(&buffer[..nbytes])
}
