Follow these playbook fragments:

- @../playbook/topics/design-docs.md

## Docs conventions

- `docs/` holds Lorebay's design. The design of the library Lorebay builds
  on is in `lib/docs/`; Lorebay's docs link to it and repeat none of it.
- The glossary is [CONTEXT.md](../CONTEXT.md).
- Each topic folder's `AGENTS.md` adds the rules for that folder.
- A doc stays at one of these levels:

  | Level | Covers | Leaves out |
  | --- | --- | --- |
  | System | Goals, boundaries, components, technology choices | Wire formats, table layouts |
  | Contract | Behavior observable from outside `lorebayd`: the web pages, the Web API, MCP tools, the `lorebay` CLI | Internal structure |
  | Implementation | Tables, modules and the rules they keep | Behavior a contract owns |
