# System Architecture

- **Status:** Proposed
- **Level:** System
- **Parent:** [Lorebay](../product/lorebay.md)
- **Related:** [Storage](storage.md), [Identity](identity.md),
  [Web API](../contracts/web-api.md),
  [Entry points](../../lib/docs/architecture/entry-points.md)

> Lorebay is one `lorebayd` process, holding the library and Lorebay's own
> entry points, identity and jobs.

## 1. Purpose

A team should get Lorebay running by starting one executable, with no other
service beside it. `lorebayd` holds the library, Lorebay's implementations
of the library's interfaces, the Web API, the web pages and the background
jobs.

## 2. Scope

This doc covers the process, its entry points and its background jobs. How
every entry point reaches the same library operations is in
[Entry points](../../lib/docs/architecture/entry-points.md); storage is in
[Storage](storage.md), and credentials and sign-in are in
[Identity](identity.md).

It does not cover running several `lorebayd` processes at once. A deployment
runs exactly one.

## 3. Components

```mermaid
flowchart TD
    human["Person"]
    program["External program"]
    agent["Agent"]
    subgraph lorebayd["lorebayd, one process"]
        web["Web API and pages"]
        oauth["OAuth endpoints"]
        impls["Interface implementations"]
        jobs["Background jobs"]
        library["Library, with its MCP layer"]
    end
    sqlite["SQLite"]
    store["File content<br/>local directory or S3"]
    human -- "login session" --> web
    program -- "agent authorization" --> web
    agent -- "agent authorization" --> library
    agent -- "authorize" --> oauth
    web --> library
    jobs --> library
    library --> impls
    impls --> sqlite
    impls --> store
```

`v0.1.0` keeps file content in a local directory; the S3 backend is
designed but not built ([Storage](storage.md)).

| Caller | Entry point | Credential |
| --- | --- | --- |
| Person, in a browser | Web API and web pages | Login session |
| Person, in a terminal | Web API, through the [`lorebay` CLI](../contracts/cli.md) | Login session |
| External program | Web API | Agent authorization |
| Agent | MCP | Agent authorization |

## 4. Entry points

- **One process.** Deploying one `lorebayd` gives the data, the APIs and the
  web pages.
- **The Web API is Lorebay's mapping of the library's operations**, beside
  Lorebay's own operations for sign-in, agents and participants; see
  [Web API](../contracts/web-api.md).
- **The MCP layer is the library's.** `lorebayd` mounts its router and
  answers who is calling through Lorebay's credentials; see
  [Identity](identity.md#4-credentials).
- **File content bypasses MCP and `/api/v1`.** With a local directory,
  `lorebayd` serves the file URLs itself; with S3, once
  [built](storage.md), content goes straight to S3 and never passes through
  `lorebayd`. See [Storage](storage.md#4-file-storage).

## 5. Technology

- **The backend is Rust**, compiled to one executable with the web pages'
  static files embedded in it.

## 6. Background jobs

Background jobs run inside `lorebayd`. Each job must be safe to run again.
They include:

- the library's abandoned-registration and orphaned-file rounds; see
  [Entry points](../../lib/docs/architecture/entry-points.md#4-entry-points);
- removing old temporary uploads and old stored content with out-of-format
  names in the local directory; see [Storage](storage.md#4-file-storage);
- removing expired authorization codes, device sign-in requests, login
  sessions and agent authorizations, and replaced refresh tokens past their
  expiry;
- removing registered clients that were never authorized, once they reach
  a configurable age; see [Identity](identity.md#6-agent-authorization).
