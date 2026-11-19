## Contract docs conventions

- Contract docs are at the Contract level: they state what people on the web
  pages or in the `lorebay` CLI, programs on the Web API and agents over MCP
  can observe. What the library's operations and MCP tools do is in the
  library's contracts; Lorebay's contracts link to them.
- Who may do what lives only in two permission tables: the library's
  [permission matrix](../../lib/docs/contracts/access-control.md#4-permissions)
  for its operations, and the rows of Lorebay's own operations in
  [access-control.md](access-control.md#4-permissions). Other contracts link
  to them.
- A rule that only the skill enforces lives only in
  [agent-skill.md](agent-skill.md). Its backstop states what the library
  does when an agent breaks the rule, and links to the library contract
  that enforces it.
- Every Web API operation is a library operation or one of Lorebay's own.
  A change to one of Lorebay's own operations updates
  [web-api.md](web-api.md) and its row in
  [access-control.md](access-control.md#4-permissions) together.
