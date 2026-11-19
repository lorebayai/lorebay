# lorebayd-migration

Lorebay's SQLite schema migrations and migration history.

## Usage

The server applies these migrations after the library's migrations.
Run `moon run lorebayd/migration:format lorebayd/migration:lint lorebayd/migration:build`
from the workspace root. Run `moon run lorebayd/migration:generate-entities`
to regenerate Lorebay's entities from a disposable SQLite database.

## Docs

Start at the [docs overview](../../docs/overview.md).
