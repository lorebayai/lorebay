Follow these playbook fragments:

- @../playbook/languages/rust.md
- @../playbook/topics/seaorm.md
- @../playbook/topics/testing.md

## lorebayd conventions

### Crate

- `lorebayd` is a library crate with a thin `src/main.rs`, so the start
  helper of its integration tests builds it through the same entry point as
  `main` ([testing.md](../playbook/topics/testing.md#black-box)).
- The library exposes only what `main.rs` and the start helper use: the
  `Config` type and the entry point `run`. Every other item is `pub(crate)`.
- The source follows this layout, with only the folders the code needs:

  ```text
  src/
  ├── main.rs       Loads Config, starts logging, binds the listener, calls run
  ├── lib.rs        Declares modules; re-exports Config and run
  ├── config.rs     Config
  ├── server.rs     run and the server state
  ├── api/          The Web API under /api/v1
  ├── mcp/          The MCP layer
  ├── oauth/        Sign-in, consent and the OAuth endpoints
  ├── pages/        The embedded web pages
  ├── files/        Artifact file URLs served with the local directory backend
  ├── domain/       The API operations, one module per area
  ├── worker/       Background jobs, one file per job
  ├── storage/      The file storage layer and its backends
  ├── migration/    Schema migrations (seaorm.md)
  ├── entity/       Generated SeaORM entities (seaorm.md)
  └── repository/   Repositories (seaorm.md)
  ```

### Layers

The layers are those of
[System architecture](../docs/architecture/system.md#3-components):
transports and workers call the domain, and the domain calls the database
and file storage layers.

| Layer | Lives in | Owns |
| --- | --- | --- |
| Transport | `src/api/`, `src/mcp/`, `src/oauth/`, `src/pages/`, `src/files/` | Converting a protocol's requests into domain inputs, and domain results back |
| Worker | `src/worker/` | The [background jobs](../docs/architecture/system.md#6-background-jobs), calling domain functions as a transport does per request |
| Domain | `src/domain/` | The API operations: permission checks, data rules and transactions |
| Data | `src/repository/`, `src/entity/`, `src/migration/` | The database layer |
| Storage | `src/storage/` | The [file storage layer](../docs/architecture/storage.md#3-layers) |

- A **domain function** performs one API operation, such as appending a
  note. A Web API operation and the MCP tool mapped to it call the same
  domain function ([MCP tools](../docs/contracts/mcp-tools.md#5-the-mcp-layer)).
- Domain functions must not use transport types: no axum, HTTP or MCP
  types, and no request or response structs. They take typed inputs and the
  verified caller, a person and, for an agent, its authorization, never a
  raw token.
- Domain functions check permissions, by the
  [permission matrix](../docs/contracts/access-control.md#4-permissions) and
  in the order of
  [the space and task checks](../docs/contracts/access-control.md#6-space-and-task-checks).
  Transports never check permissions.
- A domain function that writes opens one transaction, passes it to every
  repository and helper it calls, and commits once. A helper that joins a
  caller's transaction takes it as a parameter and never commits or rolls
  back.

### Transports

- Web API paths follow [Web API](../docs/contracts/web-api.md#3-paths).
  `src/api/v1/<resource>.rs` holds one resource's handlers and its request
  and response types, such as `src/api/v1/tasks.rs`.
- Request and response types are data structs per
  [rust.md](../playbook/languages/rust.md#errors-and-data-structures).
  Requests derive `Deserialize`, responses `Serialize`. They never reach the
  domain layer: a handler converts the request into the domain function's
  input, and the domain result into the response type, with `From` or
  `TryFrom`. They never derive SeaORM traits or expose entity types.
- Field rules that need no stored data, such as lengths and formats, are
  checked on the request type. Rules that need stored data belong to the
  domain function.
- A handler, or an MCP tool, extracts its inputs, calls one domain function
  and converts the result. It holds no business rule.

### Errors

- Each domain area has one error enum, per
  [rust.md](../playbook/languages/rust.md#errors-and-data-structures). Each
  transport converts it into its protocol's error in one place: an HTTP
  status and body for the Web API, a tool error for MCP.
- Database, I/O and other internal failures are logged with their cause and
  reach the caller only as an internal error.

### Configuration

- `src/config.rs` holds one `Config` struct, which `main` loads in layers:
  the defaults, then an optional config file named by a command-line flag,
  then environment variables with the `LOREBAYD__` prefix. It holds, among
  others, the users file of
  [Identity](../docs/architecture/identity.md#3-accounts) and the data
  directory of [Storage](../docs/architecture/storage.md#3-layers).
- `main` exits on a config error, naming the key, before anything starts.

### Startup and shutdown

1. `main` loads `Config`, starts logging, binds the listen address, and
   calls `run` with a shutdown future that resolves on SIGINT or SIGTERM.
2. `run` reads the users file, opens the database in the data directory and
   applies pending migrations
   ([seaorm.md](../playbook/topics/seaorm.md#migrations)).
3. `run` builds the server state and the routers, starts its workers, and
   serves until the shutdown future resolves.
4. On shutdown, `run` stops accepting connections and waits for in-flight
   requests, and the workers finish the job they hold. `run` then closes the
   database and returns.

- `run` takes the listener and the shutdown future as parameters. It never
  binds a port, reads environment variables or listens for process signals.
- The start helper calls `run` with test configuration, a listener bound to
  `127.0.0.1:0` and a shutdown future it controls.
- The server state holds the long-lived handles, such as the database
  connection, the file storage backend and the accounts from the users file.
  `run` builds it once and passes it to handlers, which pass what they need
  to domain functions. Nothing opens a connection per request.
