//use mail_parser_superboum::Message; // FAIL

use mail_parser::Message; // PASS

//use mail_parser_05::Message;				// PASS
//use mail_parser_main::Message;			// PASS
//use mail_parser_db61a03::Message;			// PASS

#[test]
fn test1() {
    let input = br#"Content-Type: multipart/mixed; boundary="1234567890123456789012345678901234567890123456789012345678901234567890123456789012"

--1234567890123456789012345678901234567890123456789012345678901234567890123456789012
Content-Type: multipart/mixed; boundary="123456789012345678901234567890123456789012345678901234567890123456789012345678901"

--123456789012345678901234567890123456789012345678901234567890123456789012345678901
Content-Type: multipart/mixed; boundary="12345678901234567890123456789012345678901234567890123456789012345678901234567890"

--12345678901234567890123456789012345678901234567890123456789012345678901234567890
Content-Type: text/plain

1
--1234567890123456789012345678901234567890123456789012345678901234567890123456789012
Content-Type: text/plain

22
--123456789012345678901234567890123456789012345678901234567890123456789012345678901
Content-Type: text/plain

333
--12345678901234567890123456789012345678901234567890123456789012345678901234567890
Content-Type: text/plain

4444
"#;

    let message = Message::parse(input);
    dbg!(message);
}

#[test]
fn test2() {
    let input = br#"Content-Type: message/rfc822

Content-Type: message/rfc822

Content-Type: text/plain

1"#;

    let message = Message::parse(input);
    dbg!(message);
}

#[test]
fn test3() {
    let input = br#"Content-Type: multipart/mixed; boundary=":foo"

--:foo
--:foo
Content-Type: text/plain
--:foo
Content-Type: text/plain
--:foo
Content-Type: text/html
--:foo--


"#;

    let message = Message::parse(input);
    dbg!(message);
}
