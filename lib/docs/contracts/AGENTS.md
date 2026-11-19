## Library contract docs conventions

- Contract docs are at the Contract level: they state what a product
  observes through the library's public API, and what agents observe
  through the MCP tools.
- Who may do what lives only in the permission matrix of
  [access-control.md](access-control.md#4-permissions). Other contracts link
  to it.
- A rule that only agents keep lives only in
  [agent-rules.md](agent-rules.md). Its backstop states what the library
  does when an agent breaks the rule, and links to the library contract
  that enforces it.
- Every MCP tool maps to one library operation. A change to a tool updates
  [mcp-tools.md](mcp-tools.md) and the operation's row in the permission
  matrix together.
