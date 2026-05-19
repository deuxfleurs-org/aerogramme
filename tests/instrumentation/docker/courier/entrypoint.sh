#!/usr/bin/env bash


/usr/lib/courier/courier-authlib/authdaemond &
/usr/sbin/couriertcpd -address=0 -maxprocs=40 -maxperip=20 -access=/etc/courier/imapaccess.dat -nodnslookup 144 /usr/lib/courier/courier/imaplogin /usr/bin/imapd Maildir
