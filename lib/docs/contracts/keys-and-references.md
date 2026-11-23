# Keys and References

- **Status:** Proposed
- **Level:** Contract
- **Parent:** [Library](../product/library.md)
- **Related:** [MCP tools](mcp-tools.md),
  [Data model](../architecture/data-model.md),
  [Library architecture](../architecture/library.md)

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

- **Tasks, knowledge entries, files and proposals have keys**: a prefix
  and a number that counts up, such as `TK-1`, `KN-1`, `FL-1` and `PP-1`.
  Each space counts from 1, and a deleted key is never reused. Products
  show keys.
- **A space's key**, such as `SPACE-1`, carries the number the product
  supplies when it creates the space. The library refuses a number already
  taken and derives the key from it; the product never reuses a number. See
  [Library architecture](../architecture/library.md#8-tenant-and-space-numbers).
  The number is separate from the internal primary key. The key never
  changes or gets reused, even when the space is renamed.
- **Objects to read may be named by key**, such as `TK-12` in `get_task` or
  `KN-3` in `read_knowledge`, resolved in the space the call names. A
  product's entry points accept keys too, except for the task in a call that
  writes into it; see [section 4](#4-task-ids).

## 4. Task IDs

- **A task ID is a random string**, separate from the task's key. Copying one
  wrong yields "not found", never another real task.
- **A call that writes into a task names it only by its task ID**, never by
  key: the MCP `task_id` argument and the task in a product's entry point
  alike.
  Changing or deleting a task counts as writing into it.

## 5. References

- **A reference is `#` followed by a key**, such as `#KN-1`. A product shows
  it as a link that reads `#KN-1`.
- **Where references work:** task titles and results, checkpoints, Knowledge and
  proposals. References are optional.
- **Checkpoints have no references of their own.** Refer to the checkpoint's
  task.
- **Unresolvable references show as plain text**, with no error: a key that
  doesn't exist, was deleted, or belongs to another space.
- **Writes don't check references.** A product resolves them when it shows
  the text.
- **Fixing a wrong reference.** Fix it where it was written. A result can be
  edited. A checkpoint cannot: append a checkpoint that corrects it, or a
  person deletes the checkpoint.
