use std::process::Command;
use std::net::{Shutdown, TcpStream};
use std::{thread, time};
use anyhow::Result;
use std::io::{Write, Read};

static ONE_SEC: time::Duration = time::Duration::from_secs(1);

fn main() {
    let mut daemon = Command::new(env!("CARGO_BIN_EXE_aerogramme"))
        .arg("--dev")
        .arg("provider")
        .arg("daemon")
        .spawn()
        .expect("daemon should be started");

    let mut max_retry = 10;
    let mut imap_socket = loop {
        max_retry -= 1;
        match (TcpStream::connect("[::1]:1143"), max_retry) {
            (Err(e), 0) => panic!("no more retry, last error is: {}", e),
            (Err(e), _) => {
                println!("unable to connect: {} ; will retry in 1 sec", e);
            },
            (Ok(v), _) => break v,
        }
        thread::sleep(ONE_SEC);
    };

    let mut lmtp_socket = TcpStream::connect("[::1]:1025").expect("lmtp socket must be connected");

    println!("-- ready to test imap features --");
    login(&mut imap_socket).expect("login test");
    select_inbox(&mut imap_socket).expect("select inbox");
    inject_email(&mut lmtp_socket).expect("inject email");

    println!("-- test teardown --");

    imap_socket.shutdown(Shutdown::Both).expect("closing imap socket at the end of the test");
    lmtp_socket.shutdown(Shutdown::Both).expect("closing lmtp socket at the end of the test");
    daemon.kill().expect("daemon should be killed");
}

fn login(imap: &mut TcpStream) -> Result<()> {
    let mut buffer: [u8; 1500] = [0; 1500];
    let mut nbytes = 0;
    loop {
        nbytes += imap.read(&mut buffer)?;
        if buffer[..nbytes].windows(2).any(|w| w == &b"\r\n"[..]) {
            break
        }
    }
    println!("read: {}", std::str::from_utf8(&buffer[..nbytes])?);
    assert_eq!(&buffer[..4], &b"* OK"[..]);
    assert_eq!(&buffer[nbytes-2..nbytes], &b"\r\n"[..]);

    imap.write(&b"10 login alice hunter2\r\n"[..])?;

    let mut nbytes = 0;
    loop {
        nbytes += imap.read(&mut buffer)?;
        if &buffer[nbytes-2..nbytes] == &b"\r\n"[..] {
            break
        }
    }
    println!("read: {}", std::str::from_utf8(&buffer[..nbytes])?);
    assert_eq!(&buffer[..5], &b"10 OK"[..]);

    Ok(())
}

fn select_inbox(imap: &mut TcpStream) -> Result<()> {
    let mut buffer: [u8; 6000] = [0; 6000];

    imap.write(&b"20 select inbox\r\n"[..])?;

    let mut nbytes = 0;
    loop {
        nbytes += imap.read(&mut buffer)?;
        if buffer[..nbytes].windows(5).any(|w| w == &b"20 OK"[..]) && &buffer[nbytes-2..nbytes] == &b"\r\n"[..] {
            break
        }
    }
    println!("read: {}", std::str::from_utf8(&buffer[..nbytes])?);

    Ok(())
}

fn inject_email(lmtp: &mut TcpStream) -> Result<()> {

    Ok(())
}
