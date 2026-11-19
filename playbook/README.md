# Playbook

Conventions for coding agents working in this repo. Each file is a fragment
that an `AGENTS.md` imports, so an agent loads only the rules for the folder
it works in.

## Usage

Import a fragment from an `AGENTS.md`, with the path written relative to that
`AGENTS.md`:

```markdown
Follow these playbook fragments:

- @playbook/core/documentation.md
```

From `lorebayd/AGENTS.md`, the same import is
`@../playbook/core/documentation.md`. The full rules for `AGENTS.md` files
are in [documentation.md](core/documentation.md#agent-instructions).

| Folder | Holds | Imported by |
| --- | --- | --- |
| `core/` | Rules for every change in the repo | The root `AGENTS.md` |
| `languages/` | Rules for one language | The `AGENTS.md` of projects in that language, or of a folder above several of them |
| `topics/` | Rules for one kind of work | The `AGENTS.md` of folders where that work happens |

Fragments:

- [core/code-comments.md](core/code-comments.md): when code gets a comment,
  and how it reads.
- [core/contribution.md](core/contribution.md): issues, worktrees, pull
  requests, reviews, landing and cleanup.
- [core/documentation.md](core/documentation.md): where docs live, READMEs,
  `AGENTS.md` files.
- [languages/rust.md](languages/rust.md): Rust crates, dependencies, lints
  and code rules.
- [languages/typescript.md](languages/typescript.md): TypeScript packages,
  the pnpm catalog, checks, and the statically exported Next.js pages.
- [topics/design-docs.md](topics/design-docs.md): placing, shaping,
  writing and reviewing design docs, in `docs/` and `lib/docs/`.
- [topics/github-actions.md](topics/github-actions.md): writing and pinning
  GitHub Actions workflows.
- [topics/seaorm.md](topics/seaorm.md): storing data through SeaORM, on
  SQLite and, where it differs, on Postgres: migrations, generated entities,
  repositories, queries and transactions.
- [topics/testing.md](topics/testing.md): the integration, E2E and unit test
  levels, and where each one's tests live.

## Invariants

- A fragment never `@`-imports another file. Imports happen only in
  `AGENTS.md` files, so what an agent loads is visible in one place. Refer to
  another fragment with a normal Markdown link.
- A fragment holds rules shared by more than one folder, or rules for one
  kind of work. A rule for one folder goes in that folder's `AGENTS.md`.
- Everything imported loads in full into the agent's context. A `core/`
  fragment loads in every session and must stay within 150 lines; any other
  fragment within 300.
- Fragments state rules, in the present tense, using must, must not, should
  and may. They hold no history.
- Adding, renaming or deleting a fragment updates the list above and every
  `AGENTS.md` that imports it, in the same change.
