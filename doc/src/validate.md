# Validate

Start a server as follow:

```bash
cargo run -- server
```

Now you can use netcat to connect on the LMTP and IMAP endpoint to check that Aerogramme behaves as expected. As an example, here is an IMAP trace extracted from Aerogramme:

```
S: * OK Hello
C: A1 LOGIN lx plop
S: A1 OK Completed
C: A2 SELECT INBOX
S: * 0 EXISTS
S: * 0 RECENT
S: * FLAGS (\Seen \Answered \Flagged \Deleted \Draft)
S: * OK [PERMANENTFLAGS (\Seen \Answered \Flagged \Deleted \Draft \*)] Flags permitted
S: * OK [UIDVALIDITY 1] UIDs valid
S: * OK [UIDNEXT 1] Predict next UID
S: A2 OK [READ-WRITE] Select completed
C: A3 NOOP
S: A3 OK NOOP completed.
        <---- e-mail arrives through LMTP server ---->
C: A4 NOOP
S: * 1 EXISTS
S: A4 OK NOOP completed.
C: A5 FETCH 1 FULL
S: * 1 FETCH (UID 1 FLAGS () INTERNALDATE "06-Jul-2022 14:46:42 +0000" 
      RFC822.SIZE 117 ENVELOPE (NIL "test" (("Alan Smith" NIL "alan" "smith.me")) 
      NIL NIL (("Alan Smith" NIL "alan" "aerogramme.tld")) NIL NIL NIL NIL) 
      BODY ("TEXT" "test" NIL "test" "test" "test" 1 1))
S: A5 OK FETCH completed
C: A6 FETCH 1 (RFC822)
S: * 1 FETCH (UID 1 RFC822 {117}
S: Subject: test
S: From: Alan Smith <alan@smith.me>
S: To: Alan Smith <alan@aerogramme.tld>
S: 
S: Hello, world!
S: .
S: )
S: A6 OK FETCH completed
C: A7 LOGOUT
S: * BYE Logging out
S: A7 OK Logout completed
```
