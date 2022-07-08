#!/usr/bin/env python3
import sys

buf = ""
with open(sys.argv[1], 'r+b') as f:
    buf = f.read()

if buf.find(b'\r\n'):
    print(f"{sys.argv[1]} is already a CRLF file")
    sys.exit(1)

buf = buf.replace(b'\n', b'\r\n')

with open(sys.argv[1], 'w+b') as f:
    f.write(buf)
