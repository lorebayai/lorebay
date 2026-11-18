# Contract docs

Rules for the docs in `docs/contracts/`.

- Contract docs are at the Contract level: they state what people on the web
  pages or in the `lorebay` CLI, programs on the Web API and agents over MCP
  can observe. What the library's operations and MCP tools do is in the
  library's contracts; Lorebay's contracts link to them.
- Who may do what lives only in two permission tables: the library's
  [permission matrix](../../lib/docs/contracts/access-control.md#4-permissions)
  for its operations, and the rows of Lorebay's own operations in
  [access-control.md](access-control.md#4-permissions). Other contracts link
  to them.
- Every Web API operation is a library operation or one of Lorebay's own.
  A change to one of Lorebay's own operations updates
  [web-api.md](web-api.md) and its row in
  [access-control.md](access-control.md#4-permissions) together.
- Write status values as inline code, exactly as stored: `in progress` and
  `ended` for tasks; `pending`, `approved` and `rejected` for proposals.
- Name objects in examples by key, such as `TK-12` or `#KN-1`.
