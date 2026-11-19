# Lorebay library

The library holds the features every product shares: spaces, tasks, notes,
artifacts, Knowledge, proposals and the MCP tools. Lorebay is the first
product built on it, and a project in another repository depends on the
crates it needs.

## Usage

| Crate | Holds | Depend on it when |
| --- | --- | --- |
| `lorebay-lib-types` | The secret type, SHA-256 hashing, the cursor codec and time conversion; no database, MCP or async dependency | You need only these, as a command-line client that keeps a token secret does |
| `lorebay-lib-core` | The operations, their rules, the interfaces a product implements and the data layer | You build a product that serves spaces and their content |
| `lorebay-lib-migration` | The library's schema migrations and its `Migrator` | You depend on core: the product applies the library's schema |
| `lorebay-lib-mcp` | The MCP tools and the router a product mounts | Your product also serves agents over MCP; a product without MCP depends on core and migration only |

Core brings in types. Pin every library crate you use to the same full
commit SHA with `rev`, and enable the database feature you need on the
crates that select a driver. For Postgres, with all four:

```toml
[dependencies]
lorebay-lib-core = { git = "https://github.com/lorebayai/lorebay", rev = "<commit>", features = ["postgres"] }
lorebay-lib-mcp = { git = "https://github.com/lorebayai/lorebay", rev = "<commit>", features = ["postgres"] }
lorebay-lib-migration = { git = "https://github.com/lorebayai/lorebay", rev = "<commit>", features = ["postgres"] }
lorebay-lib-types = { git = "https://github.com/lorebayai/lorebay", rev = "<commit>" }
```

Use `sqlite` for SQLite. Build with Rust 1.100 or later, the release this
repository pins. Use the releases of the public dependencies that the
library architecture names.

Implement the five interfaces and construct a `LibraryState`. Apply the
library's `Migrator` before any product migration and before running any
operation. Product transactions own commit and rollback.

The four crates share one version, and [CHANGELOG.md](CHANGELOG.md) lists
what changed. Once a release is tagged `lib-v<version>`, pin that tag with
`tag` instead of `rev`.

Run `moon run lib:boundary lib:package` from the workspace root to check
that the library depends on nothing outside `lib/` and that each crate
builds from its own package.

## Docs

Start at the [library docs overview](docs/overview.md).
