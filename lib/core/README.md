# lorebay-lib-core

Core operations, authorization rules, interfaces, and migrations for shared spaces and their content.

## Usage

Enable the `sqlite` or `postgres` feature for the database your product supplies.
Implement the five interfaces and construct a `LibraryState`. Apply the library
migrator before running any operation. Product transactions own commit and rollback.

Run `moon run lib/core:format lib/core:lint lib/core:build` from the workspace root.
Run `moon run :generate-entities` to apply the migrations to a disposable
database and regenerate the entities.

## Docs

Start at the [library docs overview](../docs/overview.md).
