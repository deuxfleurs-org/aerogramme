FROM debian:buster as builder

WORKDIR /root
RUN apt-get update && apt-get install -y openssl && \
    openssl req -nodes -new -x509 -subj "/C=DW/ST=Sto/L=Ankh-Morpork /O=Unseen University/OU=Library/CN=Ook/emailAddress=ook@ook.ook" -keyout privkey.pem -out fullchain.pem

FROM foxcpp/maddy:0.6.2

COPY --from=builder /root/privkey.pem /data/tls/privkey.pem
COPY --from=builder /root/fullchain.pem /data/tls/fullchain.pem

ENV MADDY_DOMAIN=example.com MADDY_HOSTNAME=mx.example.com

COPY entrypoint.sh /entrypoint.sh

ENTRYPOINT [ "/entrypoint.sh" ]
