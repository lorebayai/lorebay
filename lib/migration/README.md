# lorebay-lib-migration

The library's schema migrations and migration history.

## Usage

Enable the `sqlite` or `postgres` feature for the database your product uses.
Apply the public `Migrator` before applying any product migrations.
Enable `entity-generation` only for the disposable database helper and its binary.

Run `moon run lib/migration:format lib/migration:lint lib/migration:build`
from the workspace root. Run `moon run lib/migration:generate-entities` to
regenerate the core entities from a disposable SQLite database.

## Docs

Start at the [library docs overview](../docs/overview.md).
