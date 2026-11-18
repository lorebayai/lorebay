# Docs Overview

Lorebay's designs: what one deployment carries, how `lorebayd` is built,
and the contracts its users and programs rely on. Terms follow the
[glossary](../CONTEXT.md). The library Lorebay builds on, with its spaces,
tasks, Knowledge and MCP tools, has its own design, from the
[library overview](../lib/docs/overview.md).

## Product

- [Lorebay](product/lorebay.md): what a deployment carries, and who may
  create a space.

## Architecture

- [System architecture](architecture/system.md): the single `lorebayd`
  process, its entry points and background jobs.
- [Storage](architecture/storage.md): the backends Lorebay runs, its data
  directory, file serving and backups.
- [Identity](architecture/identity.md): accounts, sign-in, agent
  authorization and tokens.
- [Data model](architecture/data-model.md): Lorebay's eight tables, their
  columns, constraints and rules, and the SQLite DDL.

## Contracts

- [Access control](contracts/access-control.md): participants, and the
  permissions of Lorebay's own operations.
- [Web API](contracts/web-api.md): paths, credentials and conventions of
  `/api/v1`.
- [CLI](contracts/cli.md): the `lorebay` commands, browser and device sign-in,
  credentials, output and exit codes.
