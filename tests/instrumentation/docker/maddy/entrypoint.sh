#!/bin/sh

maddy -config /data/maddy.conf run &

sleep 2
maddy creds create --password pass test@example.com
maddy imap-acct create test@example.com

wait
