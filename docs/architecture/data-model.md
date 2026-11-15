# Data Model

- **Status:** Proposed
- **Level:** Implementation
- **Parent:** [Storage](storage.md)
- **Related:** [Tasks](../contracts/tasks.md),
  [Knowledge](../contracts/knowledge.md),
  [Keys and references](../contracts/keys-and-references.md)

> Participant rows hold only people. An agent's spaces and role are derived
> from its person on every request, never stored.

## 1. Purpose

This doc lists the tables behind the business data and the storage rules
the contracts rely on.

## 2. Scope

This doc covers the 10 business tables. OAuth client metadata, authorization
codes and login sessions are stored too; they belong to the authorization
implementation and are not listed. Behavior observable through the APIs is
in the contract docs.

## 3. Tables

`user` and `space` sit directly under the Lorebay; every other table belongs
to one of them.

| Table | Belongs to | Holds |
| --- | --- | --- |
| `user` | Lorebay | Username from the users file; the password stays only in the file |
| `authorization` | `user` | Client, name (defaults to the client name), hashes of the access and refresh tokens |
| `space` | Lorebay | Key, such as `SPACE-1`; name; summary; owner |
| `participant` | `space` | Person, role |
| `task` | `space` | Key; person; agent name, copied from the authorization at creation; title; status; result; start, end and last activity times |
| `note` | `task` | Time, content |
| `artifact` | `task` | Key, file name, summary, size, location in file storage, whether the upload is complete |
| `knowledge` | `space` | Key, name, current version (always the latest), whether deleted |
| `knowledge_version` | `knowledge` | Content; summary; whether it is a deletion; who made it take effect, the person who changed it directly or who approved it; time; source proposal |
| `proposal` | `space` | Key; kind (create, change or delete); target entry, or the new name for a create; new content, summary and name, only the fields being changed for a change; base version, none for a create; proposer; source task; status; who approved or rejected it; time; reason |

## 4. Rules

- **`user` mirrors the users file**, synced at startup. When an account is
  removed from the file, its row is marked deleted, so its tasks still show
  whose they were.
- **`participant` stores only people.** An agent's spaces and role are
  computed from its person, so "an agent follows its person" needs no sync.
- **Deletion is real.** Deleting a task, by its owner or by a space owner or
  admin, deletes its notes, its artifact rows and its files. A proposal's
  source task is cleared; keys written in text stay and show as plain text.
- **Knowledge deletion is the exception.** Deleting an entry marks its row
  deleted and adds a deletion version, with no content, recording who deleted
  it, when, and from which proposal. Earlier content stays in older versions.
- **A proposal records its source task.** An agent always has a task ID when
  it proposes; a proposal made on the web has no source task.
- **A proposal records its base version**, so approval can detect that the
  entry changed after the proposal was made.
- **The name lives on the `knowledge` row.** Renaming creates no version, and
  rolling back never restores an old name. Names are unique within a space,
  deleted entries included.
- **Keys count up per space and per type**, and deleted keys are never reused.
  Space keys count up across the Lorebay.
- **References are not stored.** Pages resolve `#KN-1` when they show the
  text.
