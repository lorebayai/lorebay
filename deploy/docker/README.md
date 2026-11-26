# Docker deployment

Run one Lorebay server with Docker Compose on x86_64 Linux. nginx serves HTTPS;
`lorebayd` runs as UID/GID 65532 on the private network, without a host port.
The aarch64 image passes the readiness smoke but is not deployment-qualified.

## Usage

Install Docker with Compose, Python 3.11+, OpenSSL, curl, coreutils and util-linux. Download
and check the deployment bundle as root; optional attestations require GitHub CLI:

```bash
mkdir -p /srv/lorebay
cd /srv/lorebay
VERSION=v0.1.4
RELEASE="https://github.com/lorebayai/lorebay/releases/download/$VERSION"
curl -fLO "$RELEASE/lorebay-deploy.tar.gz"
curl -fLO "$RELEASE/SHA256SUMS"
grep '  lorebay-deploy.tar.gz$' SHA256SUMS | sha256sum -c -
gh attestation verify lorebay-deploy.tar.gz --repo lorebayai/lorebay
gh attestation verify oci://ghcr.io/lorebayai/lorebayd:${VERSION#v} --repo lorebayai/lorebay
tar -xzf lorebay-deploy.tar.gz
docker run --rm -i ghcr.io/lorebayai/lorebayd:${VERSION#v} hash-password < /srv/inputs/password.txt
cp deploy/docker/example.env /srv/lorebay.env
```

Edit that file using the table below, with users formatted as
[documented](../../lorebayd/README.md#usage) via `lorebayd hash-password` and an empty
local data path. Before first start, check ownership, readability and running writers:

```bash
set -a; . /srv/lorebay.env; set +a
test -d "$DATA_DIRECTORY"
stat -c '%u:%g %a %n' "$DATA_DIRECTORY" "$USERS_FILE"
test -r "$USERS_FILE"
pgrep -a lorebayd || true
docker ps --no-trunc --format '{{.Names}} {{.Mounts}}'
sudo chown 65532:65532 "$DATA_DIRECTORY" "$USERS_FILE"
sudo chmod 0700 "$DATA_DIRECTORY"
sudo chmod 0400 "$USERS_FILE"
sudo setpriv --reuid=65532 --regid=65532 --clear-groups test -r "$USERS_FILE"
sudo setpriv --reuid=65532 --regid=65532 --clear-groups test -w "$DATA_DIRECTORY"
```

Expected data mode/owner is `65532:65532 700`; ancestors must allow UID 65532
traversal. Grants change metadata only; the configuration must be readable by
UID 65532 with no other `lorebayd` writing this data. Create the certificate directory:

```bash
sudo install -d -m 0700 "$CERTIFICATE_DIRECTORY" "$CERTIFICATE_DIRECTORY/pair-initial"
sudo install -m 0600 /srv/inputs/certificate.pem "$CERTIFICATE_DIRECTORY/pair-initial/certificate.pem"
sudo install -m 0600 /srv/inputs/key.pem "$CERTIFICATE_DIRECTORY/pair-initial/key.pem"
sudo ln -s pair-initial "$CERTIFICATE_DIRECTORY/live"
docker compose --env-file /srv/lorebay.env -f deploy/docker/compose.yml config
docker compose --env-file /srv/lorebay.env -f deploy/docker/compose.yml up -d --wait
curl "https://$DOMAIN/readyz"
```

Per-caller rate limits require Docker to preserve client addresses; the userland
proxy, address-specific publishes (such as loopback), or IPv6 clients without Docker
IPv6 collapse callers to the bridge gateway and share one budget (check whether
access log lines start with the bridge gateway). Compose therefore sets no per-source
connection limit; nginx buffers non-upload bodies and bounds clients by its worker
connections. `up --wait` waits for both services to be healthy (`ps` reports both);
both restart unless stopped, and nginx follows `lorebayd`'s DNS address. Use the
same env/file flags for `logs`, `restart` and `down` (retains data). Only nginx serves
HTTPS; access logs omit query strings and loopback `GET /readyz`. Replace certs with:

```bash
python3 -B deploy/common/replace-certificate.py --domain "$DOMAIN" \
  --certificate /srv/inputs/new.pem --key /srv/inputs/new-key.pem \
  --directory "$CERTIFICATE_DIRECTORY" \
  --container "$(docker compose --env-file /srv/lorebay.env -f deploy/docker/compose.yml ps -q nginx)"
```

The shared script validates the staged pair and `nginx -t`, reloads, and checks
what nginx serves; failure retains or restores the previous pair. For a nonstandard
host HTTPS port, use `--connect` with nginx's reachable container address on port 443.

## Configuration

All supplied paths are absolute. Every bind refuses a missing source.

| Env key | Value |
| --- | --- |
| `DOMAIN` | Public DNS name without scheme or port; also nginx's server name |
| `DATA_DIRECTORY` | Existing private local directory, writable by UID 65532 |
| `USERS_FILE` | Existing users file, readable by UID 65532 |
| `CONFIG_FILE` | Absolute path to `deploy/docker/lorebayd.json`; defaults only |
| `CERTIFICATE_DIRECTORY` | Managed directory with `live` pointing to a certificate pair |
| `HTTPS_PORT` | Published nginx port; default `443` |
| `HTTPS_SUFFIX` | Empty for `443`; otherwise `:<HTTPS_PORT>` for the public origin |

The shipped config raises the four application budgets for shared proxy traffic; nginx
owns per-caller policy. Match its `/files/` upload limit when changing
`upload_size_limit_bytes` ([all keys](../../lorebayd/README.md#configuration)); run `moon run deploy:check`.

## Docs

Start at the [docs overview](../../docs/overview.md).
