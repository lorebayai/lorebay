Follow these playbook fragments:

- @../playbook/languages/rust.md
- @../playbook/topics/seaorm.md
- @../playbook/topics/testing.md

## lorebayd conventions

### Crate

- `lorebayd` is a library crate with a thin `src/main.rs`, so the start
  helper of its integration tests builds it through the same entry point as
  `main` ([testing.md](../playbook/topics/testing.md#black-box)).
- The server crate exposes only what `main.rs` and the start helper use: the
  `Config` type, the entry point `run`, and the function that hashes a
  password for the users file, and the HTTP readiness probe. Every other item
  is `pub(crate)`.
- `main.rs` also runs `hash-password`, using sign-in's hash function
  ([Identity](../docs/architecture/identity.md#3-accounts)), and `readiness`,
  which probes the configured HTTP `/readyz`. Neither command starts a server.
- The `migration/` crate owns Lorebay's schema migrations.
- The source follows this layout, with only the folders the code needs:

  ```text
  src/
  |-- main.rs       Loads Config, starts logging, binds the listener, calls run
  |-- lib.rs        Declares modules; re-exports the public items
  |-- config.rs     Config
  |-- server.rs     run and the server state
  |-- api/          The Web API under /api/v1, sign-in and consent included
  |-- library/      Lorebay's implementations of the library interfaces
  |-- oauth/        The OAuth endpoints and the authorization page
  |-- pages/        The embedded web pages
  |-- files/        File content URLs served with the local directory backend
  |-- domain/       Product operations, one module per area
  |-- worker/       Background jobs, one file per job
  |-- storage/      The file storage layer and its backends
  |-- entity/       Generated SeaORM entities (seaorm.md)
  `-- repository/   Repositories (seaorm.md)
  ```

### Layers

Transports and workers call Lorebay's domain functions and the library's
operations; the local-directory job calls the storage layer. The domain
calls the database layer.

| Layer | Lives in | Owns |
| --- | --- | --- |
| Transport | `src/api/`, `src/oauth/`, `src/pages/`, `src/files/` | Converting a protocol's requests into domain inputs, and domain results back |
| Worker | `src/worker/` | The [background jobs](../docs/architecture/system.md#6-background-jobs), calling domain functions, the library's cleanup rounds, or the local directory's own cleanup |
| Adapter | `src/library/` | Lorebay's implementations of the library's interfaces. They call the domain, data and storage layers; the caller resolver reads the request's credential with the transport's header parsing |
| Domain | `src/domain/` | Lorebay's own operations: permission checks, data rules and transactions |
| Data | `src/repository/`, `src/entity/`, `migration/` | The database layer |
| Storage | `src/storage/` | The [file storage layer](../lib/docs/architecture/storage.md#3-file-storage) |

- Web API operations on shared content call the same public core functions
  as MCP. The embedded MCP router comes from `lorebay-lib-mcp`.
- Domain functions must not use axum, HTTP, MCP, or transport request and
  response types. They take typed inputs and verified callers and must
  never receive raw tokens. Web API handlers convert verified callers to
  the library's person ID, optional agent name and tenant ID before
  calling core.
- Domain functions check permissions for the operations they own, using
  the library's [permission matrix](../lib/docs/contracts/access-control.md#4-permissions)
  and [Lorebay's own rows](../docs/contracts/access-control.md#4-permissions),
  in the order of the
  [space and task checks](../lib/docs/contracts/access-control.md#6-space-and-task-checks).
  Transports never check permissions.
- Product-only domain functions own their transactions. Library writes and
  composed space creation use the library's write entry with Lorebay's
  transaction adapter. Helpers use the operation's transaction.

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
- Request types check the protocol's field shape and convert inputs to
  domain types. The domain types check shared field rules. Rules that need
  stored data belong to the domain function.
- A handler extracts its inputs, calls one domain function or one library
  operation, and converts the result. It holds no business rule.

### Errors

- Each domain area has one error enum, per
  [rust.md](../playbook/languages/rust.md#errors-and-data-structures). Each
  transport converts it into its protocol's error in one place: an HTTP
  status and body for the Web API. The library's MCP layer converts its
  own.
- Database, I/O and other internal failures are logged with their cause and
  reach the caller only as an internal error.

### Configuration

- `src/config.rs` holds one `Config` struct, which `main` loads in layers:
  the defaults, then an optional config file named by a command-line flag,
  then environment variables with the `LOREBAYD__` prefix. The variables
  come from an optional env file, named by another flag, and then from the
  process environment, which wins. `Config` holds, among others, the users
  file of [Identity](../docs/architecture/identity.md#3-accounts) and the
  data directory of [Storage](../docs/architecture/storage.md#3-layers).
- `main` exits on a config error, naming the key, before anything starts.

### Startup and shutdown

1. `main` loads `Config`, starts logging, binds the listen address, and
   calls `run` with a shutdown future that resolves on SIGINT or SIGTERM.
2. `run` reads the users file, opens the database in the data directory and
   applies the library then the product migration groups
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
  connection, the file storage backend, the identity state and the embedded
  library state.
  `run` builds it once and passes it to handlers, which pass what they need
  to domain functions. Nothing opens a connection per request.
