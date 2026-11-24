# Deployment

Install one Lorebay server on an x86_64 Debian 12 or Ubuntu 24.04 host with
systemd as PID 1. nginx serves HTTPS and caller limits; `lorebayd` listens on
loopback. Linux musl archives also exist for aarch64 for manual use; the
installer qualifies only these two x86_64 hosts. Fresh installs only.

## Usage

Install prerequisites, prepare the users file as
[lorebayd/README.md](../lorebayd/README.md#usage) describes, and obtain a
certificate and its unencrypted private key for the domain. Run from a checkout:

```bash
sudo apt-get update
sudo apt-get install -y python3 curl openssl ca-certificates
sudo python3 -B deploy/install.py install --version v0.1.1 \
  --domain lorebay.example.com --users /srv/inputs/users.yaml \
  --certificate /srv/inputs/certificate.pem --key /srv/inputs/key.pem \
  --data /srv/lorebay-data
sudo systemctl status lorebay nginx
sudo journalctl -u lorebay
curl https://lorebay.example.com/readyz
```

The selected release downloads over HTTPS and is checked against `SHA256SUMS`.
For local archives, append `--archives /srv/releases`, containing
`lorebayd-x86_64-unknown-linux-musl.tar.gz` and `SHA256SUMS`. `--release-url`
selects an HTTPS mirror directory instead. An identical repeated install does
nothing; another version or changed inputs are refused.

Use an empty data directory on a local writable filesystem, or a missing path
whose parent the service user can traverse. The installer checks for another
writer and unrecognized data, then makes the directory owned by `lorebay` with
mode `0700`; `lorebayd` creates its database `0600`. Stop other writers before
installing. The users file passes through systemd `LoadCredential=`; supplied
files retain their contents, owner and permissions. Only nginx's root master
reads the copied key under `/etc/nginx/lorebay-tls` (directory `0700`, key
`0600`). nginx is installed if absent, without upgrading an existing package.
Its configuration must include `/etc/nginx/conf.d/*.conf`; address-specific
443 listeners require manual configuration. Other sites keep their files.

Replace the certificate, or uninstall while keeping data and supplied inputs:

```bash
sudo python3 -B /opt/lorebay/deploy/replace-certificate.py \
  --domain lorebay.example.com --certificate /srv/inputs/new.pem \
  --key /srv/inputs/new-key.pem
sudo python3 -B deploy/install.py uninstall
```

Replacement validates a staged pair and nginx configuration, then reloads and
checks the served certificate. Failure restores the previous pair. For a
container, use the same script with `--directory` naming its host-mounted
certificate directory, `--container` naming nginx and `--connect` naming its
reachable address on port 443. `--nginx-directory` defaults to
`/etc/nginx/lorebay-tls`; mount the directory, including its `live` symlink.
Uninstall retains nginx and the service account so preserved data keeps its
owner; it reloads a running nginx without stopping it.

For single-host use or tests, edit `/etc/lorebay/lorebayd.json` and set
`public_url` to the exact loopback origin clients use, such as
`http://127.0.0.1:8080`, then `sudo systemctl restart lorebay`. An SSH forward
must use the same local port (`ssh -L 8080:127.0.0.1:8080 server`). While set,
new sign-ins at the domain are refused. Changing `public_url` invalidates issued
access tokens but does not end sign-ins; `lorebay logout` ends one. Restore the
HTTPS origin and restart when finished.

Run static syntax and nginx checks from the repository root with Docker,
OpenSSL and Python 3.11 or later:

```bash
moon run deploy:check
```

## Configuration

| Input | Flag / default |
| --- | --- |
| Host | x86_64 Debian 12 or Ubuntu 24.04, systemd |
| DNS domain | `--domain`, required; lowercase, no port or scheme |
| Users file | `--users`, required |
| Certificate chain / key | `--certificate` / `--key`, required |
| Empty local data path | `--data`, required |
| Release | `--version v0.1.1`; HTTPS, or `--archives` |

Paths must be absolute, using letters, digits, `/`, `.`, `_` and `-`.
Service keys live in `/etc/lorebay/lorebayd.json`; their reference is
[lorebayd/README.md](../lorebayd/README.md#configuration). The four application
rate limits are team budgets of 1000; per-caller policy and upload limits live
in [nginx.conf.in](nginx.conf.in). Match `client_max_body_size` to
`upload_size_limit_bytes` when changing that key; validate with `nginx -t`
before reloading nginx.

## Docs

Start at the [docs overview](../docs/overview.md).
