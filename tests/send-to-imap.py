from imaplib import IMAP4_SSL, IMAP4
from os import listdir
from os.path import isfile, join
import sys

# COMMAND USAGE
#
# start a test IMAP server locally (see comment below)
# then call this script. eg:
# ./send-to-imap.py dovecot ./emails/dxflrs/

# DOCKER CONTAINERS TO QUICKLY SPAWN A REFERENCE SERVER
#
# -- Dovecot --
# cmd: docker run --rm -it -p 993:993 -p 143:143 dovecot/dovecot
# user: test (any)
# pw: pass
#
# -- Maddy --
# cmds:
#   docker volume create maddydata
#   openssl req  -nodes -new -x509  -keyout privkey.pem -out fullchain.pem
#   docker run --rm -it --name maddy -e MADDY_DOMAIN=example.com -e MADDY_HOSTNAME=mx.example.com -v maddydata:/data -p 143:143 -p 993:993 --entrypoint /bin/sh foxcpp/maddy
#       mkdir /data/tls
#   docker cp ./fullchain.pem maddy:/data/tls/
#   docker cp ./privkey.pem maddy:/data/tls/
#       maddyctl creds create test@example.com
#       maddyctl imap-acct create test@example.com
#       maddy -config /data/maddy.conf run --debug
#   
#   docker run --rm -it  -v maddydata:/data-p 143:143 -p 993:993 foxcpp/maddy
#
# -- Cyrus --
#  docker run --rm -it --name cyrus -v /dev/log:/dev/log -p 143:143 -p 993:993 debian:buster
#      apt update; apt install -y cyrus-imapd cyrus-pop3d cyrus-nntpd cyrus-caldav cyrus-admin sasl2-bin vim
#      # ( 1. No Configuration / 8. Europe / 37. Paris / yes)
#      vim  /etc/imapd.conf
#      # (uncomment 'admins: cyrus')
#      touch /var/lib/cyrus/tls_sessions.db
#      chown cyrus:mail /var/lib/cyrus/tls_sessions.db
#      mkdir /run/cyrus
#      chown -R cyrus:mail /run/cyrus 
#      cyrmaster -D -l 32 -C /etc/imapd.conf -M /etc/cyrus.conf 
#  docker exec -ti --name cyrus /bin/bash
#      saslpasswd2 cyrus
#      # (put "cyrus" as password)
#      saslpasswd2 test
#      # (put "pass" as password)
#      cyradm -u cyrus localhost
#      cm kzUXL7HyS5OjLcU8
#      setaclmailbox kzUXL7HyS5OjLcU8 test ktelrswip
#
#  -> note, must be run between 2 send-imap commands in cyradm
# localhost> setaclmailbox kzUXL7HyS5OjLcU8 cyrus x
# localhost> dm kzUXL7HyS5OjLcU8
# localhost> cm kzUXL7HyS5OjLcU8
# localhost> setaclmailbox kzUXL7HyS5OjLcU8 test ktelrswip

def rebuild_body_res(b):
    bb = b''
    for e in b:
        if type(e) is tuple:
            bb += b'\r\n'.join([p for p in e])
        else:
            bb += e

    f = bb[bb.find(b'('):]
    return f

target = sys.argv[1]
path = sys.argv[2]

parameters = {
  "dovecot": {
    "con": IMAP4_SSL,
    "user": "test",
    "pw": "pass",
    "ext": ".dovecot",
  },
  "maddy": {
    "con": IMAP4_SSL,
    "user": "test@example.com",
    "pw": "pass",
    "ext": ".maddy",
  },
  "cyrus": {
    "con": IMAP4,
    "user": "test",
    "pw": "pass",
    "ext": ".cyrus",
  }
}
conf = parameters[target]

onlyfiles = [join(path, f) for f in listdir(path) if isfile(join(path, f)) and len(f) > 4 and f[-4:] == ".eml"]

test_mb = "kzUXL7HyS5OjLcU8"
with conf['con'](host="localhost") as M:
    print(M.login(conf['user'], conf['pw']))
    print(M.delete(test_mb))
    print(M.create(test_mb))


    print(M.list())
    print(M.select(test_mb))
    failed = 0
    for (idx, f) in enumerate(onlyfiles):
        f_noext = f[:-4]
        try:
            with open(f, 'r+b') as mail:
                print(M.append(test_mb, [], None, mail.read()))
                seq = (f"{idx+1-failed}:{idx+1-failed}").encode()
                (r, b) = M.fetch(seq, "(BODY)")
                print((r, b))
                assert r == 'OK'
            

                with open(f_noext + conf['ext'] + ".body", 'w+b') as w:
                    w.write(rebuild_body_res(b))

                (r, b) = M.fetch(seq, "(BODYSTRUCTURE)")
                print((r, b))
                assert r == 'OK'
                with open(f_noext + conf['ext'] + ".bodystructure", 'w+b') as w:
                    w.write(rebuild_body_res(b))
        except:
            failed += 1
            print(f"failed {f}")

    M.close()
    M.logout()
