use anyhow::Context;

mod common;
use crate::common::constants::*;
use crate::common::fragments::*;

fn main() {
    rfc3501_imap4rev1_base();
    rfc6851_imapext_move();
    rfc4551_imapext_condstore();
    rfc2177_imapext_idle();
    rfc5161_imapext_enable(); // 1
    rfc3691_imapext_unselect(); // 2
    rfc7888_imapext_literal(); // 3
    rfc4315_imapext_uidplus(); // 4
    rfc5819_imapext_liststatus(); // 5
    println!("✅ SUCCESS 🌟🚀🥳🙏🥹");
}

fn rfc3501_imap4rev1_base() {
    println!("🧪 rfc3501_imap4rev1_base");
    common::aerogramme_provider_daemon_dev(|imap_socket, lmtp_socket| {
        connect(imap_socket).context("server says hello")?;
        capability(imap_socket, Extension::None).context("check server capabilities")?;
        login(imap_socket, Account::Alice).context("login test")?;
        create_mailbox(imap_socket, Mailbox::Archive).context("created mailbox archive")?;
        let select_res =
            select(imap_socket, Mailbox::Inbox, SelectMod::None).context("select inbox")?;
        assert!(select_res.contains("* 0 EXISTS"));

        check(imap_socket).context("check must run")?;
        status(imap_socket, Mailbox::Archive, StatusKind::UidNext)
            .context("status of archive from inbox")?;
        lmtp_handshake(lmtp_socket).context("handshake lmtp done")?;
        lmtp_deliver_email(lmtp_socket, Email::Multipart).context("mail delivered successfully")?;
        noop_exists(imap_socket, 1).context("noop loop must detect a new email")?;

        let srv_msg = fetch(
            imap_socket,
            Selection::FirstId,
            FetchKind::Rfc822,
            FetchMod::None,
        )
        .context("fetch rfc822 message, should be our first message")?;
        let orig_email = std::str::from_utf8(EMAIL1)?;
        assert!(srv_msg.contains(orig_email));

        copy(imap_socket, Selection::FirstId, Mailbox::Archive)
            .context("copy message to the archive mailbox")?;
        append(imap_socket, Email::Basic).context("insert email in INBOX")?;
        noop_exists(imap_socket, 2).context("noop loop must detect a new email")?;
        search(imap_socket, SearchKind::Text("OoOoO")).expect("search should return something");
        store(
            imap_socket,
            Selection::FirstId,
            Flag::Deleted,
            StoreAction::AddFlags,
            StoreMod::None,
        )
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
        let select_res =
            select(imap_socket, Mailbox::Inbox, SelectMod::None).context("select inbox")?;
        assert!(select_res.contains("* 0 EXISTS"));

        noop_exists(imap_socket, 1).context("noop loop must detect a new email")?;
        store(
            imap_socket,
            Selection::FirstId,
            Flag::Deleted,
            StoreAction::AddFlags,
            StoreMod::None,
        )
        .context("add delete flags to the email")?;
        unselect(imap_socket)
            .context("unselect inbox while preserving email with the \\Delete flag")?;
        let select_res =
            select(imap_socket, Mailbox::Inbox, SelectMod::None).context("select inbox again")?;
        assert!(select_res.contains("* 1 EXISTS"));

        let srv_msg = fetch(
            imap_socket,
            Selection::FirstId,
            FetchKind::Rfc822,
            FetchMod::None,
        )
        .context("message is still present")?;
        let orig_email = std::str::from_utf8(EMAIL2)?;
        assert!(srv_msg.contains(orig_email));

        close(imap_socket).context("close inbox and expunge message")?;
        let select_res = select(imap_socket, Mailbox::Inbox, SelectMod::None)
            .context("select inbox again and check it's empty")?;
        assert!(select_res.contains("* 0 EXISTS"));

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
        let select_res =
            select(imap_socket, Mailbox::Inbox, SelectMod::None).context("select inbox")?;
        assert!(select_res.contains("* 0 EXISTS"));

        lmtp_handshake(lmtp_socket).context("handshake lmtp done")?;
        lmtp_deliver_email(lmtp_socket, Email::Basic).context("mail delivered successfully")?;

        noop_exists(imap_socket, 1).context("noop loop must detect a new email")?;
        r#move(imap_socket, Selection::FirstId, Mailbox::Archive)
            .context("message from inbox moved to archive")?;

        unselect(imap_socket)
            .context("unselect inbox while preserving email with the \\Delete flag")?;
        let select_res =
            select(imap_socket, Mailbox::Archive, SelectMod::None).context("select archive")?;
        assert!(select_res.contains("* 1 EXISTS"));

        let srv_msg = fetch(
            imap_socket,
            Selection::FirstId,
            FetchKind::Rfc822,
            FetchMod::None,
        )
        .context("check mail exists")?;
        let orig_email = std::str::from_utf8(EMAIL2)?;
        assert!(srv_msg.contains(orig_email));

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

        // RFC 3.1.1 Advertising Support for CONDSTORE
        capability(imap_socket, Extension::Condstore).context("check server capabilities")?;
        login(imap_socket, Account::Alice).context("login test")?;

        // RFC 3.1.8.  CONDSTORE Parameter to SELECT and EXAMINE
        let select_res =
            select(imap_socket, Mailbox::Inbox, SelectMod::Condstore).context("select inbox")?;
        // RFC 3.1.2   New OK Untagged Responses for SELECT and EXAMINE
        assert!(select_res.contains("[HIGHESTMODSEQ 1]"));

        // RFC 3.1.3.  STORE and UID STORE Commands
        lmtp_handshake(lmtp_socket).context("handshake lmtp done")?;
        lmtp_deliver_email(lmtp_socket, Email::Basic).context("mail delivered successfully")?;
        lmtp_deliver_email(lmtp_socket, Email::Multipart).context("mail delivered successfully")?;
        noop_exists(imap_socket, 2).context("noop loop must detect a new email")?;
        let store_res = store(
            imap_socket,
            Selection::All,
            Flag::Important,
            StoreAction::AddFlags,
            StoreMod::UnchangedSince(1),
        )?;
        assert!(store_res.contains("[MODIFIED 2]"));
        assert!(store_res.contains("* 1 FETCH (FLAGS (\\Important) MODSEQ (3))"));
        assert!(!store_res.contains("* 2 FETCH"));
        assert_eq!(store_res.lines().count(), 2);

        // RFC 3.1.4.  FETCH and UID FETCH Commands
        let fetch_res = fetch(
            imap_socket,
            Selection::All,
            FetchKind::Rfc822Size,
            FetchMod::ChangedSince(2),
        )?;
        assert!(fetch_res.contains("* 1 FETCH (RFC822.SIZE 84 MODSEQ (3))"));
        assert!(!fetch_res.contains("* 2 FETCH"));
        assert_eq!(store_res.lines().count(), 2);

        // RFC 3.1.5.  MODSEQ Search Criterion in SEARCH
        let search_res = search(imap_socket, SearchKind::ModSeq(3))?;
        // RFC 3.1.6.  Modified SEARCH Untagged Response
        assert!(search_res.contains("* SEARCH 1 (MODSEQ 3)"));

        // RFC 3.1.7   HIGHESTMODSEQ Status Data Items
        let status_res = status(imap_socket, Mailbox::Inbox, StatusKind::HighestModSeq)?;
        assert!(status_res.contains("HIGHESTMODSEQ 3"));

        Ok(())
    })
    .expect("test fully run");
}

fn rfc2177_imapext_idle() {
    println!("🧪 rfc2177_imapext_idle");
    common::aerogramme_provider_daemon_dev(|imap_socket, lmtp_socket| {
        // Test setup, check capability
        connect(imap_socket).context("server says hello")?;
        capability(imap_socket, Extension::Idle).context("check server capabilities")?;
        login(imap_socket, Account::Alice).context("login test")?;
        select(imap_socket, Mailbox::Inbox, SelectMod::None).context("select inbox")?;

        // Check that new messages from LMTP are correctly detected during idling
        start_idle(imap_socket).context("can't start idling")?;
        lmtp_handshake(lmtp_socket).context("handshake lmtp done")?;
        lmtp_deliver_email(lmtp_socket, Email::Basic).context("mail delivered successfully")?;
        let srv_msg = stop_idle(imap_socket).context("stop idling")?;
        assert!(srv_msg.contains("* 1 EXISTS"));

        Ok(())
    })
    .expect("test fully run");
}

fn rfc4315_imapext_uidplus() {
    println!("🧪 rfc4315_imapext_uidplus");
    common::aerogramme_provider_daemon_dev(|imap_socket, lmtp_socket| {
        // Test setup, check capability, insert 2 emails
        connect(imap_socket).context("server says hello")?;
        capability(imap_socket, Extension::UidPlus).context("check server capabilities")?;
        login(imap_socket, Account::Alice).context("login test")?;
        select(imap_socket, Mailbox::Inbox, SelectMod::None).context("select inbox")?;
        lmtp_handshake(lmtp_socket).context("handshake lmtp done")?;
        lmtp_deliver_email(lmtp_socket, Email::Basic).context("mail delivered successfully")?;
        lmtp_deliver_email(lmtp_socket, Email::Multipart).context("mail delivered successfully")?;
        noop_exists(imap_socket, 2).context("noop loop must detect a new email")?;

        // Check UID EXPUNGE seqset
        store(
            imap_socket,
            Selection::All,
            Flag::Deleted,
            StoreAction::AddFlags,
            StoreMod::None,
        )?;
        let res = uid_expunge(imap_socket, Selection::FirstId)?;
        assert_eq!(res.lines().count(), 2);
        assert!(res.contains("* 1 EXPUNGE"));

        // APPENDUID check UID + UID VALIDITY
        // Note: 4 and not 3, as we update the UID counter when we delete an email
        // it's part of our UID proof
        let res = append(imap_socket, Email::Multipart)?;
        assert!(res.contains("[APPENDUID 1 4]"));

        // COPYUID, check
        create_mailbox(imap_socket, Mailbox::Archive).context("created mailbox archive")?;
        let res = copy(imap_socket, Selection::FirstId, Mailbox::Archive)?;
        assert!(res.contains("[COPYUID 1 2 1]"));

        // MOVEUID, check
        let res = r#move(imap_socket, Selection::FirstId, Mailbox::Archive)?;
        assert!(res.contains("[COPYUID 1 2 2]"));

        Ok(())
    })
    .expect("test fully run");
}

///
/// Example
///
/// ```text
/// 30 list "" "*" RETURN (STATUS (MESSAGES UNSEEN))
/// * LIST (\Subscribed) "." INBOX
/// * STATUS INBOX (MESSAGES 2 UNSEEN 1)
/// 30 OK LIST completed
/// ```
fn rfc5819_imapext_liststatus() {
    println!("🧪 rfc5819_imapext_liststatus");
    common::aerogramme_provider_daemon_dev(|imap_socket, lmtp_socket| {
        // Test setup, check capability, add 2 emails, read 1
        connect(imap_socket).context("server says hello")?;
        capability(imap_socket, Extension::ListStatus).context("check server capabilities")?;
        login(imap_socket, Account::Alice).context("login test")?;
        select(imap_socket, Mailbox::Inbox, SelectMod::None).context("select inbox")?;
        lmtp_handshake(lmtp_socket).context("handshake lmtp done")?;
        lmtp_deliver_email(lmtp_socket, Email::Basic).context("mail delivered successfully")?;
        lmtp_deliver_email(lmtp_socket, Email::Multipart).context("mail delivered successfully")?;
        noop_exists(imap_socket, 2).context("noop loop must detect a new email")?;
        fetch(
            imap_socket,
            Selection::FirstId,
            FetchKind::Rfc822,
            FetchMod::None,
        )
        .context("read one message")?;
        close(imap_socket).context("close inbox")?;

        // Test return status MESSAGES UNSEEN
        let ret = list(
            imap_socket,
            MbxSelect::All,
            ListReturn::StatusMessagesUnseen,
        )?;
        assert!(ret.contains("* STATUS INBOX (MESSAGES 2 UNSEEN 1)"));

        // Test that without RETURN, no status is sent
        let ret = list(imap_socket, MbxSelect::All, ListReturn::None)?;
        assert!(!ret.contains("* STATUS"));

        Ok(())
    })
    .expect("test fully run");
}
