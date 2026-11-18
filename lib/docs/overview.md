# Library Docs Overview

Designs for the library that holds the features every product shares:
spaces, tasks, notes, artifacts, Knowledge, proposals and the MCP tools.
Lorebay is the first product built on it. Terms follow the
[glossary](../CONTEXT.md).

## Product

- [Library](product/library.md): the problem the library solves, spaces,
  and the five kinds of data a space holds.

## Architecture

- [Library architecture](architecture/library.md): what the library holds
  and what a product holds, the crates, the public API, the five interfaces
  a product implements, transactions, settings, tables, concurrent writers
  and the Postgres tests.
- [Entry points](architecture/entry-points.md): how every entry point
  reaches the same operations, and how files travel beside them.
- [Storage](architecture/storage.md): the file storage interface, the rules
  every file follows, the data layer, transactions and search.
- [Data model](architecture/data-model.md): the seven tables, their columns,
  constraints and rules, and their DDL on SQLite.

## Contracts

- [Access control](contracts/access-control.md): roles, the permission of
  every library operation, and the space and task checks.
- [Tasks](contracts/tasks.md): tasks, Scratchpads, notes and artifacts.
- [Knowledge](contracts/knowledge.md): knowledge entries, versions and
  proposals.
- [Keys and references](contracts/keys-and-references.md): keys, task IDs
  and `#` references.
- [MCP tools](contracts/mcp-tools.md): the protocol versions and the 14 tools
  agents call.
