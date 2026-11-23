# Lorebay

Lorebay lets a team see what its members' agents did and how they did it. A
single `lorebayd` process stores each agent session as a task with its
checkpoints and files, keeps the team's shared Knowledge, and serves the web
pages, the Web API and the MCP server that people and agents use.

## Getting started

`lorebayd` runs on one server host for the whole team. Each person runs the
`lorebay` CLI and an agent harness, Claude Code or Codex, on their own machine,
and both reach the server over HTTPS.

On the server host, download the `lorebayd` archive for its architecture from
the latest release, check it against `SHA256SUMS` and unpack it; on Debian,
install `curl` and `ca-certificates` first. The archives run on Linux, on
x86_64 or aarch64, in any distribution.

```bash
base=https://github.com/lorebayai/lorebay/releases/latest/download
archive=lorebayd-$(uname -m)-unknown-linux-musl.tar.gz
curl -fsSL -O "$base/$archive" -O "$base/SHA256SUMS"
sha256sum -c --ignore-missing SHA256SUMS && tar -xzf "$archive"
```

Write a configuration file, such as `lorebayd.json`;
[lorebayd/README.md](lorebayd/README.md#configuration) lists every key. Then
create the users file: hash each person's password and list each username with
its hash, in single quotes, as [lorebayd/README.md](lorebayd/README.md#usage)
shows:

```json
{
  "public_url": "https://lorebay.example.com",
  "data_directory": "/var/lib/lorebay",
  "users_file": "/etc/lorebay/users.yaml"
}
```

```bash
mkdir -p -m 0700 /etc/lorebay
./lorebayd hash-password < alice-password.txt
```

Start `./lorebayd --config lorebayd.json` behind a reverse proxy that
terminates HTTPS at `public_url` and forwards to `listen_address`,
`127.0.0.1:8080` by default; harnesses refuse OAuth over plain HTTP outside
loopback. Uploads pass through the proxy, so let it accept request bodies up to
`upload_size_limit_bytes`, 5 GB by default. Set limits per caller on the proxy,
and give it `/healthz` and `/readyz` as health checks. Keep the data directory
on a local file system; `lorebayd` creates it with mode `0700` and the database
with mode `0600`. The directory also holds the signing keys. Back it up only
while `lorebayd` is stopped ([Storage](docs/architecture/storage.md#3-layers)).

On each person's machine, get `lorebay` from the same release on Linux, as
above with `lorebay-$(uname -m)-unknown-linux-musl.tar.gz`, and install it with
the first command below; elsewhere, use the second, which needs Rust. A server
whose certificate comes from a private authority needs `SSL_CERT_FILE`, as
[skill/README.md](skill/README.md#usage) describes. Then sign in, with
`--device` on a machine without a browser, and create a space:

```bash
install -m 0755 lorebay /usr/local/bin/
cargo install --locked --git https://github.com/lorebayai/lorebay --tag v0.1.1 lorebay-cli
lorebay login https://lorebay.example.com
lorebay space create --name Team --summary 'Team work'
```

Install the skill and connect each person's agent as
[skill/README.md](skill/README.md) describes.

To change Lorebay, set up the tools as
[CONTRIBUTING.md](CONTRIBUTING.md#set-up-the-tools) describes.

## Repository layout

- `base/`: the Rust library for configuration, logging and clocks.
- `cli/`: the Rust crate that builds the `lorebay` command-line client.
- `docs/`: product and system design docs.
- `lib/`: the library crates and design for features every product shares.
- `lorebayd/`: the Rust crate that builds the `lorebayd` executable and
  the home of its `migration/` crate.
- `playbook/`: conventions for coding agents, imported from `AGENTS.md`
  files.
- `skill/`: the agent skill Lorebay ships, which tells agents how to record
  their work in a space.
- `web/`: the TypeScript web pages embedded by `lorebayd`.

## Docs

Start at the [docs overview](docs/overview.md).

## License

Lorebay is licensed under the [Apache License, Version 2.0](LICENSE).

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in Lorebay by you, as defined in the Apache-2.0
license, shall be licensed as above, without any additional terms or
conditions.
