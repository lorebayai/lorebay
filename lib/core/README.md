# lorebay-lib-core

Core operations, authorization rules, and interfaces for shared spaces and their content.

## Usage

Enable the `sqlite` or `postgres` feature for the database your product supplies.
Implement the five interfaces and construct a `LibraryState`. Apply the library
`Migrator` from `lorebay-lib-migration` before running any operation.
Product transactions own commit and rollback.

Run `moon run lib/core:format lib/core:lint lib/core:build` from the workspace root.
Run the Postgres scenarios with `moon run lib/core:integration`. They use
`postgres:18.7@sha256:2ea8630df176ad090ec92136dee850397c5af8781e4bcf2e4162593691c61bbc` on `127.0.0.1:55432`.
Run `moon run lib/migration:generate-entities` to regenerate the core entities
from a disposable database.

## Docs

Start at the [library docs overview](../docs/overview.md).
