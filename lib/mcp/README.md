# lorebay-lib-mcp

MCP tools, the request card resource, the authorization challenge and
protected resource metadata for the library.

## Usage

Enable the same database feature as core. Mount the router returned by
`router` at the root of the product's origin. The request card uses the
configured server name. Use `router_with_display_name` when the card needs
its own product display name.

Run `moon run lib/mcp:format lib/mcp:lint lib/mcp:build lib/mcp:card` from
the workspace root. The card check uses the shared Prettier settings and
extracts the HTML's inline module into a temporary directory for strict
TypeScript and ESLint checks.

## Docs

Start at the [library docs overview](../docs/overview.md).
