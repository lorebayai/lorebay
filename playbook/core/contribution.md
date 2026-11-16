# Contribution

How a change lands in Lorebay: issues, worktrees, pull requests, reviews,
landing and cleanup. Applies to every change in the repo.

## Issues

- Work is tracked in GitHub Issues on `lorebayai/lorebay` and planned in the
  organization's [Lorebay project](https://github.com/orgs/lorebayai/projects/2),
  number 2. The organization's other projects, such as Genesis, plan other
  repositories; a Lorebay issue never goes into them.
- Add each new issue to the Lorebay project:

  ```bash
  gh project item-add 2 --owner lorebayai --url <issue-url>
  ```

- Each issue has a type: bug, feature or task. Split large work into
  sub-issues, and record ordering with blocked-by relationships.
- Before starting an issue, read its asks, acceptance criteria and
  dependencies, assign it, and set its status in the Lorebay project to
  In Progress.

## Worktrees

- Every change happens in a git worktree at `.worktrees/<branch>`, branched
  from `origin/main`. The main checkout stays on `main`.
- Name the branch `<handle>-dev`, where `<handle>` is your short name, such
  as `fj`. While that name is in use, take the next `<handle>-dev-<n>`.
- A name is in use when any of these commands lists it. Stale names in
  `git branch -a` and merged PRs in `gh pr list` don't count.

  ```bash
  git ls-remote --heads origin '<handle>-dev' '<handle>-dev-*'
  git branch --list '<handle>-dev' '<handle>-dev-*'
  git worktree list
  ```

- Never delete another agent's branch or worktree to free a name; take the
  next number instead.
- Create the worktree from the main checkout:

  ```bash
  git fetch origin
  git worktree add --no-track -b "$BRANCH" ".worktrees/$BRANCH" origin/main
  ```

- Push the branch the first time with `git push -u origin "$BRANCH"`.

## Pull requests

- Title: `<type>(<scope>): <outcome>`, where `<type>` is a Conventional
  Commits type and the optional `<scope>` names the project or topic:
  `docs(contracts): define artifact uploads`.
- The title carries no issue number; the squash merge appends the PR number.
- The body fills in the sections of the pull request template, and opens
  with one intent line per linked issue:

  | Line | Intent |
  | --- | --- |
  | `Fixes #n` | Completes the issue; GitHub closes it on merge |
  | `Part of #n` | Partial work |
  | `Related to #n` | Context only |

- Open a PR ready for review, not as a draft.

## CI

- Run the fast checks the change touches (format, lint, type checks, unit
  tests) before the expensive ones (integration tests, `moon ci`), and batch
  every known fix before the next expensive run.
- On a failure, read and classify it before rerunning. Rerun unchanged code
  at most once, and only the failed job; any further run needs a fix or new
  evidence. A passing rerun does not erase the failure.
- Secrets stay out of logs, comments and commands, including while
  debugging CI.

## Reviews

- Codex reviews a PR when it is opened ready for review. A 👍 reaction with
  no new comments on the latest commit is its clean pass.
- Watch CI, Codex and human reviews until the PR lands, or the user parks
  it.
- Before each follow-up push, read every new comment and the latest CI run,
  and fold the fixes you accept into that push.
- Verify a reviewer's claim before acting on it. Every comment ends in a
  change, with a reply confirming it, or in a reply explaining why it is
  wrong or out of scope.
- Reply to each inline thread on the thread itself, then resolve it.
  Top-level comments and review bodies cannot be resolved; reply to each.
  List unresolved threads with:

  ```bash
  gh api graphql -f query='query { repository(owner: "lorebayai", name: "lorebay") {
    pullRequest(number: <N>) { reviewThreads(first: 100) {
      nodes { id isResolved path line } } } } }'
  ```

- A thread with an open objection stays unresolved; escalate in the thread.
- After pushing fixes for review feedback, comment `@codex review` once,
  after the last push of a burst.

## Landing

A PR lands when:

- its acceptance criteria are met;
- every CI check is green;
- it has one approving review, and no review requests changes;
- every comment is answered and every thread resolved.

```bash
gh pr merge <N> --squash --body 'See the PR for scope and validation.'
```

- Leave out `--delete-branch` while a worktree uses the branch; cleanup
  deletes it.
- `--admin` needs the user's grant for that PR. A grant never carries over
  to another PR.

## Cleanup

After your PR merges, clean up its worktree and branch from the main
checkout, every time:

```bash
git -C ".worktrees/$BRANCH" status --short
git fetch --prune origin
git pull --ff-only
git worktree remove ".worktrees/$BRANCH"
git branch -D "$BRANCH"
git push origin --delete "$BRANCH" || true
```

- `status --short` must print nothing before the worktree is removed.
- Removing the worktree deletes its ignored files, `docs/superpowers/` and
  `.superpowers/` included. Copy nothing from it into the main checkout;
  anything worth keeping lands in the PR, as a doc or as code.
- `git branch -D` is expected: a squash-merged branch looks unmerged to git.
- Clean up only the worktree and branch of the PR you merged.
