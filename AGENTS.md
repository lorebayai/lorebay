# Lorebay

Rules for agents working anywhere in this repository. Each folder's own
`AGENTS.md` adds the rules for that folder.

## Where things live

| File | Holds |
| --- | --- |
| [README.md](README.md) | Orientation: what Lorebay is, how to get started, the repository layout |
| [CONTEXT.md](CONTEXT.md) | The glossary: the canonical term for every domain concept |
| [docs/](docs/overview.md) | Design docs: the product and system design as it stands |
| `AGENTS.md` | Rules for agents working in its folder |

## Writing

- Give each fact one home, and link to that home from everywhere else.
- Write in American English, using the terms in [CONTEXT.md](CONTEXT.md)
  exactly.
- Describe the current state. History lives in git and pull requests.
- When a change makes a doc wrong, fix that doc in the same pull request.

## Agent instructions

- Every folder has an `AGENTS.md` holding only the rules for that folder, and
  a `CLAUDE.md` beside it that is a symlink to it: `ln -s AGENTS.md CLAUDE.md`.
  A new folder gets both in the change that creates it.
- A rule that applies to several folders lives in their nearest common
  parent's `AGENTS.md`.
- Each `AGENTS.md` is plain Markdown that reads on its own. Point to other
  files with relative Markdown links, so every harness reads it the same way.

## Pull requests

- The title is a Conventional Commits line, `<type>(<scope>): <outcome>`,
  where the optional scope names the topic, such as
  `docs(contracts): define artifact uploads`.
- The body links its issue: `Fixes #n` when the pull request completes it,
  `Part of #n` when it does part of it.
