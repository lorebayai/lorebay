# Keys and References

- **Status:** Proposed
- **Level:** Contract
- **Parent:** [Lorebay](../product/lorebay.md)
- **Related:** [MCP tools](mcp-tools.md), [Web API](web-api.md),
  [Data model](../architecture/data-model.md)

> Keys are for people and references. A write call names its task only by
> the random task ID.

## 1. Purpose

People need short, stable names to read and to cite, such as `TK-12`. Agents
fill tool arguments themselves and can copy one wrong; a random task ID turns
such a slip into an error instead of a write into the wrong task.

## 2. Scope

This doc covers keys, task IDs and references in text.

It does not cover showing an object which other objects reference it.

## 3. Keys

- **Tasks, knowledge entries, artifacts and proposals have keys**: a prefix
  and a number that counts up, such as `TK-1`, `KN-1`, `AR-1` and `PP-1`.
  Each space counts from 1, and a deleted key is never reused. Pages show
  keys.
- **A space's key**, such as `SPACE-1`, counts up across the whole Lorebay.
  Its number is separate from the internal primary key. The key never
  changes or gets reused, even when the space is renamed.
- **Objects to read may be named by key**, such as `TK-12` in `get_task` or
  `KN-3` in `read_knowledge`, resolved in the space the call names. Web API
  paths accept keys too, such as `/api/v1/spaces/SPACE-1/tasks/TK-12`,
  except for the task in a call that writes into it; see
  [section 4](#4-task-ids).

## 4. Task IDs

- **A task ID is a random string**, separate from the task's key. Copying one
  wrong yields "not found", never another real task.
- **A call that writes into a task names it only by its task ID**, never by
  key: the MCP `task_id` argument and the task in a Web API path alike.
  Changing or deleting a task counts as writing into it.

## 5. References

- **A reference is `#` followed by a key**, such as `#KN-1`. Lorebay shows it
  as a link that reads `#KN-1`.
- **Where references work:** task titles and results, notes, Knowledge and
  proposals. References are optional.
- **Notes have no references of their own.** Refer to the note's task.
- **Unresolvable references show as plain text**, with no error: a key that
  doesn't exist, was deleted, or belongs to another space.
- **Writes don't check references.** Pages resolve them when they show the
  text.
- **Fixing a wrong reference.** Fix it where it was written. A result can be
  edited. A note cannot: append a note that corrects it, or a person deletes
  the note on the web.
