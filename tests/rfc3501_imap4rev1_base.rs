use anyhow::Context;

mod common;
use crate::common::fragments::*;

fn main() {
    common::aerogramme_provider_daemon_dev(|imap_socket, lmtp_socket| {
        connect(imap_socket).context("server says hello")?;
        capability(imap_socket, Extension::None).context("check server capabilities")?;
        login(imap_socket, Account::Alice).context("login test")?;
        create_mailbox(imap_socket, Mailbox::Archive).context("created mailbox archive")?;
        // UNSUBSCRIBE IS NOT IMPLEMENTED YET
        //unsubscribe_mailbox(imap_socket).context("unsubscribe from archive")?;
        select(imap_socket, Mailbox::Inbox, None).context("select inbox")?;
        check(imap_socket).context("check must run")?;
        status_mailbox(imap_socket, Mailbox::Archive).context("status of archive from inbox")?;
        lmtp_handshake(lmtp_socket).context("handshake lmtp done")?;
        lmtp_deliver_email(lmtp_socket, Email::Multipart).context("mail delivered successfully")?;
        noop_exists(imap_socket).context("noop loop must detect a new email")?;
        fetch_rfc822(imap_socket, Selection::FirstId, Email::Multipart).context("fetch rfc822 message, should be our first message")?;
        copy(imap_socket, Selection::FirstId, Mailbox::Archive).context("copy message to the archive mailbox")?;
        append_email(imap_socket, Email::Basic).context("insert email in INBOX")?;
        // SEARCH IS NOT IMPLEMENTED YET
        //search(imap_socket).expect("search should return something");
        add_flags_email(imap_socket, Selection::FirstId, Flag::Deleted)
            .context("should add delete flag to the email")?;
        expunge(imap_socket).context("expunge emails")?;
        rename_mailbox(imap_socket, Mailbox::Archive, Mailbox::Drafts).context("Archive mailbox is renamed Drafts")?;
        delete_mailbox(imap_socket, Mailbox::Drafts).context("Drafts mailbox is deleted")?;
        Ok(())
    })
    .expect("test fully run");
}
