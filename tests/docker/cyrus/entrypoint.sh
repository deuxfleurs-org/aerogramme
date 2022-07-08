#!/bin/bash

cyrmaster -D -l 32 -C /etc/imapd.conf -M /etc/cyrus.conf &
sleep 2

echo cyrus | saslpasswd2 -p cyrus 
echo pass | saslpasswd2 -p test

cyradm -u cyrus -w cyrus 127.0.0.1 <<EOF
cm user.test
setaclmailbox user.test test kxtelrswip
exit
EOF

wait
