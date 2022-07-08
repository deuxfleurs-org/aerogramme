#!/bin/sh

maddy -config /data/maddy.conf run &

sleep 2
maddyctl creds create --password pass test@example.com
maddyctl imap-acct create test@example.com

wait
