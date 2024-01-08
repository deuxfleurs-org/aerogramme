from imaplib import IMAP4_SSL, IMAP4
from os import listdir
from os.path import isfile, join
import sys
import argparse
import mailbox

parser = argparse.ArgumentParser(
        prog='mbox-to-imap',
        description='Send an mbox to an imap server',
        epilog='Just a debug tool')
parser.add_argument('mbox_path')           # positional argument
parser.add_argument('-H', '--host', default="localhost")
parser.add_argument('-p', '--port', default="143")
parser.add_argument('-u', '--user')
parser.add_argument('-s', '--password')
parser.add_argument('-m', '--mailbox', default="INBOX")
parser.add_argument('-t', '--tls', action='store_true')
args = parser.parse_args()

mbox = mailbox.mbox(args.mbox_path)

if args.tls:
    imap = IMAP4_SSL
else:
    imap = IMAP4


print(args)
with imap(host=args.host, port=args.port) as M:
    print(M.login(args.user, args.password))
    print(M.select(args.mailbox))
    for k in mbox.keys():
        content = mbox.get(k).as_bytes()
        M.append(args.mailbox, [], None, content)
        print(f"{k}/{len(mbox)}")



