use anyhow::{bail, Result};
use std::io::Write;
use std::net::TcpStream;
use std::thread;

use crate::common::constants::*;
use crate::common::*;

/// These fragments are not a generic IMAP client
/// but specialized to our specific tests. They can't take
/// arbitrary values, only enum for which the code is known
/// to be correct. The idea is that the generated message is more
/// or less hardcoded by the developer, so its clear what's expected,
/// and not generated by a library.

pub fn connect(imap: &mut TcpStream) -> Result<()> {
    let mut buffer: [u8; 1500] = [0; 1500];

    let read = read_lines(imap, &mut buffer, None)?;
    assert_eq!(&read[..4], &b"* OK"[..]);

    Ok(())
}

pub enum Account {
    Alice,
}

pub enum Extension {
    None,
    Unselect,
    Move,
    CondStore,
}

pub enum Mailbox {
    Inbox,
    Archive,
    Drafts,
}

pub enum Flag {
    Deleted,
    Important
}

pub enum Email {
    Basic,
    Multipart,
}

pub enum Selection {
    FirstId,
    SecondId,
}

pub fn capability(imap: &mut TcpStream, ext: Extension) -> Result<()> {
    imap.write(&b"5 capability\r\n"[..])?;

    let maybe_ext = match ext {
        Extension::None => None,
        Extension::Unselect => Some("UNSELECT"),
        Extension::Move => Some("MOVE"),
        Extension::CondStore => Some("CONDSTORE"),
    };

    let mut buffer: [u8; 1500] = [0; 1500];
    let read = read_lines(imap, &mut buffer, Some(&b"5 OK"[..]))?;
    let srv_msg = std::str::from_utf8(read)?;
    assert!(srv_msg.contains("IMAP4REV1"));
    if let Some(ext) = maybe_ext {
        assert!(srv_msg.contains(ext));
    }

    Ok(())
}

pub fn login(imap: &mut TcpStream, account: Account) -> Result<()> {
    let mut buffer: [u8; 1500] = [0; 1500];

    assert!(matches!(account, Account::Alice));
    imap.write(&b"10 login alice hunter2\r\n"[..])?;

    let read = read_lines(imap, &mut buffer, None)?;
    assert_eq!(&read[..5], &b"10 OK"[..]);

    Ok(())
}

pub fn create_mailbox(imap: &mut TcpStream, mbx: Mailbox) -> Result<()> {
    let mut buffer: [u8; 1500] = [0; 1500];

    let mbx_str = match mbx {
        Mailbox::Inbox => "INBOX",
        Mailbox::Archive => "Archive",
        Mailbox::Drafts => "Drafts",
    };

    let cmd = format!("15 create {}\r\n", mbx_str);
    imap.write(cmd.as_bytes())?;
    let read = read_lines(imap, &mut buffer, None)?;
    assert_eq!(&read[..12], &b"15 OK CREATE"[..]);

    Ok(())
}

pub fn select(imap: &mut TcpStream, mbx: Mailbox, maybe_exists: Option<u64>) -> Result<()> {
    let mut buffer: [u8; 6000] = [0; 6000];

    let mbx_str = match mbx {
        Mailbox::Inbox => "INBOX",
        Mailbox::Archive => "Archive",
        Mailbox::Drafts => "Drafts",
    };
    imap.write(format!("20 select {}\r\n", mbx_str).as_bytes())?;

    let read = read_lines(imap, &mut buffer, Some(&b"20 OK"[..]))?;
    let srv_msg = std::str::from_utf8(read)?;
    if let Some(exists) = maybe_exists {
        let expected = format!("* {} EXISTS", exists);
        assert!(srv_msg.contains(&expected));
    }

    Ok(())
}

pub fn unselect(imap: &mut TcpStream) -> Result<()> {
    imap.write(&b"70 unselect\r\n"[..])?;
    let mut buffer: [u8; 1500] = [0; 1500];
    let _read = read_lines(imap, &mut buffer, Some(&b"70 OK"[..]))?;

    Ok(())
}

pub fn check(imap: &mut TcpStream) -> Result<()> {
    let mut buffer: [u8; 1500] = [0; 1500];

    imap.write(&b"21 check\r\n"[..])?;
    let _read = read_lines(imap, &mut buffer, Some(&b"21 OK"[..]))?;

    Ok(())
}

pub fn status_mailbox(imap: &mut TcpStream, mbx: Mailbox) -> Result<()> {
    assert!(matches!(mbx, Mailbox::Archive));
    imap.write(&b"25 STATUS Archive (UIDNEXT MESSAGES)\r\n"[..])?;
    let mut buffer: [u8; 6000] = [0; 6000];
    let _read = read_lines(imap, &mut buffer, Some(&b"25 OK"[..]))?;

    Ok(())
}

pub fn lmtp_handshake(lmtp: &mut TcpStream) -> Result<()> {
    let mut buffer: [u8; 1500] = [0; 1500];

    let _read = read_lines(lmtp, &mut buffer, None)?;
    assert_eq!(&buffer[..4], &b"220 "[..]);

    lmtp.write(&b"LHLO example.tld\r\n"[..])?;
    let _read = read_lines(lmtp, &mut buffer, Some(&b"250 "[..]))?;

    Ok(())
}

pub fn lmtp_deliver_email(lmtp: &mut TcpStream, email_type: Email) -> Result<()> {
    let mut buffer: [u8; 1500] = [0; 1500];

    let email = match email_type {
        Email::Basic => EMAIL2,
        Email::Multipart => EMAIL1,
    };
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

pub fn noop_exists(imap: &mut TcpStream) -> Result<()> {
    let mut buffer: [u8; 6000] = [0; 6000];

    let mut max_retry = 20;
    loop {
        max_retry -= 1;
        imap.write(&b"30 NOOP\r\n"[..])?;
        let read = read_lines(imap, &mut buffer, Some(&b"30 OK"[..]))?;
        let srv_msg = std::str::from_utf8(read)?;

        match (max_retry, srv_msg.lines().count()) {
            (_, cnt) if cnt > 1 => break,
            (0, _) => bail!("no more retry"),
            _ => (),
        }

        thread::sleep(SMALL_DELAY);
    }

    Ok(())
}

pub fn fetch_rfc822(imap: &mut TcpStream, selection: Selection, r#ref: Email) -> Result<()> {
    let mut buffer: [u8; 65535] = [0; 65535];

    assert!(matches!(selection, Selection::FirstId));
    imap.write(&b"40 fetch 1 rfc822\r\n"[..])?;

    let read = read_lines(imap, &mut buffer, Some(&b"40 OK FETCH"[..]))?;
    let srv_msg = std::str::from_utf8(read)?;

    let ref_mail = match r#ref {
        Email::Basic => EMAIL2,
        Email::Multipart => EMAIL1,
    };
    let orig_email = std::str::from_utf8(ref_mail)?;
    assert!(srv_msg.contains(orig_email));

    Ok(())
}

pub fn copy(imap: &mut TcpStream, selection: Selection, to: Mailbox) -> Result<()> {
    let mut buffer: [u8; 65535] = [0; 65535];
    assert!(matches!(selection, Selection::FirstId));
    assert!(matches!(to, Mailbox::Archive));

    imap.write(&b"45 copy 1 Archive\r\n"[..])?;
    let read = read_lines(imap, &mut buffer, None)?;
    assert_eq!(&read[..5], &b"45 OK"[..]);

    Ok(())
}

pub fn append_email(imap: &mut TcpStream, content: Email) -> Result<()> {
    let mut buffer: [u8; 6000] = [0; 6000];

    let ref_mail = match content {
        Email::Multipart => EMAIL1,
        Email::Basic => EMAIL2,
    };

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

    // we check that noop detects the change
    noop_exists(imap)?;

    Ok(())
}



pub fn add_flags_email(imap: &mut TcpStream, selection: Selection, flag: Flag) -> Result<()> {
    let mut buffer: [u8; 1500] = [0; 1500];
    assert!(matches!(selection, Selection::FirstId));
    assert!(matches!(flag, Flag::Deleted));
    imap.write(&b"50 store 1 +FLAGS (\\Deleted)\r\n"[..])?;
    let _read = read_lines(imap, &mut buffer, Some(&b"50 OK STORE"[..]))?;

    Ok(())
}

#[allow(dead_code)]
/// Not yet implemented
pub fn search(imap: &mut TcpStream) -> Result<()> {
    imap.write(&b"55 search text \"OoOoO\"\r\n"[..])?;
    let mut buffer: [u8; 1500] = [0; 1500];
    let _read = read_lines(imap, &mut buffer, Some(&b"55 OK SEARCH"[..]))?;
    Ok(())
}

pub fn expunge(imap: &mut TcpStream) -> Result<()> {
    imap.write(&b"60 expunge\r\n"[..])?;
    let mut buffer: [u8; 1500] = [0; 1500];
    let _read = read_lines(imap, &mut buffer, Some(&b"60 OK EXPUNGE"[..]))?;

    Ok(())
}

pub fn rename_mailbox(imap: &mut TcpStream, from: Mailbox, to: Mailbox) -> Result<()> {
    assert!(matches!(from, Mailbox::Archive));
    assert!(matches!(to, Mailbox::Drafts));

    imap.write(&b"70 rename Archive Drafts\r\n"[..])?;
    let mut buffer: [u8; 1500] = [0; 1500];
    let read = read_lines(imap, &mut buffer, None)?;
    assert_eq!(&read[..5], &b"70 OK"[..]);

    imap.write(&b"71 list \"\" *\r\n"[..])?;
    let read = read_lines(imap, &mut buffer, Some(&b"71 OK LIST"[..]))?;
    let srv_msg = std::str::from_utf8(read)?;
    assert!(!srv_msg.contains(" Archive\r\n"));
    assert!(srv_msg.contains(" INBOX\r\n"));
    assert!(srv_msg.contains(" Drafts\r\n"));

    Ok(())
}

pub fn delete_mailbox(imap: &mut TcpStream, mbx: Mailbox) -> Result<()> {
    let mbx_str = match mbx {
        Mailbox::Inbox => "INBOX",
        Mailbox::Archive => "Archive",
        Mailbox::Drafts => "Drafts",
    };
    let cmd = format!("80 delete {}\r\n", mbx_str);

    imap.write(cmd.as_bytes())?;
    let mut buffer: [u8; 1500] = [0; 1500];
    let read = read_lines(imap, &mut buffer, None)?;
    assert_eq!(&read[..5], &b"80 OK"[..]);

    imap.write(&b"81 list \"\" *\r\n"[..])?;
    let read = read_lines(imap, &mut buffer, Some(&b"81 OK"[..]))?;
    let srv_msg = std::str::from_utf8(read)?;
    assert!(srv_msg.contains(" INBOX\r\n"));
    assert!(!srv_msg.contains(format!(" {}\r\n", mbx_str).as_str()));

    Ok(())
}

pub fn close(imap: &mut TcpStream) -> Result<()> {
    imap.write(&b"60 close\r\n"[..])?;
    let mut buffer: [u8; 1500] = [0; 1500];
    let _read = read_lines(imap, &mut buffer, Some(&b"60 OK"[..]))?;

    Ok(())
}

pub fn r#move(imap: &mut TcpStream, selection: Selection, to: Mailbox) -> Result<()> {
    let mut buffer: [u8; 1500] = [0; 1500];
    assert!(matches!(to, Mailbox::Archive));
    assert!(matches!(selection, Selection::FirstId));

    imap.write(&b"35 move 1 Archive\r\n"[..])?;
    let read = read_lines(imap, &mut buffer, Some(&b"35 OK"[..]))?;
    let srv_msg = std::str::from_utf8(read)?;
    assert!(srv_msg.contains("* 1 EXPUNGE"));

    Ok(())
}

pub fn logout(imap: &mut TcpStream) -> Result<()> {
    imap.write(&b"99 logout\r\n"[..])?;
    let mut buffer: [u8; 1500] = [0; 1500];
    let read = read_lines(imap, &mut buffer, None)?;
    assert_eq!(&read[..5], &b"* BYE"[..]);
    Ok(())
}
