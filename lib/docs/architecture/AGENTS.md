# Library architecture docs

Rules for the docs in `lib/docs/architecture/`.

- Architecture docs state how the library is built: its crates, its public
  API, the interfaces a product implements, transactions, storage and the
  tables. A doc about tables is at the Implementation level; the others are
  at the System level.
- Each technology choice carries the reason for it. A rejected alternative
  stays only as the trade-off a reader needs to understand the choice.
- Behavior a caller observes through the public API or the MCP tools
  belongs in a contract doc. An architecture doc keeps to how the behavior
  is achieved.
