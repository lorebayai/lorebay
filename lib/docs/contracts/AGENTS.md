## Library contract docs conventions

- Contract docs are at the Contract level: they state what a product
  observes through the library's public API, and what agents observe
  through the MCP tools.
- Who may do what lives only in the permission matrix of
  [access-control.md](access-control.md#4-permissions). Other contracts link
  to it.
- Every MCP tool maps to one library operation. A change to a tool updates
  [mcp-tools.md](mcp-tools.md) and the operation's row in the permission
  matrix together.
