## Architecture docs conventions

- Architecture docs state how `lorebayd` is built: the process, storage,
  identity and the tables. [data-model.md](data-model.md) is at the
  Implementation level; the others are at the System level.
- Behavior a caller observes belongs in [contracts](../contracts/). An
  architecture doc links to the contract and keeps to how the behavior is
  achieved.
- A table or column that a contract relies on is listed in
  [data-model.md](data-model.md) in the same change.
