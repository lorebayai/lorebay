## Library architecture docs conventions

- Architecture docs state how the library is built: its crates, its public
  API, the interfaces a product implements, transactions, storage and the
  tables. A doc about tables is at the Implementation level; the others are
  at the System level.
- Behavior a caller observes through the public API or the MCP tools
  belongs in a contract doc. An architecture doc keeps to how the behavior
  is achieved.
