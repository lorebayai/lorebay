# Architecture docs

Rules for the docs in `docs/architecture/`.

- Architecture docs state how `lorebayd` is built: the process, its layers,
  storage, identity and the tables. [data-model.md](data-model.md) is at the
  Implementation level; the others are at the System level.
- Each technology choice carries the reason for it. A rejected alternative
  stays only as the trade-off a reader needs to understand the choice.
- Behavior a caller observes belongs in [contracts](../contracts/). An
  architecture doc links to the contract and keeps to how the behavior is
  achieved.
- A table or column that a contract relies on is listed in
  [data-model.md](data-model.md) in the same change.
