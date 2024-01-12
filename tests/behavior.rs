use anyhow::Context;

mod common;
use crate::common::fragments::*;

fn main() {
    rfc3501_imap4rev1_base();
    rfc3691_imapext_unselect();
    rfc5161_imapext_enable();
    rfc6851_imapext_move();
    rfc7888_imapext_literal();
    rfc4551_imapext_condstore();
    println!("✅ SUCCESS 🌟🚀🥳🙏🥹");
}

fn rfc3501_imap4rev1_base() {
    println!("🧪 rfc3501_imap4rev1_base");
    common::aerogramme_provider_daemon_dev(|imap_socket, lmtp_socket| {
        connect(imap_socket).context("server says hello")?;
        capability(imap_socket, Extension::None).context("check server capabilities")?;
        login(imap_socket, Account::Alice).context("login test")?;
        create_mailbox(imap_socket, Mailbox::Archive).context("created mailbox archive")?;
        // UNSUBSCRIBE IS NOT IMPLEMENTED YET
        //unsubscribe_mailbox(imap_socket).context("unsubscribe from archive")?;
        select(imap_socket, Mailbox::Inbox, SelectMod::None, None).context("select inbox")?;
        check(imap_socket).context("check must run")?;
        status_mailbox(imap_socket, Mailbox::Archive).context("status of archive from inbox")?;
        lmtp_handshake(lmtp_socket).context("handshake lmtp done")?;
        lmtp_deliver_email(lmtp_socket, Email::Multipart).context("mail delivered successfully")?;
        noop_exists(imap_socket, 1).context("noop loop must detect a new email")?;
        fetch_rfc822(imap_socket, Selection::FirstId, Email::Multipart)
            .context("fetch rfc822 message, should be our first message")?;
        copy(imap_socket, Selection::FirstId, Mailbox::Archive)
            .context("copy message to the archive mailbox")?;
        append_email(imap_socket, Email::Basic).context("insert email in INBOX")?;
        noop_exists(imap_socket, 2).context("noop loop must detect a new email")?;
        // Missing STORE command
        search(imap_socket).expect("search should return something");
        store(imap_socket, Selection::FirstId, Flag::Deleted, StoreAction::AddFlags, StoreMod::None)
            .context("should add delete flag to the email")?;
        expunge(imap_socket).context("expunge emails")?;
        rename_mailbox(imap_socket, Mailbox::Archive, Mailbox::Drafts)
            .context("Archive mailbox is renamed Drafts")?;
        delete_mailbox(imap_socket, Mailbox::Drafts).context("Drafts mailbox is deleted")?;
        Ok(())
    })
    .expect("test fully run");
}

fn rfc3691_imapext_unselect() {
    println!("🧪 rfc3691_imapext_unselect");
    common::aerogramme_provider_daemon_dev(|imap_socket, lmtp_socket| {
        connect(imap_socket).context("server says hello")?;

        lmtp_handshake(lmtp_socket).context("handshake lmtp done")?;
        lmtp_deliver_email(lmtp_socket, Email::Basic).context("mail delivered successfully")?;

        capability(imap_socket, Extension::Unselect).context("check server capabilities")?;
        login(imap_socket, Account::Alice).context("login test")?;
        select(imap_socket, Mailbox::Inbox, SelectMod::None, None).context("select inbox")?;
        noop_exists(imap_socket, 1).context("noop loop must detect a new email")?;
        store(imap_socket, Selection::FirstId, Flag::Deleted, StoreAction::AddFlags, StoreMod::None)
            .context("add delete flags to the email")?;
        unselect(imap_socket)
            .context("unselect inbox while preserving email with the \\Delete flag")?;
        select(imap_socket, Mailbox::Inbox, SelectMod::None, Some(1)).context("select inbox again")?;
        fetch_rfc822(imap_socket, Selection::FirstId, Email::Basic)
            .context("message is still present")?;
        close(imap_socket).context("close inbox and expunge message")?;
        select(imap_socket, Mailbox::Inbox, SelectMod::None, Some(0))
            .context("select inbox again and check it's empty")?;

        Ok(())
    })
    .expect("test fully run");
}

fn rfc5161_imapext_enable() {
    println!("🧪 rfc5161_imapext_enable");
    common::aerogramme_provider_daemon_dev(|imap_socket, _lmtp_socket| {
        connect(imap_socket).context("server says hello")?;
        login(imap_socket, Account::Alice).context("login test")?;
        enable(imap_socket, Enable::Utf8Accept, Some(Enable::Utf8Accept))?;
        enable(imap_socket, Enable::Utf8Accept, None)?;
        logout(imap_socket)?;

        Ok(())
    })
    .expect("test fully run");
}

fn rfc6851_imapext_move() {
    println!("🧪 rfc6851_imapext_move");
    common::aerogramme_provider_daemon_dev(|imap_socket, lmtp_socket| {
        connect(imap_socket).context("server says hello")?;

        capability(imap_socket, Extension::Move).context("check server capabilities")?;
        login(imap_socket, Account::Alice).context("login test")?;
        create_mailbox(imap_socket, Mailbox::Archive).context("created mailbox archive")?;
        select(imap_socket, Mailbox::Inbox, SelectMod::None, None).context("select inbox")?;

        lmtp_handshake(lmtp_socket).context("handshake lmtp done")?;
        lmtp_deliver_email(lmtp_socket, Email::Basic).context("mail delivered successfully")?;

        noop_exists(imap_socket, 1).context("noop loop must detect a new email")?;
        r#move(imap_socket, Selection::FirstId, Mailbox::Archive)
            .context("message from inbox moved to archive")?;

        unselect(imap_socket)
            .context("unselect inbox while preserving email with the \\Delete flag")?;
        select(imap_socket, Mailbox::Archive, SelectMod::None, Some(1)).context("select archive")?;
        fetch_rfc822(imap_socket, Selection::FirstId, Email::Basic).context("check mail exists")?;
        logout(imap_socket).context("must quit")?;

        Ok(())
    })
    .expect("test fully run");
}

fn rfc7888_imapext_literal() {
    println!("🧪 rfc7888_imapext_literal");
    common::aerogramme_provider_daemon_dev(|imap_socket, _lmtp_socket| {
        connect(imap_socket).context("server says hello")?;

        capability(imap_socket, Extension::LiteralPlus).context("check server capabilities")?;
        login_with_literal(imap_socket, Account::Alice).context("use literal to connect Alice")?;

        Ok(())
    })
    .expect("test fully run");
}

fn rfc4551_imapext_condstore() {
    println!("🧪 rfc4551_imapext_condstore");
    common::aerogramme_provider_daemon_dev(|imap_socket, lmtp_socket| {
        // Setup the test
        connect(imap_socket).context("server says hello")?;
        capability(imap_socket, Extension::Condstore).context("check server capabilities")?;
        login(imap_socket, Account::Alice).context("login test")?;

        // Check that the condstore modifier works
        select(imap_socket, Mailbox::Inbox, SelectMod::Condstore, None).context("select inbox")?;


        lmtp_handshake(lmtp_socket).context("handshake lmtp done")?;
        lmtp_deliver_email(lmtp_socket, Email::Basic).context("mail delivered successfully")?;
        lmtp_deliver_email(lmtp_socket, Email::Multipart).context("mail delivered successfully")?;
        noop_exists(imap_socket, 2).context("noop loop must detect a new email")?;



        //store(

        Ok(())
    })
    .expect("test fully run");
}
