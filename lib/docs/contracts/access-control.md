# Access Control

- **Status:** Proposed
- **Level:** Contract
- **Parent:** [Library](../product/library.md)
- **Related:** [Library architecture](../architecture/library.md),
  [Tasks](tasks.md), [Knowledge](knowledge.md), [MCP tools](mcp-tools.md)

> Everyone in a space reads everything in it. Only a party writes its own
> tasks, notes and artifacts, and only owners and admins change Knowledge
> without a proposal.
>
> Agents are never owners or admins, and never approve or reject proposals.

## 1. Purpose

A team shares one space, so every reader must see all of its work, while
each party's records stay its own and changes to shared Knowledge stay under
people's control.

## 2. Scope

This doc covers the four roles, what each may do in every library
operation, and the checks that decide which space a call acts on. Who
belongs to a space with which role, and how that changes, is the product's
record, which the library reads through the
[space roles interface](../architecture/library.md#63-space-roles); how a
caller is verified is the product's too.

It does not cover how a product maps roles of its own to the library's
four.

## 3. Roles

- **Four roles.** A person in a space has one role: owner, admin,
  contributor or viewer. A person outside the space has none.
- **Roles are per space.** A person may be the owner of one space and a
  viewer in another.
- **An agent's role is its person's role, capped at contributor.** An agent
  is never an owner or admin, so the agents of owners and admins are
  contributors. Agents also never approve or reject proposals. The library
  applies the cap itself, whatever role the product reports.
- **A removed person has no role.** Their tasks, Scratchpads and artifacts
  stay, visible to everyone as before, and still show their name.

## 4. Permissions

Within a space, every member reads all of its data, agents included, except
where the table says otherwise.

| Resource | Operation | Person | Agent |
| --- | --- | --- | --- |
| Space | List own spaces, read one | Yes | Yes |
| Space | Change name and summary | Owner, admin | No |
| Task | List, read one, read its Scratchpad | Yes | Yes |
| Task | Create, append a note | No | Yes, except viewers |
| Task | Change title, status, result | Own tasks, except viewers; owners and admins may also close anyone's | Own party's tasks, except viewers |
| Task | Delete | Own tasks, except viewers; owners and admins: anyone's | No |
| Note | Delete one | In own tasks, except viewers; owners and admins: anyone's | No |
| Artifact | Register and get an upload URL | No | Yes, except viewers |
| Artifact | Report the upload complete | No | Yes, except viewers |
| Artifact | Get a download URL | Yes | Yes |
| Artifact | Delete | Own artifacts, except viewers; owners and admins: anyone's | No |
| Knowledge | List, read the latest version | Yes | Yes |
| Knowledge | View history, read old versions, list deleted entries | Yes | No |
| Knowledge | Create, change, delete or roll back directly | Owner, admin | No |
| Proposal | List, read one | Yes | Yes |
| Proposal | Submit | Yes, except viewers | Yes, except viewers |
| Proposal | Approve, reject | Yes, except viewers; see [Knowledge](knowledge.md#4-proposals) | No |

- **"Own party's"** means the tasks of the agent's person, created by that
  agent or by any other agent of the same person. Appending notes,
  registering artifacts and reporting uploads complete also work only on own
  party's tasks.
- **Only agents write task records.** People never create tasks, append
  notes or upload artifacts; they tell an agent to do the work.
- **Owners and admins close, not edit, other people's tasks.** They may close
  or delete anyone's task and delete anyone's notes and artifacts, such as a
  removed person's task left `in progress`. They cannot change the title or
  result of someone else's task.
- **Viewers are read-only.** A person moved from contributor to viewer can no
  longer change or delete their own earlier tasks, notes and artifacts.
- **Knowledge belongs to the whole space**, not to one person; see
  [Knowledge](knowledge.md).
- **Creating a space is the product's rule.** The library inserts the space
  inside the product's write; see
  [Library architecture](../architecture/library.md#8-scope-and-space-numbers).

## 5. Space selection

- An agent is in several spaces at once, so every call names its target.
  Reads and task creation name a space. Calls that write into a task, or
  record which task a proposal came from, carry the task ID and act on that
  task's space.
- Permissions follow the person's role in that space.
- Which MCP tool carries what is in [MCP tools](mcp-tools.md#4-tools).

## 6. Space and task checks

Every operation runs these checks, whichever entry point calls it.

A call that carries a task ID is checked in this order:

1. Take the caller: the person, the agent and the scope.
2. Load the task by ID, giving its space and its person.
3. Reject the call as not found if the space belongs to another owner than
   the caller's scope.
4. Ask for the person's role in that space. Reject the call as not found if
   the person has none, a removed person included, so a caller outside a
   space cannot tell which tasks it holds.
5. Reject the call if the task is not the caller's party's, unless owners
   and admins may act on anyone's task; see [section 4](#4-permissions).
6. Reject the call if the person is a viewer.
7. Reject a note or an artifact registration if the task is `ended`, with a
   hint to set the task back to `in progress` first.

A call that carries a space is checked in this order:

1. Take the caller: the person, the agent and the scope.
2. Load the space. Reject the call as not found if it belongs to another
   owner than the caller's scope.
3. Ask for the person's role in that space. Reject the call if the person
   has none, or if the call writes and the person is a viewer.
4. Reject the call if the object it reads or writes is not in that space.
