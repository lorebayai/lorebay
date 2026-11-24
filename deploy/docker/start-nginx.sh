#!/bin/sh
set -eu
cat > /etc/nginx/conf.d/default.conf <<'CONFIG'
server {
    listen 443 ssl default_server;
    listen [::]:443 ssl default_server;
    ssl_reject_handshake on;
}
CONFIG
sed -e "s/@DOMAIN@/$DOMAIN/g" \
    -e 's|@TLS@|/etc/nginx/lorebay-tls|g' \
    -e 's/@UPSTREAM@/lorebayd:8080/g' \
    -e 's/@UPLOAD_LIMIT@/5000000000/g' \
    /etc/lorebay/nginx.conf.in >> /etc/nginx/conf.d/default.conf
exec nginx -g 'daemon off;'
