FROM debian:sid

RUN apt-get update
RUN apt-get install -y courier-imap
RUN useradd -m debian -p '$1$B8Mq5Hki$fg5f4SndVNWsfq.mJiqbI0'
USER debian
RUN maildirmake /home/debian/Maildir
USER root
COPY ./entrypoint.sh /entrypoint
ENTRYPOINT ["/entrypoint"]
