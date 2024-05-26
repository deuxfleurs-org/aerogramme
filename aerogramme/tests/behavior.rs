use anyhow::Context;

mod common;
use crate::common::constants::*;
use crate::common::fragments::*;

fn main() {
    // IMAP
    /*rfc3501_imap4rev1_base();
    rfc6851_imapext_move();
    rfc4551_imapext_condstore();
    rfc2177_imapext_idle();
    rfc5161_imapext_enable();
    rfc3691_imapext_unselect();
    rfc7888_imapext_literal();
    rfc4315_imapext_uidplus();
    rfc5819_imapext_liststatus();*/

    // WebDAV
    rfc4918_webdav_core();
    rfc5397_webdav_principal();
    rfc4791_webdav_caldav();
    println!("✅ SUCCESS 🌟🚀🥳🙏🥹");
}

fn rfc3501_imap4rev1_base() {
    println!("🧪 rfc3501_imap4rev1_base");
    common::aerogramme_provider_daemon_dev(|imap_socket, lmtp_socket, _dav_socket| {
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
    common::aerogramme_provider_daemon_dev(|imap_socket, lmtp_socket, _dav_socket| {
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
    common::aerogramme_provider_daemon_dev(|imap_socket, _lmtp_socket, _dav_socket| {
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
    common::aerogramme_provider_daemon_dev(|imap_socket, lmtp_socket, _dav_socket| {
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
    common::aerogramme_provider_daemon_dev(|imap_socket, _lmtp_socket, _dav_socket| {
        connect(imap_socket).context("server says hello")?;

        capability(imap_socket, Extension::LiteralPlus).context("check server capabilities")?;
        login_with_literal(imap_socket, Account::Alice).context("use literal to connect Alice")?;

        Ok(())
    })
    .expect("test fully run");
}

fn rfc4551_imapext_condstore() {
    println!("🧪 rfc4551_imapext_condstore");
    common::aerogramme_provider_daemon_dev(|imap_socket, lmtp_socket, _dav_socket| {
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
        assert!(fetch_res.contains("* 1 FETCH (RFC822.SIZE 81 MODSEQ (3))"));
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
    common::aerogramme_provider_daemon_dev(|imap_socket, lmtp_socket, _dav_socket| {
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
    common::aerogramme_provider_daemon_dev(|imap_socket, lmtp_socket, _dav_socket| {
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
    common::aerogramme_provider_daemon_dev(|imap_socket, lmtp_socket, _dav_socket| {
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

use aero_dav::acltypes as acl;
use aero_dav::caltypes as cal;
use aero_dav::realization::{self, All};
use aero_dav::types as dav;

use crate::common::dav_deserialize;

fn rfc4918_webdav_core() {
    println!("🧪 rfc4918_webdav_core");
    common::aerogramme_provider_daemon_dev(|_imap, _lmtp, http| {
        // --- PROPFIND ---
        // empty request body (assume "allprop")
        let body = http.request(reqwest::Method::from_bytes(b"PROPFIND")?, "http://localhost:8087").send()?.text()?;
        let multistatus = dav_deserialize::<dav::Multistatus<All>>(&body);
        let root_propstats = multistatus.responses.iter()
            .find_map(|v| match &v.status_or_propstat {
                dav::StatusOrPropstat::PropStat(dav::Href(p), x) if p.as_str() == "/" => Some(x),
                _ => None,
            })
            .expect("propstats for root must exist"); 

        let root_success = root_propstats.iter().find(|p| p.status.0.as_u16() == 200).expect("some propstats for root must be 200");
        let display_name = root_success.prop.0.iter()
            .find_map(|v| match v { dav::AnyProperty::Value(dav::Property::DisplayName(x)) => Some(x), _ => None } )
            .expect("root has a display name");
        let content_type = root_success.prop.0.iter()
            .find_map(|v| match v { dav::AnyProperty::Value(dav::Property::GetContentType(x)) => Some(x), _ => None } )
            .expect("root has a content type");
        let resource_type = root_success.prop.0.iter()
            .find_map(|v| match v { dav::AnyProperty::Value(dav::Property::ResourceType(x)) => Some(x), _ => None } )
            .expect("root has a resource type");

        assert_eq!(display_name, "DAV Root");
        assert_eq!(content_type, "httpd/unix-directory");
        assert_eq!(resource_type, &[ dav::ResourceType::Collection ]);

        // propname
        let propfind_req = r#"<?xml version="1.0" encoding="utf-8" ?><propfind xmlns="DAV:"><propname/></propfind>"#;
        let body = http.request(reqwest::Method::from_bytes(b"PROPFIND")?, "http://localhost:8087").body(propfind_req).send()?.text()?;
        let multistatus = dav_deserialize::<dav::Multistatus<All>>(&body);
        let root_propstats = multistatus.responses.iter()
            .find_map(|v| match &v.status_or_propstat {
                dav::StatusOrPropstat::PropStat(dav::Href(p), x) if p.as_str() == "/" => Some(x),
                _ => None,
            })
            .expect("propstats for root must exist"); 
        let root_success = root_propstats.iter().find(|p| p.status.0.as_u16() == 200).expect("some propstats for root must be 200");
        assert!(root_success.prop.0.iter().find(|p| matches!(p, dav::AnyProperty::Request(dav::PropertyRequest::DisplayName))).is_some());
        assert!(root_success.prop.0.iter().find(|p| matches!(p, dav::AnyProperty::Request(dav::PropertyRequest::ResourceType))).is_some());
        assert!(root_success.prop.0.iter().find(|p| matches!(p, dav::AnyProperty::Request(dav::PropertyRequest::GetContentType))).is_some());

        // list of properties
        let propfind_req = r#"<?xml version="1.0" encoding="utf-8" ?><propfind xmlns="DAV:"><prop><displayname/><getcontentlength/></prop></propfind>"#;
        let body = http.request(reqwest::Method::from_bytes(b"PROPFIND")?, "http://localhost:8087").body(propfind_req).send()?.text()?;
        let multistatus = dav_deserialize::<dav::Multistatus<All>>(&body);
        let root_propstats = multistatus.responses.iter()
            .find_map(|v| match &v.status_or_propstat {
                dav::StatusOrPropstat::PropStat(dav::Href(p), x) if p.as_str() == "/" => Some(x),
                _ => None,
            })
            .expect("propstats for root must exist"); 

        let root_success = root_propstats.iter().find(|p| p.status.0.as_u16() == 200).expect("some propstats for root must be 200");
        let root_not_found = root_propstats.iter().find(|p| p.status.0.as_u16() == 404).expect("some propstats for root must be not found");

        assert!(root_success.prop.0.iter().find(|p| matches!(p, dav::AnyProperty::Value(dav::Property::DisplayName(x)) if x == "DAV Root")).is_some());
        assert!(root_success.prop.0.iter().find(|p| matches!(p, dav::AnyProperty::Value(dav::Property::ResourceType(_)))).is_none());
        assert!(root_success.prop.0.iter().find(|p| matches!(p, dav::AnyProperty::Value(dav::Property::GetContentType(_)))).is_none());
        assert!(root_not_found.prop.0.iter().find(|p| matches!(p, dav::AnyProperty::Request(dav::PropertyRequest::GetContentLength))).is_some());

        // depth 1 / -> /alice/
        let body = http.request(reqwest::Method::from_bytes(b"PROPFIND")?, "http://localhost:8087").header("Depth", "1").send()?.text()?;
        let multistatus = dav_deserialize::<dav::Multistatus<All>>(&body);
        let _user_propstats = multistatus.responses.iter()
            .find_map(|v| match &v.status_or_propstat {
                dav::StatusOrPropstat::PropStat(dav::Href(p), x) if p.as_str() == "/alice/" => Some(x),
                _ => None,
            })
            .expect("user collection must exist"); 

        // depth 1 /alice/ -> /alice/calendar/
        let body = http.request(reqwest::Method::from_bytes(b"PROPFIND")?, "http://localhost:8087/alice/").header("Depth", "1").send()?.text()?;
        let multistatus = dav_deserialize::<dav::Multistatus<All>>(&body);
        let _user_calendars_propstats = multistatus.responses.iter()
            .find_map(|v| match &v.status_or_propstat {
                dav::StatusOrPropstat::PropStat(dav::Href(p), x) if p.as_str() == "/alice/calendar/" => Some(x),
                _ => None,
            })
            .expect("user collection must exist");

        // depth 1 /alice/calendar/ -> /alice/calendar/Personal/
        let body = http.request(reqwest::Method::from_bytes(b"PROPFIND")?, "http://localhost:8087/alice/calendar/").header("Depth", "1").send()?.text()?;
        let multistatus = dav_deserialize::<dav::Multistatus<All>>(&body);
        let _user_calendars_propstats = multistatus.responses.iter()
            .find_map(|v| match &v.status_or_propstat {
                dav::StatusOrPropstat::PropStat(dav::Href(p), x) if p.as_str() == "/alice/calendar/Personal/" => Some(x),
                _ => None,
            })
            .expect("Personal calendar must exist");

        // depth 1 /alice/calendar/Personal/ -> empty for now...
        let body = http.request(reqwest::Method::from_bytes(b"PROPFIND")?, "http://localhost:8087/alice/calendar/Personal/").header("Depth", "1").send()?.text()?;
        let multistatus = dav_deserialize::<dav::Multistatus<All>>(&body);
        assert_eq!(multistatus.responses.len(), 1);

        // --- PUT ---
        // first object
        let resp = http.put("http://localhost:8087/alice/calendar/Personal/rfc2.ics").header("If-None-Match", "*").body(ICAL_RFC2).send()?;
        let obj1_etag = resp.headers().get("etag").expect("etag must be set");
        assert_eq!(resp.status(), 201);

        let body = http.request(reqwest::Method::from_bytes(b"PROPFIND")?, "http://localhost:8087/alice/calendar/Personal/").header("Depth", "1").send()?.text()?;
        let multistatus = dav_deserialize::<dav::Multistatus<All>>(&body);
        assert_eq!(multistatus.responses.len(), 2);

        // second object
        let resp = http.put("http://localhost:8087/alice/calendar/Personal/rfc3.ics").header("If-None-Match", "*").body(ICAL_RFC3).send()?;
        assert_eq!(resp.status(), 201);

        let body = http.request(reqwest::Method::from_bytes(b"PROPFIND")?, "http://localhost:8087/alice/calendar/Personal/").header("Depth", "1").send()?.text()?;
        let multistatus = dav_deserialize::<dav::Multistatus<All>>(&body);
        assert_eq!(multistatus.responses.len(), 3);

        // can't create an event on an existing path
        let resp = http.put("http://localhost:8087/alice/calendar/Personal/rfc2.ics").header("If-None-Match", "*").body(ICAL_RFC1).send()?;
        assert_eq!(resp.status(), 412);

        // update first object by knowing its ETag
        let resp = http.put("http://localhost:8087/alice/calendar/Personal/rfc2.ics").header("If-Match", obj1_etag).body(ICAL_RFC1).send()?;
        assert_eq!(resp.status(), 201);

        // --- GET ---
        let body = http.get("http://localhost:8087/alice/calendar/Personal/rfc2.ics").send()?.text()?;
        assert_eq!(body.as_bytes(), ICAL_RFC1);

        let body = http.get("http://localhost:8087/alice/calendar/Personal/rfc3.ics").send()?.text()?;
        assert_eq!(body.as_bytes(), ICAL_RFC3);

        // --- DELETE ---
        // delete 1st object
        let resp = http.delete("http://localhost:8087/alice/calendar/Personal/rfc2.ics").send()?;
        assert_eq!(resp.status(), 204);

        let body = http.request(reqwest::Method::from_bytes(b"PROPFIND")?, "http://localhost:8087/alice/calendar/Personal/").header("Depth", "1").send()?.text()?;
        let multistatus = dav_deserialize::<dav::Multistatus<All>>(&body);
        assert_eq!(multistatus.responses.len(), 2);

        // delete 2nd object
        let resp = http.delete("http://localhost:8087/alice/calendar/Personal/rfc3.ics").send()?;
        assert_eq!(resp.status(), 204);

        let body = http.request(reqwest::Method::from_bytes(b"PROPFIND")?, "http://localhost:8087/alice/calendar/Personal/").header("Depth", "1").send()?.text()?;
        let multistatus = dav_deserialize::<dav::Multistatus<All>>(&body);
        assert_eq!(multistatus.responses.len(), 1);

        Ok(())
    })
    .expect("test fully run");
}

fn rfc5397_webdav_principal() {
    println!("🧪 rfc5397_webdav_principal");
    common::aerogramme_provider_daemon_dev(|_imap, _lmtp, http| {
        // Find principal
        let propfind_req = r#"<?xml version="1.0" encoding="utf-8" ?><propfind xmlns="DAV:"><prop><current-user-principal/></prop></propfind>"#;
        let body = http.request(reqwest::Method::from_bytes(b"PROPFIND")?, "http://localhost:8087").body(propfind_req).send()?.text()?;
        let multistatus = dav_deserialize::<dav::Multistatus<All>>(&body);
        let root_propstats = multistatus.responses.iter()
            .find_map(|v| match &v.status_or_propstat {
                dav::StatusOrPropstat::PropStat(dav::Href(p), x) if p.as_str() == "/" => Some(x),
                _ => None,
            })
            .expect("propstats for root must exist");

        let root_success = root_propstats.iter().find(|p| p.status.0.as_u16() == 200).expect("current-user-principal must exist");
        let principal = root_success.prop.0.iter()
            .find_map(|v| match v {
                dav::AnyProperty::Value(dav::Property::Extension(realization::Property::Acl(acl::Property::CurrentUserPrincipal(acl::User::Authenticated(dav::Href(x)))))) => Some(x),
                _ => None,
            })
            .expect("request returned an authenticated principal");
        assert_eq!(principal, "/alice/");

        Ok(())
    })
    .expect("test fully run")
}

fn rfc4791_webdav_caldav() {
    println!("🧪 rfc4791_webdav_caldav");
    common::aerogramme_provider_daemon_dev(|_imap, _lmtp, http| {
        // --- INITIAL TEST SETUP ---
        // Add entries (3 VEVENT, 1 FREEBUSY, 1 VTODO)
        let resp = http
            .put("http://localhost:8087/alice/calendar/Personal/rfc1.ics")
            .header("If-None-Match", "*")
            .body(ICAL_RFC1)
            .send()?;
        let obj1_etag = resp.headers().get("etag").expect("etag must be set");
        assert_eq!(resp.status(), 201);
        let resp = http
            .put("http://localhost:8087/alice/calendar/Personal/rfc2.ics")
            .header("If-None-Match", "*")
            .body(ICAL_RFC2)
            .send()?;
        let obj2_etag = resp.headers().get("etag").expect("etag must be set");
        assert_eq!(resp.status(), 201);
        let resp = http
            .put("http://localhost:8087/alice/calendar/Personal/rfc3.ics")
            .header("If-None-Match", "*")
            .body(ICAL_RFC3)
            .send()?;
        let obj3_etag = resp.headers().get("etag").expect("etag must be set");
        assert_eq!(resp.status(), 201);
        let resp = http
            .put("http://localhost:8087/alice/calendar/Personal/rfc4.ics")
            .header("If-None-Match", "*")
            .body(ICAL_RFC4)
            .send()?;
        let _obj4_etag = resp.headers().get("etag").expect("etag must be set");
        assert_eq!(resp.status(), 201);
        let resp = http
            .put("http://localhost:8087/alice/calendar/Personal/rfc5.ics")
            .header("If-None-Match", "*")
            .body(ICAL_RFC5)
            .send()?;
        let _obj5_etag = resp.headers().get("etag").expect("etag must be set");
        assert_eq!(resp.status(), 201);
        let resp = http
            .put("http://localhost:8087/alice/calendar/Personal/rfc6.ics")
            .header("If-None-Match", "*")
            .body(ICAL_RFC6)
            .send()?;
        let _obj6_etag = resp.headers().get("etag").expect("etag must be set");
        assert_eq!(resp.status(), 201);

        // A generic function to check a <calendar-data/> query result
        let check_cal =
            |multistatus: &dav::Multistatus<All>,
             (ref_path, ref_etag, ref_ical): (&str, Option<&str>, Option<&[u8]>)| {
                let obj_stats = multistatus
                    .responses
                    .iter()
                    .find_map(|v| match &v.status_or_propstat {
                        dav::StatusOrPropstat::PropStat(dav::Href(p), x)
                            if p.as_str() == ref_path =>
                        {
                            Some(x)
                        }
                        _ => None,
                    })
                    .expect("propstats must exist");
                let obj_success = obj_stats
                    .iter()
                    .find(|p| p.status.0.as_u16() == 200)
                    .expect("some propstats must be 200");
                let etag = obj_success.prop.0.iter().find_map(|p| match p {
                    dav::AnyProperty::Value(dav::Property::GetEtag(x)) => Some(x.as_str()),
                    _ => None,
                });
                assert_eq!(etag, ref_etag);
                let calendar_data = obj_success.prop.0.iter().find_map(|p| match p {
                    dav::AnyProperty::Value(dav::Property::Extension(
                        realization::Property::Cal(cal::Property::CalendarData(x)),
                    )) => Some(x.payload.as_bytes()),
                    _ => None,
                });
                assert_eq!(calendar_data, ref_ical);
            };

        // --- AUTODISCOVERY ---
        // Check calendar discovery from principal
        let propfind_req = r#"<?xml version="1.0" encoding="utf-8" ?>
        <D:propfind xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
            <D:prop><C:calendar-home-set/></D:prop>
        </D:propfind>"#;

        let body = http
            .request(
                reqwest::Method::from_bytes(b"PROPFIND")?,
                "http://localhost:8087/alice/",
            )
            .body(propfind_req)
            .send()?
            .text()?;
        let multistatus = dav_deserialize::<dav::Multistatus<All>>(&body);
        let principal_propstats = multistatus
            .responses
            .iter()
            .find_map(|v| match &v.status_or_propstat {
                dav::StatusOrPropstat::PropStat(dav::Href(p), x) if p.as_str() == "/alice/" => {
                    Some(x)
                }
                _ => None,
            })
            .expect("propstats for root must exist");
        let principal_success = principal_propstats
            .iter()
            .find(|p| p.status.0.as_u16() == 200)
            .expect("current-user-principal must exist");
        let calendar_home_set = principal_success
            .prop
            .0
            .iter()
            .find_map(|v| match v {
                dav::AnyProperty::Value(dav::Property::Extension(realization::Property::Cal(
                    cal::Property::CalendarHomeSet(dav::Href(x)),
                ))) => Some(x),
                _ => None,
            })
            .expect("request returns a calendar home set");
        assert_eq!(calendar_home_set, "/alice/calendar/");

        // Check calendar access support
        let _resp = http
            .request(
                reqwest::Method::from_bytes(b"OPTIONS")?,
                "http://localhost:8087/alice/calendar/",
            )
            .send()?;
        //@FIXME not yet supported. returns DAV: 1 ; expects DAV: 1 calendar-access
        // Not used by any client I know, so not implementing it now.

        // --- REPORT calendar-query ---
        //@FIXME missing support for calendar-data...
        // 7.8.8.  Example: Retrieval of Events Only
        let cal_query = r#"<?xml version="1.0" encoding="utf-8" ?>
            <C:calendar-query xmlns:C="urn:ietf:params:xml:ns:caldav">
                <D:prop xmlns:D="DAV:">
                    <D:getetag/>
                    <C:calendar-data/>
                </D:prop>
                <C:filter>
                    <C:comp-filter name="VCALENDAR">
                        <C:comp-filter name="VEVENT"/>
                    </C:comp-filter>
                </C:filter>
            </C:calendar-query>"#;
        let resp = http
            .request(
                reqwest::Method::from_bytes(b"REPORT")?,
                "http://localhost:8087/alice/calendar/Personal/",
            )
            .body(cal_query)
            .send()?;
        assert_eq!(resp.status(), 207);
        let multistatus = dav_deserialize::<dav::Multistatus<All>>(&resp.text()?);
        assert_eq!(multistatus.responses.len(), 3);

        [
            ("/alice/calendar/Personal/rfc1.ics", obj1_etag, ICAL_RFC1),
            ("/alice/calendar/Personal/rfc2.ics", obj2_etag, ICAL_RFC2),
            ("/alice/calendar/Personal/rfc3.ics", obj3_etag, ICAL_RFC3),
        ]
        .iter()
        .for_each(|(ref_path, ref_etag, ref_ical)| {
            check_cal(
                &multistatus,
                (
                    ref_path,
                    Some(ref_etag.to_str().expect("etag header convertible to str")),
                    Some(ref_ical),
                ),
            )
        });

        // 8.2.1.2.  Synchronize by Time Range (here: July 2006)
        let cal_query = r#"<?xml version="1.0" encoding="utf-8" ?>
            <C:calendar-query xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
                <D:prop>
                    <D:getetag/>
                </D:prop>
                <C:filter>
                    <C:comp-filter name="VCALENDAR">
                        <C:comp-filter name="VEVENT">
                            <C:time-range start="20060701T000000Z" end="20060801T000000Z"/>
                        </C:comp-filter>
                    </C:comp-filter>
                </C:filter>
            </C:calendar-query>"#;
        let resp = http
            .request(
                reqwest::Method::from_bytes(b"REPORT")?,
                "http://localhost:8087/alice/calendar/Personal/",
            )
            .body(cal_query)
            .send()?;
        assert_eq!(resp.status(), 207);
        let multistatus = dav_deserialize::<dav::Multistatus<All>>(&resp.text()?);
        assert_eq!(multistatus.responses.len(), 1);
        check_cal(
            &multistatus,
            (
                "/alice/calendar/Personal/rfc2.ics",
                Some(obj2_etag.to_str().expect("etag header convertible to str")),
                None,
            ),
        );

        // 7.8.5.  Example: Retrieval of To-Dos by Alarm Time Range
        let cal_query = r#"<?xml version="1.0" encoding="utf-8" ?>
            <C:calendar-query xmlns:C="urn:ietf:params:xml:ns:caldav">
                <D:prop xmlns:D="DAV:">
                    <D:getetag/>
                    <C:calendar-data/>
                </D:prop>
                <C:filter>
                    <C:comp-filter name="VCALENDAR">
                        <C:comp-filter name="VTODO">
                            <C:comp-filter name="VALARM">
                                <C:time-range start="20060201T000000Z" end="20060301T000000Z"/>
                            </C:comp-filter>
                        </C:comp-filter>
                    </C:comp-filter>
                </C:filter>
            </C:calendar-query>"#;
        let resp = http
            .request(
                reqwest::Method::from_bytes(b"REPORT")?,
                "http://localhost:8087/alice/calendar/Personal/",
            )
            .body(cal_query)
            .send()?;
        assert_eq!(resp.status(), 207);
        let multistatus = dav_deserialize::<dav::Multistatus<All>>(&resp.text()?);
        assert_eq!(multistatus.responses.len(), 1);

        // 7.8.6.  Example: Retrieval of Event by UID
        // @TODO

        // 7.8.7.  Example: Retrieval of Events by PARTSTAT
        // @TODO

        // 7.8.9.  Example: Retrieval of All Pending To-Dos
        // @TODO

        // --- REPORT calendar-multiget ---
        let cal_query = r#"<?xml version="1.0" encoding="utf-8" ?>
            <C:calendar-multiget xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
                <D:prop>
                    <D:getetag/>
                    <C:calendar-data/>
                </D:prop>
                <D:href>/alice/calendar/Personal/rfc1.ics</D:href>
                <D:href>/alice/calendar/Personal/rfc3.ics</D:href>
            </C:calendar-multiget>"#;
        let resp = http
            .request(
                reqwest::Method::from_bytes(b"REPORT")?,
                "http://localhost:8087/alice/calendar/Personal/",
            )
            .body(cal_query)
            .send()?;
        assert_eq!(resp.status(), 207);
        let multistatus = dav_deserialize::<dav::Multistatus<All>>(&resp.text()?);
        assert_eq!(multistatus.responses.len(), 2);
        [
            ("/alice/calendar/Personal/rfc1.ics", obj1_etag, ICAL_RFC1),
            ("/alice/calendar/Personal/rfc3.ics", obj3_etag, ICAL_RFC3),
        ]
        .iter()
        .for_each(|(ref_path, ref_etag, ref_ical)| {
            check_cal(
                &multistatus,
                (
                    ref_path,
                    Some(ref_etag.to_str().expect("etag header convertible to str")),
                    Some(ref_ical),
                ),
            )
        });

        Ok(())
    })
    .expect("test fully run")
}

// @TODO SYNC
