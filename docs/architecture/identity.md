# Identity

- **Status:** Proposed
- **Level:** System
- **Parent:** [System architecture](system.md)
- **Related:** [Access control](../contracts/access-control.md),
  [Web API](../contracts/web-api.md)

> There are exactly two credentials, both bearer tokens: a login session for
> a person, and an agent authorization for one of a person's agents.
>
> Every request checks the token, the account, the person's role in the space
> and the session or authorization record, so a removal or revocation takes
> effect on the next request.

## 1. Purpose

People sign in to the web pages and the `lorebay` CLI, and agents act for
them through MCP and the Web API. Lorebay needs to know, on every call, which
person is behind it and, for an agent, which authorization it uses, without
depending on an outside identity service.

## 2. Scope

This doc covers accounts, sign-in, agent authorization and tokens. What each
role may do is in [Access control](../contracts/access-control.md).

It does not cover:

- sign-in with an emailed code, or adding and removing accounts on the web;
- personal access tokens for CI or other environments without a browser;
- an owner for spaces whose owner's account was deleted;
- client registration through Client ID Metadata Documents;
- cookies: no call is authenticated by one.

## 3. Accounts

- **Accounts live in the users file.** `lorebayd` reads a YAML or JSON users
  file named in its configuration. It holds only usernames and passwords.
  People sign in with them.
- **Adding or removing a user means editing the file and restarting.**
- **Everyone in the file can sign in**, create spaces, and be added to other
  people's spaces.
- **Deleting an account removes the person from every space.** They can no
  longer sign in, and all of their agents' authorizations stop working. Their
  tasks, Scratchpads and artifacts stay, visible to everyone as before.
- **A username added back is the same person.** Their earlier tasks and
  proposals are theirs again, but they are in no space until an owner or
  admin adds them, and their agents must authorize again. Never give a
  username to a different person: they would take over the earlier person's
  tasks.

## 4. Credentials

| Credential | Represents | Used for |
| --- | --- | --- |
| Login session | A person | Web API, from the web pages and the `lorebay` CLI |
| Agent authorization | One agent of a person | MCP, Web API |

- **Both credentials are bearer tokens.** Each is an access token and a
  refresh token, and a call carries the access token in its `Authorization`
  header. The web pages, the CLI, external programs and agents then call the
  Web API the same way, and no call needs protection against cross-site
  request forgery.
- **External programs are agents.** A program that calls the Web API
  authorizes through OAuth like an agent, is recorded as one of a person's
  agents, and gets an agent's permissions. There is no third credential.
- **Operations only people perform accept only a login session**, such as
  approving and rejecting proposals, deleting tasks, notes and artifacts,
  changing Knowledge directly, rolling back, and managing participants. The
  full list is in [Access control](../contracts/access-control.md#4-permissions).

## 5. Sign-in

- **A person signs in with a username and password.** Signing in is a Web API
  operation that returns a login session. Sign-in is a separate step from
  agent authorization, so it can move to emailed codes or SSO without
  changing anything on the agent side.
- **The `lorebay` CLI signs in through the browser.** The CLI is a client
  built into `lorebayd`. It starts an authorization code flow with PKCE and a
  redirect to a port on the same computer; the person signs in on the page
  and confirms, and the CLI receives a login session for that person, not an
  agent. The CLI never sees the password.
- **Logging out deletes the login session.** Both of its tokens stop working.

## 6. Agent authorization

- **Lorebay is its own authorization server.** It uses the users file, not
  an outside identity service.
- **Clients register through Dynamic Client Registration.** Lorebay accepts
  MCP clients on several protocol versions; see
  [MCP tools](../contracts/mcp-tools.md#3-protocol-versions). Dynamic Client
  Registration is the one registration method that all of those versions
  define. Registration is open and needs no credential, as
  [RFC 7591](https://datatracker.ietf.org/doc/html/rfc7591) allows; a
  registered client can do nothing until a person authorizes it.
- **Sign-in and consent are Web API operations.** The authorization endpoint
  is a minimal page that `lorebayd` serves. The page signs the person in,
  reads the request it was opened with, shows the client and asks for
  consent, and reports the decision, all through the Web API. Richer web
  pages can replace it without changing the operations.
- **Each authorization is one agent.** Its record holds the person, the
  client and a name. The name defaults to the client name the client
  registered, which the client reports about itself and is used only for
  display. When consenting, the person may change it, such as to
  `Claude Code (home)`.
- **A person's agents have distinct names.** When the name chosen at consent
  already belongs to another of the person's agents, the person picks
  another name or replaces the old authorization. Replacing deletes the old
  record and creates the new one in one transaction when the tokens are
  issued, so the name carries over.
- **An agent follows its person.** It is a participant in every space its
  person is in, and leaves a space when its person does. Its role is derived
  from its person's; see
  [Access control](../contracts/access-control.md#3-participants-and-roles).
- **Revoking an authorization deletes its record.** People see the agents
  they authorized and can revoke any of them. A task keeps the agent name
  copied when it was created, so the name survives revocation.

## 7. Tokens

- **Access tokens are signed JWTs.** `lorebayd` signs each one with its own
  key. The token names the person and the login session or agent
  authorization it belongs to, and carries its expiry, so a forged or expired
  token is rejected before any lookup.
- **Every request still loads the record the token names**, together with the
  caller's role. A token whose login session or authorization was deleted is
  rejected, although its signature is valid and it has not expired.
- **Access tokens expire soon.** A client exchanges its refresh token for a
  new access token.
- **Refresh tokens are random strings**, stored only as hashes. Each exchange
  replaces the refresh token, so the old one stops working, and revocation
  deletes it with its record.

## 8. Authorization flow

```mermaid
sequenceDiagram
    participant P as Person
    participant H as Harness
    participant L as lorebayd
    H->>L: Call without a token
    L-->>H: 401, where to authorize
    H->>L: Register as a client
    H->>P: Open the authorization page
    P->>L: Sign in
    P->>L: Consent
    L-->>H: Authorization code
    H->>L: Exchange the code
    L-->>H: Access and refresh tokens
    H->>L: Calls with the access token
    H->>L: Refresh, when the access token expires
    L-->>H: New access and refresh tokens
```

When the person revokes the authorization, its record is deleted and the
agent's next call fails.

## 9. References

- [MCP authorization, 2026-07-28](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization):
  Dynamic Client Registration stays available for backward compatibility,
  and public clients' refresh tokens must rotate.
- [RFC 7591](https://datatracker.ietf.org/doc/html/rfc7591): OAuth 2.0
  Dynamic Client Registration.
