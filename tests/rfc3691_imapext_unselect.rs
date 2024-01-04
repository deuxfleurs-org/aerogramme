use anyhow::Context;

mod common;
use crate::common::fragments::*;

fn main() {
    common::aerogramme_provider_daemon_dev(|imap_socket, lmtp_socket| {
        connect(imap_socket).context("server says hello")?;

        lmtp_handshake(lmtp_socket).context("handshake lmtp done")?;
        lmtp_deliver_email(lmtp_socket, Email::Basic).context("mail delivered successfully")?;

        capability(imap_socket, Extension::Unselect).context("check server capabilities")?;
        login(imap_socket, Account::Alice).context("login test")?;
        select(imap_socket, Mailbox::Inbox, None).context("select inbox")?;
        noop_exists(imap_socket).context("noop loop must detect a new email")?;
        add_flags_email(imap_socket, Selection::FirstId, Flag::Deleted).context("add delete flags to the email")?;
        unselect(imap_socket)
            .context("unselect inbox while preserving email with the \\Delete flag")?;
        select(imap_socket, Mailbox::Inbox, Some(1)).context("select inbox again")?;
        fetch_rfc822(imap_socket, Selection::FirstId, Email::Basic).context("message is still present")?;
        close(imap_socket).context("close inbox and expunge message")?;
        select(imap_socket, Mailbox::Inbox, Some(0)).context("select inbox again and check it's empty")?;

        Ok(())
    })
    .expect("test fully run");
}
