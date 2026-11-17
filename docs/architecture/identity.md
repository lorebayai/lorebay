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
  file named in its configuration. It holds only usernames and password
  hashes, never plaintext passwords, so reading the file reveals no
  password. `lorebayd` provides a command that produces the hash of a
  password.
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
- **Sign-in is rate limited by address.** Too many attempts from one address
  in a short time are refused, which slows password guessing.
- **The `lorebay` CLI signs in through the browser.** The CLI uses a client
  built into `lorebayd`. It starts an authorization code flow with PKCE and a
  redirect to a port on the same computer; the person signs in on the page
  and confirms, and the CLI receives a login session for that person. The CLI
  never sees the password. What the terminal shows is in
  [CLI](../contracts/cli.md#4-sign-in-and-credentials).
- **Device sign-in works when the browser is on another computer.** The
  CLI's second sign-in method is the OAuth 2.0 Device Authorization Grant
  of [RFC 8628](https://datatracker.ietf.org/doc/html/rfc8628). It asks for a
  device sign-in request and shows a verification address, a short code and
  a link carrying that code. A person opens the address in any browser,
  signs in and confirms the code; the CLI waits for the decision by polling
  the token endpoint. Only the built-in CLI client may use this grant. Both
  CLI sign-in methods issue a person's login session and never create an
  agent authorization.
- **A device sign-in request expires soon and gives a login session once.**
  A request may be approved or declined only once. Approval binds it to the
  person who approved; exchanging it issues that person's login session and
  consumes the request in one transaction. A crash leaves either an approved
  request that can still be exchanged or an issued session with a consumed
  request. A declined, expired or consumed request cannot issue a session.
- **Device code issuance and short-code guesses are limited.** Asking for a
  device sign-in request needs no credential and is rate limited by address,
  as sign-in and client registration are. Attempts to read or decide a
  request using a wrong short code are limited too, so the code cannot be
  guessed through the verification page.
- **The verification page asks the person to confirm only a sign-in they
  started.** It says that this signs the `lorebay` CLI in on another computer,
  shows when the request was made and asks the person to check the code,
  including when the link carried it. Like the authorization page, it signs
  the person in, reads the request and submits the decision through Web API
  operations. It keeps its own login session only in memory and logs it out
  when finished; that session is separate from the one issued to the CLI.
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
  Registration is rate limited by address, as sign-in is, so open
  registration cannot be used to fill the database with clients.
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
- **An expired authorization is removed by a background job.** Its agent
  leaves the person's list of agents, and its name can be used again.

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
  replaces the refresh token and remembers the hash it replaced, overwriting
  the previously remembered hash. Presenting that token again deletes the
  login session or agent authorization, so all of its tokens stop working.
  The person signs in or authorizes the agent again. There is no grace period
  for a retry or concurrent refresh. A token replaced two or more refreshes
  ago is unknown and ends nothing. Revocation deletes both hashes with the
  record.

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
- [RFC 8628](https://datatracker.ietf.org/doc/html/rfc8628): OAuth 2.0 Device
  Authorization Grant, including polling, code guessing and remote phishing.
