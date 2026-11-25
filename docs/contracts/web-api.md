# Web API

- **Status:** Proposed
- **Level:** Contract
- **Parent:** [System architecture](../architecture/system.md)
- **Related:** [Access control](access-control.md),
  [Identity](../architecture/identity.md),
  [Library access control](../../lib/docs/contracts/access-control.md)

> The Web API is Lorebay's HTTP mapping of the library's operations, beside
> Lorebay's own operations, under `/api/v1`.

## 1. Purpose

The web pages, the `lorebay` CLI and external programs reach a space's
content through the library's operations, and Lorebay's own data through
Lorebay's operations. The Web API exposes both over HTTP and JSON. Agents
in a harness reach the library through its
[MCP tools](../../lib/docs/contracts/mcp-tools.md) instead.

## 2. Scope

This doc covers the Web API's paths, credentials and conventions. Which
operations exist, and who may call each, is in two tables: the library's
[permission matrix](../../lib/docs/contracts/access-control.md#4-permissions)
and the rows of Lorebay's own operations in
[Access control](access-control.md#4-permissions).

It does not cover:

- an OpenAPI document or a compatibility promise for API versions, which
  come with personal access tokens for CI.

## 3. Paths

- **Web API paths are under `/api/v1`.** A call on anything inside a space names
  the space in the path; `{space}` is the space's key, such as `SPACE-1`.
- **Calls that write into a task also name the task** in the path, by its
  task ID and never by key; see
  [Keys and references](../../lib/docs/contracts/keys-and-references.md#4-task-ids).
- **Other paths accept keys**, such as `/api/v1/spaces/SPACE-1/tasks/TK-12`.

```text
GET  /api/v1/spaces/{space}/tasks
POST /api/v1/spaces/{space}/tasks/{task_id}/checkpoints
POST /api/v1/spaces/{space}/requests/{key}/approve
```

- **OAuth endpoints are outside `/api/v1`**, at the standard locations:
  metadata, registration, authorize, device authorization and token. The
  authorization server metadata names the device authorization endpoint and
  its grant; device sign-in follows
  [Identity](../architecture/identity.md#5-sign-in).
- **File content URLs are outside `/api/v1`.** They are short-lived URLs
  that point at `lorebayd` with a local directory, and at S3 with S3, a
  backend not yet built ([Storage](../architecture/storage.md)).
- **Health checks are outside `/api/v1`** and need no credential. `/healthz`
  answers while the process runs. `/readyz` answers once startup has
  finished and until shutdown begins, so a proxy in front of `lorebayd`
  knows when to send it calls.

## 4. Conventions

- **Credentials.** Every call carries an access token in its `Authorization`
  header: a sign-in's for a person, an agent sign-in's for an
  agent or external program. See
  [Identity](../architecture/identity.md#4-credentials).
- **Sign-in, device authorization and client registration are rate limited
  by address.** A call over the limit is refused, and the caller may try
  again after a wait. See
  Identity, on [sign-in](../architecture/identity.md#5-sign-in) and on
  [registration](../architecture/identity.md#6-agent-sign-in).
- **Every list is paginated** with a cursor, a default page size and a fixed
  order, as the library's
  [lists](../../lib/docs/contracts/mcp-tools.md#4-tools) are.
- **Library errors keep their codes.** A library operation's refusal carries the
  library's error code, such as `task.not_found`, with an HTTP status Lorebay
  chooses in one place; `task.state_invalid`, `task.held`, `task.not_holder`,
  `task.has_work`, `task.result_changed`, `agent_file.not_uploaded`,
  `agent_file.refused`, `agent_file.too_large`, `space.number_taken`,
  `knowledge.name_taken`, `knowledge.version_changed`, `person_request.decided`,
  `person_request.outdated` and `person_request.revision_changed` answer 409.
- **Writes don't check references**; see [Keys and
  references](../../lib/docs/contracts/keys-and-references.md#5-references).
- **Only the holder of a `working` or `waiting` task writes checkpoints and file
  registrations**; see [Tasks](../../lib/docs/contracts/tasks.md#3-tasks).

## 5. Operations

- **Task fields are `state` and `state_reason`** in requests and responses.
  Requests use `status`.
- **Planning uses task creation with `planned=true`**, for a person or agent.
  Only agents create a started task.
- **Canceling changes state to `canceled` with `state_reason`**, a required
  one-line reason. People cancel their own tasks; owners and admins may cancel
  anyone's. There is no close operation.

- **Task reads and changes return `result_version`** beside `result`. A PATCH
  carrying `result` supplies the version it replaces in `result_version`,
  omitted or null for the first write. The result rules are in
  [Tasks](../../lib/docs/contracts/tasks.md#33-fields).
- **Result history** is `GET /api/v1/spaces/{space}/tasks/{key}/results`,
  paged by `page_size` and `cursor`, newest version first. Each row has
  `version`, `result`, `person`, `agent_name` and `created_at`. Task reads
  accept `include_results=true` to include that page under `results`, or
  `include_scratchpad=true` for checkpoints, with the same pagination
  parameters; the two flags cannot both be true.

- **Listing users** exists for adding space members. Everyone signed in may
  call it and sees every username.
- **Revising a request** uses
  `PATCH /api/v1/spaces/{space}/requests/{key}` with optional `name`, `summary`
  and `content`; `{}` refreshes its snapshot. Reads and lists include
  `revision`, `outdated` while pending, `requester_agent_name`,
  `reviser_agent_name`, `revised_by` and `revised_at`. Request reads also show
  `base_version` and `base_name`. Both decision POST bodies require `revision`;
  rejection also requires `reason`. See
  [Knowledge](../../lib/docs/contracts/knowledge.md#4-requests).
- **Direct Knowledge writes name the replaced version.** The PATCH body
  requires `version`, DELETE requires the `version` query parameter, and the
  rollback POST body requires `replaced_version` beside the historical source
  `version`. See
  [Knowledge](../../lib/docs/contracts/knowledge.md#3-knowledge-entries).
- **Withdrawing a request** has no operation of its own: a person rejects
  their own party's request; see
  [Knowledge](../../lib/docs/contracts/knowledge.md#4-requests).
