Follow these playbook fragments:

- @../../playbook/topics/design-docs.md

## Library docs conventions

- `spaces/docs/` holds the design of the library: the features every
  product shares, and the interfaces a product implements to use them.
- [overview.md](overview.md) lists every doc. Terms follow the library's
  glossary, `spaces/CONTEXT.md`.
- Nothing here links to a file outside `spaces/`, except the playbook.
- The docs describe the library for any product. A choice that only
  Lorebay makes is named as Lorebay's.
- Each topic folder's `AGENTS.md` adds the rules for that folder.
- A doc stays at one of these levels:

  | Level | Covers | Leaves out |
  | --- | --- | --- |
  | System | Goals, boundaries, components, interfaces, technology choices | Wire formats, table layouts |
  | Contract | Behavior observable through the library's public API and its MCP tools | Internal structure |
  | Implementation | Tables, modules and the rules they keep | Behavior a contract owns |
