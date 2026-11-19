# lorebay-lib-core

Core operations, authorization rules, and interfaces for shared spaces and their content.

## Usage

In an external product's Cargo manifest, pin every library crate you use to
the same git commit and enable the database feature you need. For Postgres:

```toml
[dependencies]
lorebay-lib-core = { git = "https://github.com/lorebayai/lorebay", rev = "<commit>", features = ["postgres"] }
lorebay-lib-mcp = { git = "https://github.com/lorebayai/lorebay", rev = "<commit>", features = ["postgres"] }
lorebay-lib-migration = { git = "https://github.com/lorebayai/lorebay", rev = "<commit>", features = ["postgres"] }
```

Replace `<commit>` with a full commit SHA. Use `sqlite` for SQLite. Build with
Rust 1.100 or later. Use the releases of the public dependencies that the
library architecture names.

Implement the five interfaces and construct a `LibraryState`. Apply the library
`Migrator` from `lorebay-lib-migration` before running any operation.
Product transactions own commit and rollback.

Run `moon run lib/core:format lib/core:lint lib/core:build` from the workspace root.
Run the Postgres scenarios with `moon run lib/core:integration`. They need
Postgres on `127.0.0.1:55432`; without it the tests fail and print the
command that starts it.
Run `moon run lib/migration:generate-entities` to regenerate the core entities
from a disposable database.

## Docs

Start at the [library docs overview](../docs/overview.md).
