# Spawn Dovecot+Maddy+Cyrus

Run:

```
docker-compose up
```

  - Dovecot
    - listen on :993, run `openssl s_client -connect 127.0.0.1:993`
    - login with `A LOGIN test pass`
  - Maddy
    - listen on :994,  run `openssl s_client -connect 127.0.0.1:993`
    - login with `A LOGIN test@example.com pass`
  - Cyrus
    - lient on :143, run `nc 127.0.0.1 143`
    - login with `A LOGIN test pass`

Other IMAP servers we could add:
  - WildDuck (own node.js imap implementation)
    - https://github.com/nodemailer/wildduck
  - DBMail (own C IMAP implementation)
    - https://github.com/dbmail/dbmail/tree/master
  - UW IMAP (known to be the reference IMAP implementation)
    - https://wiki.archlinux.org/title/UW_IMAP
  - Apache James (has its own implementation of IMAP too)
    - https://james.apache.org/

# Inject emails and dump the computed `BODY` + `BODYSTRUCTURE`

Once you ran `docker-compose up`, launch `./send-to-imap.py`
