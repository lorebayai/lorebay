# GitHub Actions

How workflows and composite actions under `.github/` are written and pinned.
Imported by `.github/AGENTS.md`.

## Steps

- Every step, including a `uses:` step, has a `name` of two or three
  capitalized words, such as `Checkout Repo` or `Run Repo Checks`.
- A `run:` command fits on one line. Logic that needs more is a moon task,
  which the step calls.
- A step sets environment variables in its `env:` mapping, never with
  `export`, `env` or a `VAR=value` prefix in the command.

```yaml
- name: Verify Action Lockfile
  env:
    GH_TOKEN: ${{ github.token }}
  run: gh actions-lock --verify
```

## Pinning

- `.github/workflows/actions.lock` pins every action, and every action those
  actions use, to a commit. Regenerate it with `gh actions-lock` after
  changing a `uses:` reference or adding a workflow, and commit it with that
  change. It is never edited by hand.
- Workflows reference actions by full version tag, such as
  `actions/checkout@v4.2.2`. The lockfile holds the commit.
- A reusable workflow called with `jobs.<job_id>.uses` is pinned by full
  commit SHA, with its version in a trailing comment, because the lockfile
  does not cover it.
- Local actions in `.github/actions/` take no version.
- Dependabot updates actions and their lockfile entries in the same PR.

## Runs

- Each workflow runs from one trigger. The same commits never run one
  workflow from both `push` and `pull_request`.
- Each workflow sets a `concurrency` group, so a new push to a PR cancels
  the run it supersedes:

  ```yaml
  concurrency:
    group: ${{ github.workflow }}-${{ github.ref }}
    cancel-in-progress: ${{ github.event_name == 'pull_request' }}
  ```

- A new check joins an existing suite, such as a moon task, so it covers
  future files and cases. A workflow step exists only for validation that
  runs on every change.
