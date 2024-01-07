FROM debian:sid

ARG DEBIAN_FRONTEND=noninteractive
RUN apt update && \
    apt install -y cyrus-imapd cyrus-pop3d cyrus-nntpd cyrus-caldav cyrus-admin sasl2-bin && \
    echo "admins: cyrus" >> /etc/imapd.conf && \
    touch /var/lib/cyrus/tls_sessions.db && \
    chown cyrus:mail /var/lib/cyrus/tls_sessions.db && \
    mkdir -p /run/cyrus && \
    chown -R cyrus:mail /run/cyrus

COPY entrypoint.sh /usr/local/bin/entrypoint.sh

CMD [ "/usr/local/bin/entrypoint.sh" ]

