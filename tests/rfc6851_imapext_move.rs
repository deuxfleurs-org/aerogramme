use anyhow::Context;

mod common;
use common::fragments::*;

fn main() {
    common::aerogramme_provider_daemon_dev(|imap_socket, lmtp_socket| {
        connect(imap_socket).context("server says hello")?;

        capability(imap_socket, Extension::Move).context("check server capabilities")?;
        login(imap_socket, Account::Alice).context("login test")?;
        create_mailbox(imap_socket, Mailbox::Archive).context("created mailbox archive")?;
        select(imap_socket, Mailbox::Inbox, None).context("select inbox")?;

        lmtp_handshake(lmtp_socket).context("handshake lmtp done")?;
        lmtp_deliver_email(lmtp_socket, Email::Basic).context("mail delivered successfully")?;

        noop_exists(imap_socket).context("noop loop must detect a new email")?;
        r#move(imap_socket, Selection::FirstId, Mailbox::Archive).context("message from inbox moved to archive")?;

        unselect(imap_socket)
            .context("unselect inbox while preserving email with the \\Delete flag")?;
        select(imap_socket, Mailbox::Archive, Some(1)).context("select archive")?;
        fetch_rfc822(imap_socket, Selection::FirstId, Email::Basic).context("check mail exists")?;
        logout(imap_socket).context("must quit")?;

        Ok(())
    })
    .expect("test fully run");
}
