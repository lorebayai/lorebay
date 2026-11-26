#!/bin/sh
set -eu
cat > /etc/nginx/conf.d/default.conf <<'CONFIG'
upstream lorebay_backend {
    zone lorebay_backend 64k;
    resolver 127.0.0.11 valid=5s ipv6=off;
    server lorebayd:8080 resolve;
}

server {
    listen 443 ssl default_server;
    listen [::]:443 ssl default_server;
    ssl_reject_handshake on;
}
CONFIG
sed -e "s/@DOMAIN@/$DOMAIN/g" \
    -e 's|@TLS@|/etc/nginx/lorebay-tls|g' \
    -e 's/@UPSTREAM@/lorebay_backend/g' \
    -e 's/@UPLOAD_LIMIT@/5000000000/g' \
    -e 's|@ACCESS_LOG@|access_log /var/log/nginx/access.log lorebay_redacted if=$lorebay_access_log;|g' \
    -e 's/@CONNECTION_LIMIT@//g' \
    /etc/lorebay/nginx.conf.in >> /etc/nginx/conf.d/default.conf
exec nginx -g 'daemon off;'
