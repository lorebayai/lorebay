# Lorebay

Lorebay lets a team see what its members' agents did and how they did it. A
single `lorebayd` process stores each agent session as a task with its
checkpoints and files, keeps the team's shared Knowledge, and serves the web
pages, the Web API and the MCP server that people and agents use.

## Getting started

`lorebayd` runs on one server host for the whole team. Each person runs the
`lorebay` CLI and an agent harness, Claude Code or Codex, on their own machine,
and both reach the server over HTTPS.

On the server host, follow [the native guide](deploy/native/README.md) to install
`lorebayd` as a systemd service behind nginx HTTPS. The installer qualifies
x86_64 Debian 12 and Ubuntu 24.04; Linux musl archives are also available for
manual use on x86_64 and aarch64. Supply the users file, domain, certificate,
key and a private local data path. The installer checks the selected archive
against the release's `SHA256SUMS`.
Back up the data directory only while stopped
([Storage](docs/architecture/storage.md#3-layers)).

On each person's machine, get `lorebay` from the same release on Linux, using
`lorebay-$(uname -m)-unknown-linux-musl.tar.gz`, check it against `SHA256SUMS`,
unpack it, and install it with the first command below; elsewhere, use the
second, which needs Rust. A server
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
- `deploy/`: installation guides, shared nginx policy and certificate tools.
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
