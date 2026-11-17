# Web API

- **Status:** Proposed
- **Level:** Contract
- **Parent:** [System architecture](../architecture/system.md)
- **Related:** [Access control](access-control.md), [MCP tools](mcp-tools.md),
  [Identity](../architecture/identity.md)

> The Web API is the full set of operations, under `/api/v1`. The MCP tools
> cover only the part agents use.

## 1. Purpose

The web pages, external programs and the MCP layer all use the same
operations. The Web API exposes all of them over HTTP and JSON.

## 2. Scope

This doc covers the Web API's paths, credentials and conventions. Which
operations exist, and who may call each, is in
[Access control](access-control.md#4-permissions).

It does not cover:

- an OpenAPI document or a compatibility promise for API versions, which
  come with personal access tokens for CI.

## 3. Paths

- **Web API paths are under `/api/v1`.** A call on anything inside a space names
  the space in the path; `{space}` is the space's key, such as `SPACE-1`.
- **Calls that write into a task also name the task** in the path, by its
  task ID and never by key; see
  [Keys and references](keys-and-references.md#4-task-ids).
- **Other paths accept keys**, such as `/api/v1/spaces/SPACE-1/tasks/TK-12`.

```text
GET  /api/v1/spaces/{space}/tasks
POST /api/v1/spaces/{space}/tasks/{task_id}/notes
POST /api/v1/spaces/{space}/proposals/{key}/approve
```

- **OAuth endpoints are outside `/api/v1`**, at the standard locations:
  metadata, registration, authorize, device authorization and token. The
  authorization server metadata names the device authorization endpoint and
  its grant; device sign-in follows
  [Identity](../architecture/identity.md#5-sign-in).
- **Artifact file URLs are outside `/api/v1`.** They are short-lived URLs
  that point at `lorebayd` with a local directory, and at S3 with S3.
- **Health checks are outside `/api/v1`** and need no credential. `/healthz`
  answers while the process runs. `/readyz` answers once startup has
  finished and until shutdown begins, so a proxy in front of `lorebayd`
  knows when to send it calls.

## 4. Conventions

- **Credentials.** Every call carries an access token in its `Authorization`
  header: a login session's for a person, an agent authorization's for an
  agent or external program. See
  [Identity](../architecture/identity.md#4-credentials).
- **Sign-in, device authorization and client registration are rate limited
  by address.** A call over the limit is refused, and the caller may try
  again after a wait. See
  Identity, on [sign-in](../architecture/identity.md#5-sign-in) and on
  [registration](../architecture/identity.md#6-agent-authorization).
- **Every list is paginated** with a cursor, a default page size and a fixed
  order.
- **Writes don't check references**; see
  [Keys and references](keys-and-references.md#5-references).
- **Ended tasks reject notes and artifact registrations**; see
  [Tasks](tasks.md#3-tasks).

## 5. Operations

- **Listing users** exists for adding participants. Everyone signed in may
  call it and sees every username.
- **A contributor's rollback is a proposal** whose content and summary are
  the chosen old version.
- **Withdrawing a proposal** has no operation of its own: a person rejects
  their own party's proposal; see [Knowledge](knowledge.md#4-proposals).
