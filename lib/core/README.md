# lorebay-lib-core

Core operations, authorization rules, and interfaces for shared spaces and their content.

## Usage

Run `moon run lib/core:format lib/core:lint lib/core:build` from the workspace root.
Run the Postgres scenarios with `moon run lib/core:integration`. They need
Postgres on `127.0.0.1:55432`; without it the tests fail and print the
command that starts it.
Run `moon run lib/migration:generate-entities` to regenerate the core entities
from a disposable database.

## Docs

Start at the [library docs overview](../docs/overview.md).
