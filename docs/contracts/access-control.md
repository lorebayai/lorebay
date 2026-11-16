# Access Control

- **Status:** Proposed
- **Level:** Contract
- **Parent:** [Lorebay](../product/lorebay.md)
- **Related:** [Identity](../architecture/identity.md),
  [Tasks](tasks.md), [Knowledge](knowledge.md),
  [MCP tools](mcp-tools.md), [Web API](web-api.md)

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

This doc covers participants, roles, the permission of every operation, and
the checks that decide which space a call acts on. Credentials are in
[Identity](../architecture/identity.md).

It does not cover:

- people leaving a space on their own;
- transferring a space's ownership.

## 3. Participants and roles

- **Participants.** Each space has participants. Each participant has a type,
  human or agent, and a role: owner, admin, contributor or viewer.
- **Roles are per space.** A person may be the owner of one space and a
  viewer in another.
- **An agent's role is its person's role, capped at contributor.** An agent
  is never an owner or admin, so the agents of owners and admins are
  contributors. Agents also never approve or reject proposals.
- **The creator of a space is its owner.** The owner appoints admins.
- **Adding a participant.** An owner or admin picks a person from all users
  in the users file and makes them a contributor or a viewer. Owners and
  admins may change that role later.
- **Removing a participant.** A removed person cannot enter the
  space, and their agents leave it with them. Their tasks, Scratchpads and
  artifacts stay, visible to everyone as before.
- **Limits on removal.** The owner cannot be removed. Only the owner can
  remove an admin.
- **A deleted account counts as removed from every space**; see
  [Identity](../architecture/identity.md#3-accounts).

## 4. Permissions

Within a space, every participant reads all of its data, agents included,
except where the table says otherwise.

| Resource | Operation | Person (login session) | Agent (agent authorization) |
| --- | --- | --- | --- |
| Sign-in | Log in, log out | Yes | Not applicable |
| Authorization | List own agents, revoke one | Yes | No |
| User | List the users in the users file | Yes | No |
| Space | List own spaces, read one | Yes | Yes |
| Space | Create | Yes | No |
| Space | Change name and summary | Owner, admin | No |
| Participant | List | Yes | Yes |
| Participant | Add, change role, remove | Owner, admin; only the owner when an admin is involved | No |
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
  or delete anyone's task and delete anyone's notes and artifacts on the
  web, such as a removed person's task left `in progress`. They cannot change
  the title or result of someone else's task.
- **Viewers are read-only.** A person moved from contributor to viewer can no
  longer change or delete their own earlier tasks, notes and artifacts.
- **Knowledge belongs to the whole space**, not to one person; see
  [Knowledge](knowledge.md).

## 5. Space selection

- An agent is in several spaces at once, so every call names its target.
  Reads and task creation name a space. Calls that write into a task, or
  record which task a proposal came from, carry the task ID and act on that
  task's space.
- Permissions follow the person's role in that space.
- Web API calls name the space in the path, and Lorebay checks the agent's
  role in that space. See [Web API](web-api.md#3-paths).
- Which MCP tool carries what is in [MCP tools](mcp-tools.md#4-tools).

## 6. Space and task checks

Every agent call runs these checks, through MCP and the Web API alike.

A call that carries a task ID is checked in this order:

1. Verify the access token, giving the person and the agent.
2. Load the task by ID, giving its space and its person.
3. Reject the call if the task is not the caller's party's.
4. Look up the person's role in that space. Reject the call if the person
   was removed, or if the person is a viewer.
5. Reject a note or an artifact registration if the task is `ended`, with a
   hint to set the task back to `in progress` first.

A call that carries a space is checked in this order:

1. Verify the access token, giving the person and the agent.
2. Look up the person's role in that space. Reject the call if the person is
   not in the space, or if the call writes and the person is a viewer.
3. Reject the call if the object it reads or writes is not in that space.
