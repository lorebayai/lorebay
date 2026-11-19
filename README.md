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

Install [rustup](https://rustup.rs), then install the Rust release that
`rust-toolchain.toml` pins, before moon runs tasks in parallel:

```bash
rustup toolchain install
```

Install Python 3.11 or later for the library boundary check and entity generation.

Install moon, then add `~/.moon/bin` to `PATH`:

```bash
bash <(curl -fsSL https://moonrepo.dev/install/moon.sh)
```

moon installs the Node.js and pnpm releases that `.moon/toolchains.yml` pins
the first time a task needs them. Start the Postgres service for the library
integration tests. The tests use the fixed loopback address below.

```bash
docker run --rm -d --name lorebay-test-postgres \
  -e POSTGRES_USER=lorebay_test -e POSTGRES_PASSWORD=lorebay_test \
  -e POSTGRES_DB=postgres -p 127.0.0.1:55432:5432 postgres:18.7@sha256:2ea8630df176ad090ec92136dee850397c5af8781e4bcf2e4162593691c61bbc
docker exec lorebay-test-postgres pg_isready -h 127.0.0.1 -U lorebay_test -d postgres
```

Once Postgres is ready, run every check from the repo root:

```bash
moon ci
docker stop lorebay-test-postgres
```

## Repository layout

- `base/`: the Rust library for configuration, logging and clocks.
- `cli/`: the Rust crate that builds the `lorebay` command-line client.
- `docs/`: product and system design docs.
- `lib/`: the library crates and design for features every product shares.
- `lorebayd/`: the Rust crate that builds the `lorebayd` executable and
  the home of its `migration/` crate.
- `playbook/`: conventions for coding agents, imported from `AGENTS.md`
  files.
- `web/`: the TypeScript web pages embedded by `lorebayd`.

## Docs

Start at the [docs overview](docs/overview.md).

## License

Lorebay is licensed under the [Apache License, Version 2.0](LICENSE).

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in Lorebay by you, as defined in the Apache-2.0
license, shall be licensed as above, without any additional terms or
conditions.
