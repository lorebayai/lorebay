# Contract docs

Rules for the docs in `docs/contracts/`.

- Contract docs are at the Contract level: they state what people on the web
  pages or in the `lorebay` CLI, programs on the Web API and agents over MCP
  can observe.
- Who may do what lives only in the permission matrix of
  [access-control.md](access-control.md#4-permissions). Other contracts link
  to it.
- A rule that only the skill enforces lives only in
  [agent-skill.md](agent-skill.md). Its backstop states what Lorebay does when
  an agent breaks the rule, and links to the contract that enforces it.
- Every MCP tool maps to one Web API operation. A change to a tool updates
  [mcp-tools.md](mcp-tools.md) and the operation's row in the permission
  matrix together.
- Write status values as inline code, exactly as stored: `in progress` and
  `ended` for tasks; `pending`, `approved` and `rejected` for proposals.
- Name objects in examples by key, such as `TK-12` or `#KN-1`.
