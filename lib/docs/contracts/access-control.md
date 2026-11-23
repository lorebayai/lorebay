# Access Control

- **Status:** Proposed
- **Level:** Contract
- **Parent:** [Library](../product/library.md)
- **Related:** [Library architecture](../architecture/library.md),
  [Tasks](tasks.md), [Knowledge](knowledge.md), [MCP tools](mcp-tools.md)

> Everyone in a space reads everything in it. Only a party writes its own
> tasks, checkpoints and files, and only space owners and space admins change
> Knowledge without a proposal.
>
> Agents are never space owners or space admins, and never approve or reject
> proposals.

## 1. Purpose

A team shares one space, so every reader must see all of its work, while
each party's records stay its own and changes to shared Knowledge stay under
people's control.

## 2. Scope

This doc covers the four space roles, what each may do in every library
operation, and the checks that decide which space a call acts on. Who
belongs to a space with which space role, and how that changes, is the
product's record, which the library reads through the
[space roles interface](../architecture/library.md#63-space-roles); how a
caller is verified is the product's too.

It does not cover how a product maps roles of its own to the library's
four.

## 3. Roles

- **Four space roles.** A person in a space has one space role: space owner,
  space admin, space contributor or space viewer. A person outside the
  space has none.
- **Space roles are per space.** A person may be the space owner of one
  space and a space viewer in another.
- **An agent's space role is its person's space role, capped at space
  contributor.** An agent is never a space owner or space admin, so the
  agents of space owners and space admins are space contributors. Agents
  also never approve or reject proposals. The library applies the cap
  itself, whatever space role the product reports.
- **A removed person has no space role.** Their tasks, Scratchpads and files
  stay, visible to everyone as before, and still show their name.

## 4. Permissions

Within a space, every space member reads all of its data, agents included,
except where the table says otherwise.

| Resource | Operation | Person | Agent |
| --- | --- | --- | --- |
| Space | List own spaces, read one | Yes | Yes |
| Space | Change name and summary | Owner, admin | No |
| Task | List, read one, read its Scratchpad | Yes | Yes |
| Task | Create, append a checkpoint | No | Yes, except viewers |
| Task | Change title, status, result | Own tasks, except viewers; owners and admins may also close anyone's | Own party's tasks, except viewers |
| Task | Delete | Own tasks, except viewers; owners and admins: anyone's | No |
| Checkpoint | Delete one | In own tasks, except viewers; owners and admins: anyone's | No |
| File | Register and get an upload URL | No | Yes, except viewers |
| File | Report the upload complete | No | Yes, except viewers |
| File | Get a download URL | Yes | Yes |
| File | Delete | Own files, except viewers; owners and admins: anyone's | No |
| Knowledge | List, read the latest version | Yes | Yes |
| Knowledge | View history, read old versions, list deleted entries | Yes | No |
| Knowledge | Create, change, delete or roll back directly | Owner, admin | No |
| Proposal | List, read one | Yes | Yes |
| Proposal | Submit | Yes, except viewers | Yes, except viewers |
| Proposal | Approve, reject | Yes, except viewers; see [Knowledge](knowledge.md#4-proposals) | No |

- **"Own party's"** means the tasks of the agent's person, created by that
  agent or by any other agent of the same person. Appending checkpoints,
  registering files and reporting uploads complete also work only on own
  party's tasks.
- **Only agents write task records.** People never create tasks, append
  checkpoints or upload files; they tell an agent to do the work.
- **Space owners and space admins close, not edit, other people's tasks.**
  They may close or delete anyone's task and delete anyone's checkpoints and
  files, such as a removed person's task left `in progress`. They cannot
  change the title or result of someone else's task.
- **Space viewers are read-only.** A person moved from space contributor to
  space viewer can no longer change or delete their own earlier tasks,
  checkpoints and files.
- **Knowledge belongs to the whole space**, not to one person; see
  [Knowledge](knowledge.md).
- **Creating a space is the product's rule.** The library inserts the space
  inside the product's write; see
  [Library architecture](../architecture/library.md#8-tenant-and-space-numbers).

## 5. Space selection

- An agent is in several spaces at once, so every call names its target.
  Reads and task creation name a space. Calls that write into a task, or
  record which task a proposal came from, carry the task ID and act on that
  task's space.
- Permissions follow the person's space role in that space.
- Which MCP tool carries what is in [MCP tools](mcp-tools.md#4-tools).

## 6. Space and task checks

Every operation runs these checks, whichever entry point calls it.

A call that carries a task ID is checked in this order:

1. Take the caller: the actor and its tenant.
2. Load the task by ID, giving its space and its person.
3. Reject the call as not found if the space belongs to a tenant other
   than the caller's.
4. Ask for the person's space role in that space. Reject the call as not
   found if the person has none, a removed person included, so a caller
   outside a space cannot tell which tasks it holds.
5. Reject the call if the task is not the caller's party's, unless space
   owners and space admins may act on anyone's task; see
   [section 4](#4-permissions).
6. Reject the call if the person is a space viewer.
7. Reject a checkpoint or a file registration if the task is `ended`, with a
   hint to set the task back to `in progress` first.

A call that carries a space is checked in this order:

1. Take the caller: the actor and its tenant.
2. Load the space. Reject the call as not found if it belongs to a tenant
   other than the caller's.
3. Ask for the person's space role in that space. Reject the call if the
   person has none, or if the call writes and the person is a space viewer.
4. Reject the call if the object it reads or writes is not in that space.
