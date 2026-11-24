FROM gcr.io/distroless/static-debian13:nonroot@sha256:8ce6dd87e9823c5b48dcfb2c8604714ccb351ce59607327bcc4957448da279b5
COPY lorebayd /usr/local/bin/lorebayd
COPY LICENSE THIRD-PARTY-NOTICES.txt /usr/share/doc/lorebayd/
USER 65532:65532
WORKDIR /data
ENV LOREBAYD__DATA_DIRECTORY=/data LOREBAYD__USERS_FILE=/etc/lorebay/users.yaml LOREBAYD__LISTEN_ADDRESS=0.0.0.0:8080
ENTRYPOINT ["/usr/local/bin/lorebayd"]
CMD ["--config", "/etc/lorebay/lorebayd.json"]
HEALTHCHECK --interval=5s --timeout=4s --start-period=10s --retries=6 CMD ["/usr/local/bin/lorebayd", "readiness", "--config", "/etc/lorebay/lorebayd.json"]
