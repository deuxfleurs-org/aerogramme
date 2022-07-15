#!/bin/sh

cd $(dirname $0)

function mail_lmtp_session (
	echo -e "LHLO localhost\r"
	for mail in $(find emails -name '*.eml'); do
		echo -e "MAIL FROM: <alex@adnab.me>\r"
		echo -e "RCPT TO: <lx@staging.deuxfleurs.org>\r"
		echo -e "DATA\r"
		cat $mail
		echo -e "\r"
		echo -e ".\r"
	done
	echo -e "QUIT\r"
)

mail_lmtp_session | tee >(nc localhost 12024)
