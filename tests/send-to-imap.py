from imaplib import IMAP4_SSL
from os import listdir
from os.path import isfile, join
import sys

def rebuild_body_res(b):
    bb = b''
    for e in b:
        if type(e) is tuple:
            bb += b'\r\n'.join([p for p in e])
        else:
            bb += e

    f = bb[bb.find(b'('):]
    return f

path = sys.argv[1]
onlyfiles = [join(path, f) for f in listdir(path) if isfile(join(path, f)) and len(f) > 4 and f[-4:] == ".eml"]

# docker run --rm -it -p 993:993 -p 143:143 dovecot/dovecot
test_mb = "kzUXL7HyS5OjLcU8"
with IMAP4_SSL(host="localhost") as M:
    M.login("test", "pass")
    M.delete(test_mb)
    M.create(test_mb)
    M.select(test_mb)
    for (idx, f) in enumerate(onlyfiles):
        f_noext = f[:-4]
        with open(f, 'r+b') as mail:
            M.append(test_mb, [], None, mail.read())
            seq = (f"{idx+1}:{idx+1}").encode()
            (r, b) = M.fetch(seq, "(BODY)")
            assert r == 'OK'
            

            with open(f_noext + ".body", 'w+b') as w:
                w.write(rebuild_body_res(b))

            (r, b) = M.fetch(seq, "(BODYSTRUCTURE)")
            assert r == 'OK'
            with open(f_noext + ".bodystructure", 'w+b') as w:
                w.write(rebuild_body_res(b))

    M.close()
    M.logout()

# old :
    #(res, v) = M.select(test_mb)
    #assert res == 'OK'
    #exists = v[0]
    #print(M.fetch(b"1:"+exists, ))
    #print(M.fetch(b"1:"+exists, "(BODYSTRUCTURE)"))

