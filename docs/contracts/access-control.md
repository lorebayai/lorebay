# Access Control

- **Status:** Proposed
- **Level:** Contract
- **Parent:** [Lorebay](../product/lorebay.md)
- **Related:** [Identity](../architecture/identity.md),
  [Web API](web-api.md),
  [Library access control](../../lib/docs/contracts/access-control.md)

> Who belongs to a space, and with which role, is Lorebay's record. What
> each role may do is the library's.
>
> Only owners and admins manage participants, and only the owner manages
> admins.

## 1. Purpose

A space's owner decides who works in it. Lorebay records those people and
their roles, and the library enforces what each role may do with the space's
data.

## 2. Scope

This doc covers participants, how they are managed, and the permissions of
Lorebay's own operations: sign-in, agent authorization, device sign-in,
users, creating spaces and managing participants. The roles, the permission
of every library operation and the space and task checks are in
[Library access control](../../lib/docs/contracts/access-control.md).
Credentials are in [Identity](../architecture/identity.md).

It does not cover:

- people leaving a space on their own;
- transferring a space's ownership.

## 3. Participants

- **Participants.** Each space has participants: the people in it, each
  with one of the library's
  [roles](../../lib/docs/contracts/access-control.md#3-roles).
- **An agent follows its person.** It is in every space its person is in,
  with its person's role capped as the library caps it, and leaves a space
  when its person does.
- **Listing participants also lists their agents.** Each row has a type,
  `human` or `agent`, a username and a role; an agent's row adds its name,
  and its role is its person's, capped.
- **The creator of a space is its owner.** The owner appoints admins.
- **Adding a participant.** An owner or admin picks a person from all users
  in the users file and makes them a contributor or a viewer. Owners and
  admins may change that role later.
- **Removing a participant.** A removed person cannot enter the space, and
  their agents leave it with them. What happens to their work is in the
  library's [roles](../../lib/docs/contracts/access-control.md#3-roles).
- **Limits on removal.** The owner cannot be removed. Only the owner can
  remove an admin.
- **A deleted account counts as removed from every space**; see
  [Identity](../architecture/identity.md#3-accounts).
- **Every participant operation checks the space as the library does.**
  Listing, adding, changing and removing participants resolve the space key
  through the library, which answers a space the caller may not see as not
  found; Lorebay then checks the caller's role itself.

## 4. Permissions

These are the rows of Lorebay's own operations. Every operation on a
space's content follows the library's
[permission matrix](../../lib/docs/contracts/access-control.md#4-permissions),
whichever credential the caller holds.

| Resource | Operation | Person (login session) | Agent (agent authorization) |
| --- | --- | --- | --- |
| Sign-in | Log in, refresh the login session, log out | Yes | Not applicable |
| Authorization | Read an authorization request, submit the decision on it | Yes | No |
| Authorization | List own agents, revoke one | Yes | No |
| Device sign-in request | Read a request, submit the decision on it | Yes | No |
| User | List the users in the users file | Yes | No |
| Space | Create | Yes | No |
| Participant | List | Yes | Yes |
| Participant | Add, change role, remove | Owner, admin; only the owner when an admin is involved | No |

- **Any signed-in person may create a space.**
- **Credentials map onto the library's columns.** In the library's matrix,
  a login session calls as a person and an agent authorization as an
  agent.
