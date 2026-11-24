# Native deployment

Install one Lorebay server on x86_64 Debian 12 or Ubuntu 24.04 with systemd as
PID 1. nginx serves HTTPS and caller limits; `lorebayd` listens on loopback.
A native install requires a dedicated host whose local users are trusted: any
local process can reach `lorebayd` on loopback without nginx's per-caller limits.

## Usage

As root, use Python 3.11+ and supply `/srv/inputs/password.txt` with mode `0600`.

```bash
apt-get update
apt-get install -y python3 curl openssl ca-certificates tar gzip coreutils
VERSION=v0.1.2
curl -fL --proto '=https' --proto-redir '=https' -o source.tar.gz \
  "https://github.com/lorebayai/lorebay/archive/refs/tags/$VERSION.tar.gz"
tar -xzf source.tar.gz
cd "lorebay-${VERSION#v}"
RELEASE="https://github.com/lorebayai/lorebay/releases/download/$VERSION"
ARCHIVE=lorebayd-x86_64-unknown-linux-musl.tar.gz
curl -fL --proto '=https' --proto-redir '=https' -O "$RELEASE/$ARCHIVE"
curl -fL --proto '=https' --proto-redir '=https' -O "$RELEASE/SHA256SUMS"
grep -E "^[a-fA-F0-9]{64} [ *]$ARCHIVE$" SHA256SUMS | sha256sum -c -
mkdir release
tar -xzf "$ARCHIVE" -C release lorebayd
./release/lorebayd hash-password < /srv/inputs/password.txt
```

Create `/srv/inputs/users.yaml` with that hash using the
[users-file format](../../lorebayd/README.md#usage); hashing needs no users file or server.
Remove the password input. Obtain the domain's certificate and unencrypted key:

```bash
python3 -B deploy/native/install.py install --version "$VERSION" \
  --domain lorebay.example.com --users /srv/inputs/users.yaml \
  --certificate /srv/inputs/certificate.pem --key /srv/inputs/key.pem \
  --data /srv/lorebay-data
systemctl status lorebay nginx
journalctl -u lorebay
curl https://lorebay.example.com/readyz
```

HTTPS downloads are checked against `SHA256SUMS`. `--archives "$PWD"` reuses
root-owned regular archives/sums files without group/other write access; the
directory and ancestors must also be root-owned without group/other write.
`--release-url` selects an HTTPS mirror. Identical repeats do nothing; changed
inputs or versions are refused. nginx installs if absent without upgrades.

Use an empty root-owned data directory or a missing path on a writable local
filesystem outside `/tmp` and `/var/tmp`. Every existing ancestor must be
root-owned, traversable by the service user, and not group- or other-writable.
Missing ancestors become root-owned `0755`; data becomes `lorebay`-owned `0700`,
with database `0600`. Existing databases are refused; stop other writers first.
The users file and all its ancestors must be root-owned and not writable by
group or others; it passes through systemd `LoadCredential=`. Supplied files
keep contents, owner and mode. Only nginx's root master reads the copied key under
`/etc/nginx/lorebay-tls` (directory `0700`, key `0600`). nginx must include
`/etc/nginx/conf.d/*.conf`; address-specific 443 listeners need manual setup.
Other sites keep their files. Without an explicit 443 `default_server`, the
installer adds a rejecting default; unknown names stop reaching the implicit
site. Installation checks that nginx serves the supplied certificate.

Replace the certificate, or uninstall while keeping data and supplied inputs:

```bash
python3 -B /opt/lorebay/deploy/common/replace-certificate.py \
  --domain lorebay.example.com --certificate /srv/inputs/new.pem \
  --key /srv/inputs/new-key.pem
python3 -B deploy/native/install.py uninstall
```

Replacement validates the staged pair and nginx configuration, reloads, checks
the served certificate, then deletes the previous pair. Failure restores it.
For a container, use `--directory` for the host-mounted certificate directory,
`--container` for nginx and `--connect` for its reachable address on port 443.
`--nginx-directory` defaults to `/etc/nginx/lorebay-tls`; mount the whole directory.
Uninstall retains nginx, transfers kept data and its contents to root, removes
the service account, and reloads running nginx. Reinstall with a new empty path.

For single-host use, set `public_url` in `/etc/lorebay/lorebayd.json` to the exact
loopback origin, e.g. `http://127.0.0.1:8080`, then `systemctl restart lorebay`.
SSH forwarding needs the same local port (`ssh -L 8080:127.0.0.1:8080 server`).
Domain sign-ins are refused while set. Changing the origin invalidates access
tokens but keeps sign-ins; `lorebay logout` ends one. Restore HTTPS and restart.

Run `moon run deploy:check` from the repo root with Docker, OpenSSL and Python 3.11+.

## Configuration

Paths must be absolute, using letters, digits, `/`, `.`, `_` and `-`. Required:
`--domain` (lowercase DNS name), `--users`, `--certificate`, `--key` and `--data`.
Select the release explicitly with `--version`. Service keys are in
`/etc/lorebay/lorebayd.json`; see [the reference](../../lorebayd/README.md#configuration).
The four application budgets are 1000; per-caller policy is in [nginx.conf.in](../common/nginx.conf.in).
Match `client_max_body_size` to `upload_size_limit_bytes`; run `nginx -t` before reload.

## Docs

Start at the [docs overview](../../docs/overview.md).
