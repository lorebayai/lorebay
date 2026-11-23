# Glossary

Canonical terms for Lorebay's docs and code. Use them exactly, and don't
introduce synonyms. The terms Lorebay shares with the library it builds on,
such as space, task, note, file, Knowledge, proposal, agent, role and
key, are in the [library's glossary](lib/CONTEXT.md) and mean the same here.
Terms are written in lowercase in running text.

## Product

**Lorebay**: The product, and one deployment of it: a single `lorebayd`
process with its data, its web pages, its Web API and its MCP server.

**`lorebayd`**: The Lorebay server process.

**Participant**: A person in a space, with a role. An agent is in the spaces
its person is in.

**Users file**: The file of usernames and password hashes that `lorebayd`
reads at startup. It defines who can sign in.

**Account**: One username in the users file, and the person who signs in
with it.

**Login session**: One sign-in of a person, from the web pages or the
`lorebay` CLI. It is the person's credential.

**`lorebay`**: The command-line client. People use it for the operations
only people perform and to read a space's content.

**Device sign-in request**: A short-lived request to sign the `lorebay` CLI
in from a browser on another computer. A person confirms its short code
before the CLI receives that person's login session.

## Agents

**Agent authorization**: The record and credential of one agent. In
Lorebay, each agent is one OAuth authorization a person grants to a client,
such as Claude Code; external programs that call the Web API are agents
too.

**Skill**: The skill Lorebay ships, as the
[library's glossary](lib/CONTEXT.md) defines it.
