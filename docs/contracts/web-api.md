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
  come with personal access tokens for CI;
- protection against cross-site request forgery for calls made with the
  login cookie.

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
POST /api/v1/spaces/{space}/proposals/{id}/approve
```

- **OAuth endpoints are outside `/api/v1`**, at the standard locations:
  metadata, authorize and token.
- **Artifact file URLs are outside `/api/v1`.** They are short-lived URLs
  that point at `lorebayd` with a local directory, and at S3 with S3.

## 4. Conventions

- **Credentials.** People call with the login cookie; agents and external
  programs call with an access token. See
  [Identity](../architecture/identity.md#4-credentials).
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
