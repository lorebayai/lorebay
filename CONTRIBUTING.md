# Contributing to Lorebay

Thanks for helping. This guide is for contributors outside the Lorebay team,
except its tool setup, which serves the team too; the team's own workflow is
in [contribution.md](playbook/core/contribution.md).

## Before you start

- Report a security problem privately, as [SECURITY.md](SECURITY.md)
  describes, never in an issue.
- Open an issue before any change larger than a small fix, so we agree on
  the approach first. Lorebay does only what its
  [design docs](docs/overview.md) describe, so a new feature starts as a
  change to those docs.
- Follow the [Code of Conduct](CODE_OF_CONDUCT.md).

## Set up the tools

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

Install Python 3.11 or later for the library boundary check, entity generation
and deployment syntax checks. Deployment checks cover every `deploy/` subfolder
and need OpenSSL and Docker;
they run `nginx -t` in a pinned nginx image with a throwaway certificate.

The optional [deployment suite](tests/deploy/README.md) needs an x86_64 Linux
host, Python 3.12+, Docker Engine with Compose and buildx, GitHub CLI with
`gh attestation`, OpenSSL, curl and passwordless sudo, plus
Skopeo for local image archives. Docker must support privileged systemd
containers. Chromium and pinned Playwright
run inside the disposable client image. Supply a test domain, certificate,
key and authority, two test usernames and an existing GitHub sign-in with
release and package access in the suite's one configuration file.

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

## Making a change

1. Fork the repository and create a branch; any branch name works.
2. [Set up the tools](#set-up-the-tools).
3. Follow the rules in the `AGENTS.md` files and the
   [playbook](playbook/README.md). They are written for coding agents and
   apply to people too.
4. Fix every doc the change makes wrong, in the same pull request.
5. Run `moon ci` from the repo root until it passes.

## Pull requests

- The title and the issue line follow
  [contribution.md](playbook/core/contribution.md#pull-requests), and the
  template lays out the body.
- A pull request lands once CI passes, a maintainer approves it, and every
  review comment is answered. Codex reviews it automatically.
- Pull requests are squash-merged.

## License

Contributions are licensed as the [README](README.md#license) states.
