# Glossary

Canonical terms for Lorebay's docs and code. Use them exactly, and don't
introduce synonyms. The terms Lorebay shares with the library it builds on,
such as space, task, checkpoint, file, Knowledge, proposal, agent, space role
and key, are in the [library's glossary](lib/CONTEXT.md) and mean the same here.
[Space members](lib/CONTEXT.md#spaces-and-roles) follow the library's definition.
Terms are written in lowercase in running text.

## Product

**Lorebay**: The product, and one deployment of it: a single `lorebayd`
process with its data, its web pages, its Web API and its MCP server. One
deployment is one tenant.

**`lorebayd`**: The Lorebay server process.

**Users file**: The file of usernames and password hashes that `lorebayd`
reads at startup. It defines who can sign in.

**Account**: One username in the users file, and the person who signs in
with it.

**Sign-in**: A person's credential, from one sign-in on the web pages; from
the `lorebay` CLI it is a **CLI sign-in**.

**`lorebay`**: The command-line client. People use it for the operations
only people perform and to read a space's content.

**Device sign-in request**: A short-lived request to sign the `lorebay` CLI
in from a browser on another computer. A person confirms its short code
before the CLI receives that person's CLI sign-in.

## Agents

**Agent sign-in**: An agent's Sign-in: the record and credential of one
agent, one OAuth authorization a person grants to a client, such as Claude
Code. Each agent has exactly one. External programs that call the Web API
are agents too.

**Skill**: The skill Lorebay ships, as the
[library's glossary](lib/CONTEXT.md) defines it.
