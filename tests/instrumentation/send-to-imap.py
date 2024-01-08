from imaplib import IMAP4_SSL, IMAP4
from os import listdir
from os.path import isfile, join
import sys

# COMMAND USAGE
#
# start a test IMAP servers:
#   docker-compose.up
# then call this script. eg:
#   ./send-to-imap.py all ./emails/dxflrs/


def rebuild_body_res(b):
    bb = b''
    for e in b:
        if type(e) is tuple:
            bb += b'\r\n'.join([p for p in e])
        else:
            bb += e

    f = bb[bb.find(b'('):]
    return f

mode = sys.argv[1]
path = sys.argv[2]

base_test_mb = "kzUXL7HyS5OjLcU8"
parameters = {
  "dovecot": {
    "con": IMAP4_SSL,
    "port": 993,
    "user": "test",
    "pw": "pass",
    "ext": ".dovecot",
    "mb": base_test_mb,
  },
  "maddy": {
    "con": IMAP4_SSL,
    "port": 994,
    "user": "test@example.com",
    "pw": "pass",
    "ext": ".maddy",
    "mb": base_test_mb,
  },
  "cyrus": {
    "con": IMAP4,
    "port": 143,
    "user": "test",
    "pw": "pass",
    "ext": ".cyrus",
    "mb": "INBOX."+base_test_mb,
  },
  "courier": {
    "con": IMAP4,
    "port": 144,
    "user": "debian",
    "pw": "debian",
    "ext": ".courier",
    "mb": base_test_mb,
  },
  "stalwart": {
    "con": IMAP4_SSL,
    "port": 1993,
    "user": "test@example.com",
    "pw": "pass",
    "ext": ".stalwart.0.2.0",
    "mb": base_test_mb,
  }
}

queue = list(parameters.keys())
if mode in parameters:
    queue = [ mode ]

onlyfiles = [join(path, f) for f in listdir(path) if isfile(join(path, f)) and len(f) > 4 and f[-4:] == ".eml"]

for target in queue:
    print(f"--- {target} ---")
    conf = parameters[target]
    test_mb = conf['mb']

    with conf['con'](host="localhost", port=conf['port']) as M:
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
