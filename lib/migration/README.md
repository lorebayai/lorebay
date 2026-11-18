# lorebay-lib-migration

The library's schema migrations and migration history.

## Usage

Enable the `sqlite` or `postgres` feature for the database your product uses.
Apply the public `Migrator` before applying any product migrations.

Run `moon run lib/migration:generate-entities` from the workspace root
to regenerate the core entities from a disposable SQLite database.

## Docs

Start at the [library docs overview](../docs/overview.md).
