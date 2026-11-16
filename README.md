# Lorebay

Lorebay lets a team see what its members' agents did and how they did it. A
single `lorebayd` process stores each agent session as a task with its notes
and files, keeps the team's shared Knowledge, and serves the web pages, the
Web API and the MCP server that people and agents use.

## Getting started

Clone the repo:

```bash
git clone git@github.com:lorebayai/lorebay.git
```

Make each change in its own worktree, as
[contribution.md](playbook/core/contribution.md#worktrees) describes.

Install [rustup](https://rustup.rs); `rust-toolchain.toml` picks the Rust
release. Install moon, then add `~/.moon/bin` to `PATH`:

```bash
bash <(curl -fsSL https://moonrepo.dev/install/moon.sh)
```

moon installs the Node.js and pnpm releases that `.moon/toolchains.yml` pins
the first time a task needs them. Run every check from the repo root:

```bash
moon ci
```

## Repository layout

- `docs/`: product and system design docs.
- `lorebayd/`: the Rust crate that builds the `lorebayd` executable.
- `playbook/`: conventions for coding agents, imported from `AGENTS.md`
  files.

## Docs

Start at the [docs overview](docs/overview.md).
