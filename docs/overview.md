# Docs Overview

Product- and system-wide designs for Lorebay: what it is, how `lorebayd` is
built, and the contracts its users, APIs and agents rely on. Terms follow the
[glossary](../CONTEXT.md).

## Product

- [Lorebay](product/lorebay.md): the problem Lorebay solves, spaces, and
  what a deployment ships.

## Architecture

- [System architecture](architecture/system.md): the single `lorebayd`
  process, its entry points, layers and background jobs.
- [Storage](architecture/storage.md): the database and file storage layers,
  their backends, transactions and search.
- [Identity](architecture/identity.md): accounts, sign-in, agent
  authorization and tokens.
- [Data model](architecture/data-model.md): the 13 tables, their columns,
  constraints and rules, and the SQLite DDL.

## Contracts

- [Access control](contracts/access-control.md): participants, roles, the
  permission of every operation, and the space and task checks.
- [Tasks](contracts/tasks.md): tasks, Scratchpads, notes and artifacts.
- [Knowledge](contracts/knowledge.md): knowledge entries, versions and
  proposals.
- [Keys and references](contracts/keys-and-references.md): keys, task IDs
  and `#` references.
- [MCP tools](contracts/mcp-tools.md): the protocol version and the 14 tools
  agents call.
- [Web API](contracts/web-api.md): paths, credentials and conventions of
  `/api/v1`.
- [Agent skill](contracts/agent-skill.md): the rules the skill gives agents,
  and what Lorebay does when they are broken.
