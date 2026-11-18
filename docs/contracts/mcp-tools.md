# MCP Tools

- **Status:** Proposed
- **Level:** Contract
- **Parent:** [System architecture](../architecture/system.md)
- **Related:** [Web API](web-api.md), [Access control](access-control.md),
  [Keys and references](keys-and-references.md), [Agent skill](agent-skill.md)

> Each of the 14 tools maps to one Web API operation and adds no logic of
> its own.
>
> Calls that write into a task carry `task_id`; other calls carry the space;
> only `list_spaces` carries neither.

## 1. Purpose

Agents reach Lorebay through MCP from any harness. The tools cover what an
agent may do, and nothing more.

## 2. Scope

This doc covers the MCP protocol versions, the tools, and how they map to Web
API operations. Permissions are in [Access control](access-control.md).

It does not cover:

- a harness hook that fills in the task ID automatically;
- the HTTP+SSE transport of MCP 2024-11-05.

## 3. Protocol versions

- **Lorebay accepts MCP 2025-03-26 through 2026-07-28**, over the Streamable
  HTTP transport, so a harness that has not moved to the newest version still
  connects. Lorebay advertises 2026-07-28.
- **Lorebay keeps no protocol session at any version.** A harness on a
  version with an initialization handshake gets it answered, and nothing is
  remembered from it.
- **Every call names its target.** Every request stands alone, so each call
  carries its own space or task ID.

## 4. Tools

| Tool | Carries | Does | Web API operation |
| --- | --- | --- | --- |
| `list_spaces` | Neither | Lists the person's spaces: key, such as `SPACE-1`; name; summary; role | List spaces |
| `create_task` | Space | Creates a task from a space and a title; returns the task ID and key | Create a task |
| `update_task` | Task ID | Changes the title, status or result, each optional; the status may go back to `in progress`; the result is replaced whole | Change a task |
| `list_tasks` | Space | Lists the space's tasks, filtered by person, agent or status, or only the caller's party's tasks that are not `ended`; takes a keyword; each row has the agent name and last activity time; newest activity first | List tasks |
| `get_task` | Space | Reads one task: its result and artifact list, and optionally its Scratchpad | Read a task |
| `add_note` | Task ID | Appends a note to the task | Append a note |
| `create_artifact` | Task ID | Registers a file name and summary; returns a short-lived upload URL and the headers the upload must carry | Register an artifact |
| `complete_artifact` | Task ID | Reports the upload finished; Lorebay checks the file is in storage, and only then is the upload complete | Report an upload complete |
| `get_artifact_url` | Space | Returns a short-lived download URL | Get a download URL |
| `list_knowledge` | Space | Lists one level by name prefix: its entries, with names and summaries, and the next level's prefixes; no prefix lists the top level; a keyword searches names, summaries and content across the space | List Knowledge |
| `read_knowledge` | Space | Reads one entry by name, latest version only | Read a knowledge entry |
| `create_proposal` | Task ID | Proposes creating, changing or deleting an entry | Submit a proposal |
| `list_proposals` | Space | Lists proposals, filtered by status | List proposals |
| `get_proposal` | Space | Reads one proposal: content, status, rejection reason | Read a proposal |

- **Lists are paginated.** `list_spaces`, `list_tasks`, `list_knowledge`,
  `list_proposals` and the Scratchpad in `get_task` return pages, so no call puts an
  unbounded result into the model's context. Paging follows
  [Web API](web-api.md#4-conventions).
- **`create_artifact` returns an object, not a bare URL**, so multipart
  upload can be added without changing the tool. How an upload uses the URL
  and headers is in [Tasks](tasks.md#5-artifacts).
- **Files never pass through MCP.** Agents upload and download them over
  HTTP with the URLs they get.
- **Objects to read may be named by key**; see
  [Keys and references](keys-and-references.md#3-keys).

What MCP leaves out, people do on the web: deleting tasks, notes and
artifacts; approving and rejecting proposals; changing Knowledge directly;
viewing history and rolling back; managing participants; creating spaces.
Agents may also read a space and list its participants through the Web API;
those two operations have no tool.

## 5. The MCP layer

The MCP layer runs inside `lorebayd` and calls the API operations directly,
not over the network. It does two things:

1. Converts tool arguments into operation arguments.
2. Converts the result back, adding the task's title to the result of every
   write.

The Web API takes the same arguments from the path; MCP takes them from the
tool arguments. A call that carries a task ID passes it alone, and the task
check finds the task's space. From then on the checks are identical; see
[Access control](access-control.md#6-space-and-task-checks).

## 6. Wrong task IDs

The model fills in `task_id` itself, and the harness only forwards it, so the
ID can be wrong:

| Filled in | Lorebay | Outcome |
| --- | --- | --- |
| An ID that doesn't exist | Rejects the call | The agent sees the error and fills it in again |
| Another party's task ID | Rejects the call | The agent sees the error and fills it in again |
| Another task of the caller's own party | Passes every check | The note lands in the wrong task; agents cannot delete it, so a person deletes it on the web |

Three rules reduce the risk:

- **Task IDs are random**, so a copying slip yields "not found"; see
  [Keys and references](keys-and-references.md#4-task-ids).
- **Every write returns the task's title**, so the agent sees which task it
  wrote into.
- **`list_tasks` can list only the caller's party's tasks that are not
  `ended`**, so an agent that lost its task ID can find it again.

## 7. References

- [MCP changelog, 2026-07-28](https://modelcontextprotocol.io/specification/2026-07-28/changelog):
  protocol-level sessions and the initialization handshake are removed, and
  every request stands alone.
- [MCP transports, 2025-03-26](https://modelcontextprotocol.io/specification/2025-03-26/basic/transports):
  the first version with the Streamable HTTP transport.
