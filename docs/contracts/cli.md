# CLI

- **Status:** Proposed
- **Level:** Contract
- **Parent:** [System architecture](../architecture/system.md)
- **Related:** [Web API](web-api.md), [Access control](access-control.md),
  [Identity](../architecture/identity.md),
  [Keys and references](../../lib/docs/contracts/keys-and-references.md)

> The `lorebay` executable is a Web API client acting with a person's login
> session. It adds no server operation and never reads `lorebayd`'s data
> directly.
>
> Permissions stay in the Web API; the CLI never grants a permission.
>
> Results go to standard output; prompts, hints and errors go to standard
> error. Login session tokens and secret sign-in codes appear in neither
> stream.

## 1. Purpose

A person in a terminal reads a space's content and performs the operations
that need a person, through the same Web API as the pages. Where a library
contract says a person does something, Lorebay offers it in the CLI, subject
to the scope below, and on the web pages once they are built
([Lorebay](../product/lorebay.md#3-what-a-deployment-carries)).

## 2. Scope

This doc covers commands, sign-in from a terminal, stored credentials,
output, pagination, deletion confirmation and exit codes. The CLI is a
separate executable from `lorebayd`. For personal access tokens, see
[Identity](../architecture/identity.md#2-scope).

It does not cover:

- signing in with a password, reading an authorization request or submitting
  consent, or reading a device sign-in request or submitting its decision:
  these belong to the pages `lorebayd` serves;
- changing a task's title or result, or setting it back to `in progress`:
  agents do these; the CLI can close a task;
- rendering Markdown;
- updating the CLI itself.

## 3. Commands

- **Commands use `lorebay <resource> <verb>`, except `login` and `logout`.**
  Every command accepts `--output human|json`; `human` is the default.
  `lorebay` with no command, and `--help` on any command, print usage and exit
  0 without a server call. Invalid arguments print a usage error and exit 2.
- **A space is named by key and a person by username.** Examples are
  `SPACE-1` and `alice`. Objects to read can be named by key. A command that
  writes into a task takes its task ID, including closing or deleting it,
  as
  [Keys and references](../../lib/docs/contracts/keys-and-references.md#4-task-ids)
  requires. It never treats a task key as a task ID.
- **Commands send the operation's inputs and show its result.** They neither
  bypass checks nor choose a different operation based on the person's
  role. Permissions are solely in the library's
  [permission matrix](../../lib/docs/contracts/access-control.md#4-permissions)
  and Lorebay's [own rows](access-control.md#4-permissions).

In the table, `<space>` is a space key, `<task>` a task key, `<task-id>` a
task ID, `<entry>` a knowledge entry's name or key, `<version>` an integer
version returned by its history, `<file>` a file key, `<proposal>` a
proposal key, and `<note>` the note's time as the Scratchpad shows it,
which names one note; see
[Tasks](../../lib/docs/contracts/tasks.md#4-scratchpad).
An `<entry>` in the exact key form `KN-<n>` is sent as a key, and any other
value as a name; an entry whose name has the key form is reachable by its own key.
`<name>` in an agent command is the person's agent name, quoted when it
contains spaces. Optional arguments appear in brackets. List rows follow
[section 6](#6-lists); all output follows [section 5](#5-output-and-failures).

| Command after `lorebay` | Web API operation | Result in `human` output |
| --- | --- | --- |
| `login <server-url> [--no-browser]` | Start and complete CLI sign-in through OAuth; receive a login session | Sign-in completion; the browser link is on standard error |
| `login <server-url> --device` | Request device sign-in and exchange it through OAuth; receive a login session | Sign-in completion; the address, short code and link carrying it are on standard error |
| `logout` | Log out | Sign-out completion after removing local credentials |
| `agent list` | List own agents | Agent names and the other fields the operation returns |
| `agent revoke <name>` | Revoke one agent | Revocation result |
| `space list` | List own spaces | Space keys, names, summaries and the person's roles |
| `space create --name <name> --summary <summary>` | Create a space | Created space: key, name, summary and role |
| `space show <space>` | Read a space | Space key, name, summary and role |
| `space change <space> [--name <name>] [--summary <summary>]` | Change a space's name and summary | Updated space, in the same form as `space show` |
| `user list` | List the users in the users file | Usernames |
| `member list <space>` | List a space's members | Human usernames and agent names, with each agent's person, and their types and roles |
| `member add <space> <username> --role <role>` | Add a space member | Added space member |
| `member set-role <space> <username> --role <role>` | Change a space member's role | Updated space member |
| `member remove <space> <username>` | Remove a space member | Removal result |
| `task list <space> [--person <username>] [--agent <name>] [--status <status>] [--keyword <text>] [--own-open]` | List tasks | Task keys, titles, statuses, people, agent names and last activity times |
| `task show <space> <task>` | Read a task | Task fields, including its task ID, result and completed file list |
| `task scratchpad <space> <task>` | Read a task's Scratchpad | One page of notes, with their times, in time order |
| `task close <space> <task-id>` | Change a task, setting only its status to `ended` | Updated task |
| `task delete <space> <task-id>` | Delete a task | Deletion result |
| `note delete <space> <task-id> <note>` | Delete a note | Deletion result |
| `file url <space> <file>` | Get a file download URL | The short-lived download URL |
| `file delete <space> <task-id> <file>` | Delete a file | Deletion result |
| `knowledge list <space> [--prefix <prefix>] [--keyword <text>]` | List Knowledge | Entry names, keys and summaries, and the next level's prefixes |
| `knowledge show <space> <entry> [--version <version>]` | Read the latest or an old knowledge version | Name, key, summary and content, or a deletion version |
| `knowledge history <space> <entry>` | View a knowledge entry's history | Versions and their authors and times, including deletions |
| `knowledge deleted <space>` | List deleted knowledge entries | Deleted entries and the information needed to read their history |
| `knowledge create <space> <name> --summary <summary> (--content <content> \| --content-file <path\|->)` | Create a knowledge entry directly | Created entry |
| `knowledge change <space> <entry> [--name <name>] [--summary <summary>] [--content <content> \| --content-file <path\|->]` | Change a knowledge entry directly | Updated entry |
| `knowledge delete <space> <entry>` | Delete a knowledge entry directly | Deletion result |
| `knowledge rollback <space> <entry> <version>` | Roll back a knowledge entry directly | Entry with the new version |
| `proposal list <space> [--status <status>] [--keyword <text>]` | List proposals | Proposal keys, kinds, statuses, target entries and proposed summaries |
| `proposal show <space> <proposal>` | Read a proposal | Its fields, content, status and rejection reason, when present |
| `proposal create <space> <name> --summary <summary> (--content <content> \| --content-file <path\|->)` | Submit a create proposal | Submitted proposal |
| `proposal change <space> <entry> [--name <name>] [--summary <summary>] [--content <content> \| --content-file <path\|->]` | Submit a change proposal | Submitted proposal |
| `proposal delete <space> <entry>` | Submit a delete proposal | Submitted proposal |
| `proposal approve <space> <proposal>` | Approve a proposal | Decided proposal |
| `proposal reject <space> <proposal> --reason <reason>` | Reject a proposal, including withdrawing one's own party's | Decided proposal |

- **An update requires a field to change.** `space change`, `knowledge
  change` and `proposal change` require at least one of their optional
  fields; omitted fields are unchanged. Proposal inputs and version behavior
  follow [Knowledge](../../lib/docs/contracts/knowledge.md#3-knowledge-entries).
- **Roles and statuses are checked before making a call.** Space member
  addition accepts `contributor` or `viewer`; setting a role accepts `admin`,
  `contributor` or `viewer`. Ownership is not assigned by these commands.
  Task statuses are `in progress` and `ended`; proposal statuses are
  `pending`, `approved` and `rejected`. Another value is a usage error.
- **File lists come from `task show`.** `file url` returns the Web
  API's URL; the person can transfer the file's content with it over HTTP as
  [Tasks](../../lib/docs/contracts/tasks.md#5-files) describes.
- **`--own-open` lists only the person's own tasks that are not `ended`**,
  as `list_tasks` does for an agent's party; see
  [MCP tools](../../lib/docs/contracts/mcp-tools.md#6-wrong-task-ids).
- **A contributor proposes a rollback with `proposal change`.** They read
  the chosen old version and supply its content and summary, following
  [Knowledge](../../lib/docs/contracts/knowledge.md#3-knowledge-entries).
  The CLI never silently changes a direct operation into a proposal.

## 4. Sign-in and credentials

- **`login` takes the server's URL.** It accepts `https`, or `http` only
  when the host is `localhost`, any IPv4 address in `127.0.0.0/8`, or the
  IPv6 address `::1`. Every other host requires `https`.
  An invalid URL or another `http` host is a usage error, before
  requesting credentials. The CLI keeps the URL with the login session and
  sends authenticated calls only to that server until another login
  succeeds, including after an HTTP redirect. Other commands have no server
  override.
- **HTTPS uses the system's trusted certificate authorities.** When
  `SSL_CERT_FILE` is set, the CLI trusts the authorities in that file instead
  of the system store.
  The CLI adds no certificate option or environment variable of its own.
- **Default sign-in uses a browser on the CLI's computer.** It follows
  [Identity](../architecture/identity.md#5-sign-in). The CLI always prints
  the sign-in link on standard error, then opens it in a browser unless
  `--no-browser` was given. If opening fails, it explains that the person
  can open the printed link and continues waiting. It does not choose
  device sign-in automatically. A stray request to the local redirect
  address cannot complete or end the sign-in. The wait ends on approval,
  refusal or a time limit, with the outcome reported.
- **`login --device` uses a browser on any computer.** On standard error it
  prints the verification address, the short code and the link carrying the
  code, and asks the person to check that code in the browser. It opens no
  browser and never prints the secret device code. The person signs in there
  and approves or declines the request.
  The CLI waits at the server's polling pace and slows down when asked;
  temporary connection failures, server errors and rate limits are retried
  while the request lasts. Approval stores a login session; refusal or expiry stores
  nothing new and exits 1, saying which happened. `--device` and
  `--no-browser` together are a usage error.
- **Credentials belong to one local directory.** The CLI keeps the server
  URL and the login session's access and refresh tokens in a file only the
  person can read, under `.lorebay` in the home directory, or in the
  directory `LOREBAY_HOME` names when set. A successful login replaces the
  saved session; a failed one preserves any earlier saved session. Login
  completion output contains no tokens or secret authorization codes.
- **Refresh is automatic.** An expired access token, or a 401 response,
  triggers one refresh and at most one repeat of the operation. Concurrent
  commands using the same local directory never present the same refresh
  token twice, as [Identity](../architecture/identity.md#7-tokens) requires.
  Readers see complete credentials even if another command stops during
  replacement. If the server refuses the refresh because the session ended,
  the CLI removes its credentials, asks the person to run `lorebay login`
  again and exits 4. A connection failure preserves the credentials and
  exits 1. If a refresh's outcome is unknown, the next command retries with
  the saved token; if the server had already replaced it, the server ends
  that login session under [Identity](../architecture/identity.md#7-tokens),
  and the CLI removes its credentials, asks the person to run
  `lorebay login` and exits 4. A command cannot wait indefinitely for another
  command refreshing the credentials; a wait that times out exits 1 without
  refreshing.
- **`logout` removes local credentials even when the server is unreachable.**
  It attempts the Web API logout so both tokens stop working, then removes
  the saved session. It reports when the session was not ended on the
  server. An unreachable server or another failure exits 1 after local
  removal; an already absent server session is treated as signed out.
  With no saved session, logout succeeds. A local removal failure exits 1
  and says that the credentials could not be removed.

## 5. Output and failures

- **Success data goes to standard output.** `human` is plain text for
  reading, with Markdown shown as its source. Lists have one row per item;
  `show` commands label fields and keep line feeds and tabs in multiline
  text. `json` prints one object carrying the operation's `data` and
  `meta`, without a CLI-specific envelope around them. Empty success
  responses, including login and logout, use `data: null` and `meta: {}`.
- **Hints, prompts and failures go to standard error.** This includes
  sign-in links, the short code meant for the person, and next-page hints.
  Output mode is explicit and does not change when a stream is redirected.
- **A failure prints one line and no success data.** In `human`, use the Web
  API error's `title`, followed by each field error's `message`, with line
  breaks and other control characters escaped. In `json`, print one object
  with the Web API error's fields, preserving its `code`; a login hint goes
  in an added `message` field. OAuth failures use the OAuth error code as
  `code` and its description as `message`. Local failures use `code` and
  `message`: `cli.usage`,
  `cli.login_required`, `cli.connection`, `cli.credentials`, `cli.timeout`
  or `cli.cancelled`, as appropriate. A missing or ended login session also
  gives the `lorebay login` hint, in that same line. No error includes a token.
- **Server text cannot control the terminal.** In `human`, escape control
  characters in all server-supplied fields, prompts, hints and error text,
  including terminal escape sequences, except that multiline text keeps
  line feed and tab. Single-line fields, prompts, hints and errors escape
  line feed and tab too. The CLI may insert its own line breaks and field
  separators. JSON strings use JSON escaping.

An example page in `json` mode:

```json
{"data":[{"key":"SPACE-1","name":"Team","summary":"Team work","role":"owner","caller_person":"alice"}],"meta":{"next_cursor":"opaque-cursor"}}
```

Exit codes:

| Code | Meaning |
| --- | --- |
| 0 | Success, including help and logout when already signed out |
| 1 | The server refuses the operation, or another failure occurs, including declined or expired sign-in, a connection failure or cancelled deletion |
| 2 | Usage error, such as a missing argument, invalid option, role, status or server URL |
| 4 | No saved login session, or the login session has ended and refresh cannot restore it |

## 6. Lists

- **A list command prints one page.** It accepts `--page-size <count>` and
  `--cursor <cursor>`, passed as the Web API's `page_size` and `cursor`.
  Omission uses the Web API defaults and order. This also applies to
  `task scratchpad`, `knowledge history` and `knowledge deleted`.
- **The next cursor stays opaque.** A nonempty `meta.next_cursor` is kept in
  JSON output. In either output mode, standard error gives the command for
  the next page with that cursor, retaining the space, page size and every
  filter. No next cursor means no hint and no further page is fetched. See
  [Web API](web-api.md#4-conventions).

## 7. Deletion confirmation

- **A deleting command needs an explicit yes.** This covers `task delete`,
  `note delete`, `file delete`, `knowledge delete` and `agent revoke`.
  Before sending the operation it names the target on standard error and
  accepts only `y` or `yes`, ignoring case. An empty
  answer, end of input or any other answer cancels the operation. When input
  is not a terminal, the CLI does not read it and treats it as no. Nothing
  is deleted, and cancellation exits 1.
- **`--yes` skips the question.** It is accepted on these deleting commands
  in both output modes. `member remove` runs without a confirmation
  prompt. Logout is an explicit sign-out and needs no further confirmation.
  `proposal delete` submits a proposal rather than deleting
  an entry; it follows [Knowledge](../../lib/docs/contracts/knowledge.md#4-proposals).
