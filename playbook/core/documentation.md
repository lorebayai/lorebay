# Documentation

Where each kind of document lives and what belongs in it. Applies to every
change in the repo.

## Homes

| Document | Lives in | Holds |
| --- | --- | --- |
| `README.md` | The repo root and every project folder | Orientation: what it is, how to use it |
| `AGENTS.md`, with a `CLAUDE.md` symlink | The repo root and folders with their own agent rules | Rules for agents working in that folder |
| Design docs | `docs/` for Lorebay and `lib/docs/` for the library, by topic | The current product and system design |
| Glossary | `CONTEXT.md` at the repo root for Lorebay, `lib/CONTEXT.md` for the library | Canonical domain terms |
| `CONTRIBUTING.md`, `SECURITY.md`, `CODE_OF_CONDUCT.md` | The repo root | How outside contributors propose changes, report vulnerabilities and behave |
| Issue forms and the pull request template | `.github/` | What a new issue or pull request asks for |
| Playbook fragments | `playbook/` | Conventions shared by several folders |
| Drafts, plans, reviews | `docs/superpowers/` and `.superpowers/` (gitignored) | Local working files |

A **project folder** has its own manifest (`Cargo.toml` or `package.json`).
It can be a top-level folder, a crate under `lib/`, a product-owned nested
crate such as `lorebayd/migration/`, or a test suite under `tests/`.

- Each fact has one home. Link to it instead of copying it.
- Write in American English, using the terms of the folder's glossary
  exactly.
- Committed docs describe the current state. History lives in git and pull
  requests.
- A change that makes a doc wrong fixes that doc in the same pull request.
- Never commit, or link to, files in `docs/superpowers/` or `.superpowers/`.
- Design docs follow [design-docs.md](../topics/design-docs.md) and the
  `AGENTS.md` of their docs folder.

### The library's folder

`lib/` holds the library that Lorebay builds on, with its crates, its
design docs and its glossary. It is a second home for design docs, under
the same rules, plus one:

- Nothing in `lib/` depends on or links to `lorebayd/`, `cli/`, `web/`,
  `base/` or the root `docs/`. The root Cargo workspace settings, the
  playbook and the toolchain stay shared, so an `AGENTS.md` there may import
  a playbook fragment and a doc may link to one.

## READMEs

A README orients someone arriving at a folder today: what it is, how to use
it, and where to read more.

### Sections

A project README has these sections, in this order, and no others:

| Section | Holds | Required |
| --- | --- | --- |
| `# <name>` and one paragraph | What it is and who uses it | Yes |
| `## Usage` | The commands to build, test and run it | Yes |
| `## Configuration` | A table of env vars and config keys | When it reads config |
| `## Invariants` | Rules contributors must follow that types, tests and lint don't enforce | When there are any |
| `## Docs` | One link to its design home, or to the doc that covers it | When there is one |

The root README has `# <name>` and one paragraph, `## Getting started`,
`## Repository layout` (one line per top-level folder), `## Docs` and
`## License`.

Other folders with a README, such as `playbook/`, use the project sections
that apply.

### What stays out

| Content | Its home |
| --- | --- |
| Change history: "now supports", "previously", "no longer", dates, versions | Git log, the PR description, `CHANGELOG.md` for released artifacts |
| Issue keys and PR numbers | The PR or issue |
| Design rationale, trade-offs, findings, benchmarks | A design doc |
| Links to individual design docs | One link to the design home's `overview.md` |
| Why a particular line of code exists | A code comment |
| Full API reference | Generated docs: rustdoc, typedoc |
| Debugging notes and troubleshooting history | Nowhere |
| Instructions for agents | `AGENTS.md` |

### Editing

- Edit in place: rewrite the sentence the change made wrong. Never add a note
  about the change.
- Change a README only when what its readers see changes: commands,
  configuration, public behavior, or who uses it. Internal refactors leave it
  alone.
- Keep a line only if someone new needs it to use or change the folder today.
- A README must stay within 100 lines. Past that, move content to its home.
- Read a project's README before changing the project.

## Agent instructions

- `AGENTS.md` is the agent instruction file. The `CLAUDE.md` beside it is a
  symlink to it (`ln -s AGENTS.md CLAUDE.md`), never a separate file, added
  in the same change.
- Add an `AGENTS.md` to a folder only when that folder needs rules or
  fragments its parents don't give. Claude Code loads a nested one when it
  works in that folder.
- An `AGENTS.md` has this shape:

  ```markdown
  Follow these playbook fragments:

  - @../playbook/languages/rust.md

  ## <Folder> conventions

  - Rules that apply only to this folder.
  ```

- Write each import path relative to the `AGENTS.md` that holds it, as its own
  list item with nothing after the path.
- Import a fragment at the deepest folder that needs it. The root imports only
  `core/` fragments, because everything imported loads into every session
  under that folder.
- Never restate a rule an imported fragment already gives.
- An `AGENTS.md` holds rules, not orientation. To give agents a folder's
  README, import it (`@README.md`) instead of restating it.
