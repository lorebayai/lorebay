# Changelog

Lorebay's releases: the `lorebayd` server, the `lorebay` CLI and the agent
skill, tagged `v<version>`. The library's changes are in
[lib/CHANGELOG.md](lib/CHANGELOG.md).

## Unreleased

### Added

- `lorebayd` serves read-only Knowledge-change request cards to chat apps
  that support MCP Apps. Pending cards label the proposed entry action.

### Changed

- The native installer supports x86_64 Debian 12 and Ubuntu 26.04, and
  refuses earlier Ubuntu releases.

### Fixed

- Revoking an unknown agent (`DELETE /api/v1/agents/{name}` and
  `lorebay agent revoke <name>`) returns 404 `agent.not_found` with a `name`
  field error naming the agent and pointing to `lorebay agent list`, instead
  of `http.not_found`. Another person's agent keeps the same answer as an
  unknown name, and unknown paths keep `http.not_found`.

## 0.1.4

### Changed

- The CLI's human output prints times as RFC 3339 in UTC, and `task show`
  prints one row per file with its key, name, size, summary, agent name and
  times. Scripts parsing human output see new time and file formats;
  `--output json` is unchanged. `checkpoint delete` accepts the printed
  time as well as Unix milliseconds.
- CLI usage errors and command-group help span lines, as `--help` does.
  Server-supplied text still has control characters escaped; exit codes
  and JSON failures are unchanged.
- The Web API request submit routes refuse `null` for `target_name` and
  `target_key`, which were previously accepted as absent. MCP already
  refused these values.

### Fixed

- New `uploads/` and `agent_files/` directories are private and writable
  by the server user under any umask, with mode `0700`. Existing
  directories keep their modes; stored file content remains `0600`.
- Request body errors name the missing, wrongly typed, invalid or unknown
  field in `request.invalid_body`, including on the request submit routes.
  MCP reports every offending argument; the Web API reports the first
  field deserialization identifies. Non-object or unparseable bodies keep
  one `body` entry; messages do not repeat submitted values.
- Compose checks nginx's configuration and HTTPS readiness through to
  `lorebayd`, so `up --wait` waits for both services and `ps` reports both
  health states.
- Compose's nginx access log omits its health check's loopback
  `GET /readyz` requests; other requests are logged as before. Native
  installations' logging is unchanged.

## 0.1.3

### Breaking

- **Data directories:** directories written by `v0.1.0` through `v0.1.2`
  are refused at startup without changes. Delete the old data directory
  and restart with an empty one; the users file recreates the accounts.
  Old data is not migrated. `v0.1.3` is the first release whose data is kept.
- **Task lifecycle:** six states replace `in progress` and `ended`:
  `planned`, `working`, `waiting`, `done`, `failed` and `canceled`, with
  reasons and one holder. Only the holder writes; agents claim planned
  tasks, take over within their party and release tasks before work is
  recorded. Requests can wait for a decision and resume their task.
  MCP adds `claim_task` and `release_task`; task fields and filters use
  `state`. The CLI replaces `task close` with `task plan` and `task cancel`
  (with required `--reason`), and replaces `task list --status` with
  `--state`. Passing `--status` to `task list` is a usage error (exit 2).
  It adds `--claimable`; `--own-open` lists `working` and `waiting` tasks.
- **Results:** each write keeps a numbered result version with its agent,
  person and time. Updates name the `result_version` they replace; stale
  writes are refused with `task.result_changed`. The CLI adds `task results`
  to read the history; MCP and the Web API expose versions and history.
- **Requests and Knowledge:** requests can be revised while pending,
  keeping their key and latest revision. `outdated` replaces
  `changed_since_requested`; outdated requests cannot be approved.
  Decisions name the revision read, and direct Knowledge changes, deletion
  and rollback name the version replaced. Stale writes are refused.
  MCP adds `revise_request`; the CLI adds `request revise`, `--revision`
  for decisions and `--version` for direct Knowledge writes.
- **Skill:** agent practices cover planning, claims, takeover, waiting,
  ending and reopening, result versions and request revisions. Install
  the skill and CLI at the server's release.

### Added

- Checkpoints and files show the writing or registering agent's copied
  name. Tasks show the person, agent and time of the current state change;
  requests show their submitting and last revising agents.
- A reusable `tests/e2e` suite drives team journeys through the compiled
  CLI, web pages and MCP. Its nine journeys reach M8's team workflows,
  including refused operations; contract details stay in integration
  scenarios. A functional MCP scenario checks `claimable` task filtering.
- A new `tests/deploy` suite qualifies release files and native and Docker
  installations, running the same journeys after each install.
- The CLI's `request list` shows `Outdated`, and an outdated approval
  refusal names the steps to reconcile, revise and approve the request.
  Revision results expose `was_outdated` through MCP, the Web API and the
  CLI, reporting the request's state before revision.
- A `tests/skill` suite runs 13 M8 and earlier scenarios in real agent
  harnesses. It records skill-loading evidence and sanitized transcripts,
  checks end state and tool lists, preserves explicit rerun results, and
  requires the person's verdict.
- Testing rules keep deterministic milestone acceptance checks automated
  and require a full deployment-suite pass before deployment changes land.

## 0.1.2

### Added

- Native deployment on x86_64 Debian 12 and Ubuntu 24.04 with systemd and
  nginx, plus Docker Compose deployment with a non-root `lorebayd` image.
- Shared nginx HTTPS policy, per-caller rate limits, streamed uploads and
  downloads, and validated certificate replacement for both methods.
- `lorebayd readiness`, an HTTP probe used by the image's health check.
- Native x86_64 and aarch64 image smokes in pull request and release CI.
  Releases push both smoked images by digest before tagging their combined
  manifest, and attest the manifest, archives and deployment bundle.

### Fixed

- New data directories and SQLite database files are private to the server
  user, with modes `0700` and `0600` respectively.

## 0.1.1

The vocabulary release: task uploads are files, task entries are
checkpoints, Knowledge changes awaiting a decision are requests, people
in a space are space members, credentials are sign-ins, data belongs to a
tenant, and the skill states agent practices.

### Breaking

- **Clean break:** no old name keeps working. Old tools, commands,
  routes, fields, error codes, configuration keys, environment variables
  and key prefixes have no aliases. A data directory written by `v0.1.0`
  or earlier is refused at startup: delete it and start with an empty one;
  the users file recreates the accounts. No data is converted or moved.
- **MCP tools and fields:** `create_artifact`, `complete_artifact` and
  `get_artifact_url` become `create_file`, `complete_file` and
  `get_file_url`; `get_task` returns `files` instead of `artifacts`.
  `add_note` becomes `add_checkpoint`. `create_proposal`, `list_proposals`
  and `get_proposal` become `create_request`, `list_requests` and
  `get_request`. Requests use `action` instead of `kind`, `requested_by`
  instead of `proposed_by`, `requester_agent_name` instead of
  `proposer_agent_name`, and `changed_since_requested` instead of
  `changed_since_proposed`.
- **CLI commands and output:** `artifact`, `note`, `proposal` and
  `participant` become `file`, `checkpoint`, `request` and `member`.
  Task output says Files, the Scratchpad holds checkpoints, and credentials
  are sign-ins.
- **Web API routes, fields and error codes:** `/artifacts`, `/notes`,
  `/proposals` and `/participants` become `/files`, `/checkpoints`,
  `/requests` and `/members` under their existing space or task paths.
  `/api/v1/login-session` and its refresh route become `/api/v1/sign-in`
  and `/api/v1/sign-in/refresh`. Task and request fields follow MCP's
  names; Knowledge history uses `request_key` instead of `proposal_key`.
  `artifact.*`, `note.not_found`, `proposal.*` and `participant.*` become
  `agent_file.*`, `checkpoint.not_found`, `person_request.*` and
  `space_member.*`. Transport `file.*` and `request.*` errors keep their
  names.
- **Configuration and environment:** `artifact_cleanup_interval_seconds`
  and `abandoned_artifact_lifetime_seconds` become
  `file_cleanup_interval_seconds` and `abandoned_file_lifetime_seconds`;
  `LOREBAYD__ARTIFACT_CLEANUP_INTERVAL_SECONDS` and
  `LOREBAYD__ABANDONED_ARTIFACT_LIFETIME_SECONDS` become
  `LOREBAYD__FILE_CLEANUP_INTERVAL_SECONDS` and
  `LOREBAYD__ABANDONED_FILE_LIFETIME_SECONDS`.
- **Keys and stored content:** `FL-` and `RQ-` replace `AR-` and `PP-`, in
  keys and references. Uploaded content lives in
  `<data_directory>/agent_files/` instead of `artifacts/`.
- **Skill:** the contract is agent practices, with practices 1 through 13;
  its description opens "How to record your work for the person in
  Lorebay". Tool names, fields, commands, keys and references follow the
  renames above. Install the skill and CLI at the server's release.
- **Library public API and schema:** `AgentFile`, `Checkpoint` and
  `PersonRequest` replace the upload, task-entry and Knowledge-change
  types and their operations; fields and error codes follow the names
  above. Products set `Caller.tenant_id`, and space-role implementations
  read `MembershipPage.tenant_id`, instead of `owner_id`. The library's
  created migration is rewritten with `space_agent_file`,
  `space_checkpoint`, `space_person_request` and `space.tenant_id`; a
  product that applied it must recreate its database. Lorebay's created
  migration uses `member`, `agent_sign_in` and `person_sign_in`. The four
  library crates keep version `0.1.0`; this release has no `lib-v` tag.

### Fixed

- Upgrade `url` to 2.5.9 for
  [RUSTSEC-2026-0592](https://rustsec.org/advisories/RUSTSEC-2026-0592.html),
  with its required `form_urlencoded` 1.2.3 dependency.

## 0.1.0

The first release of Lorebay: `lorebayd` for Linux on x86_64 and aarch64,
the `lorebay` CLI, and the skill for Claude Code and Codex. A data
directory made by a build before this release is refused at startup.
