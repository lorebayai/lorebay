# Contributing to Lorebay

Thanks for helping. This guide is for contributors outside the Lorebay team;
the team's own workflow is in
[contribution.md](playbook/core/contribution.md).

## Before you start

- Report a security problem privately, as [SECURITY.md](SECURITY.md)
  describes, never in an issue.
- Open an issue before any change larger than a small fix, so we agree on
  the approach first. Lorebay does only what its
  [design docs](docs/overview.md) describe, so a new feature starts as a
  change to those docs.
- Follow the [Code of Conduct](CODE_OF_CONDUCT.md).

## Making a change

1. Fork the repository and create a branch; any branch name works.
2. Set up the tools as the [README](README.md#getting-started) describes.
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
