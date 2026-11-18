# Library

- **Status:** Proposed
- **Level:** System
- **Related:** [Library architecture](../architecture/library.md),
  [Access control](../contracts/access-control.md),
  [Agent skill](../contracts/agent-skill.md)

> The library exists to make people's attention count: anyone in a team must
> be able to see quickly what an agent did and how it did it.
>
> Spaces are fully isolated from each other.

## 1. Purpose

The library solves how people and agents work together in a team. Without
it:

- What one person's agent did is invisible to the rest of the team; the
  record stays in that person's harness.
- Knowing what a session did, and how, means reading the whole conversation.
- Every new session needs the background explained again, and what an agent
  learns while working is lost.

How the data is shown matters most: a reader must be able to tell quickly
what an agent did and how.

A product builds the library into its own process and gives its people
spaces, tasks, notes, artifacts, Knowledge and the MCP tools for their
agents. What the library holds and what a product holds is in
[Library architecture](../architecture/library.md#3-boundary).

## 2. Scope

This doc covers the problem the library solves, what a space is, and the
five kinds of data a space holds. The library does only what its design
docs describe; anything they don't describe is out of scope.

It does not cover:

- notifications, such as telling approvers about a new proposal;
- comments or replies on tasks, notes, Knowledge or proposals.

## 3. Spaces

A product holds many spaces. A space is one team's workspace.

- **Spaces are isolated.** Each space has its own members, tasks,
  Scratchpads, artifacts, Knowledge and proposals, and no space sees
  another's. A person may belong to several spaces.
- **The product decides who may create a space**, and who belongs to it
  with which role. The library enforces what each role may do; see
  [Access control](../contracts/access-control.md).
- **Name and summary.** Each space has a name and a one-sentence summary of
  what it is for. Its creator writes them; its owner and admins may change
  them. Names need not be unique.
- **Key.** Each space has a key, such as `SPACE-1`, that never changes. It is
  its public identifier, separate from its internal primary key. See
  [Keys and references](../contracts/keys-and-references.md).
- From a space, anyone in it can see every member's tasks, what each task
  produced and how the work was done.

## 4. Data

The library stores five kinds of data in each space:

| Data | What it records | Home |
| --- | --- | --- |
| Task | What one piece of agent work did, and its result | [Tasks](../contracts/tasks.md#3-tasks) |
| Scratchpad | How the agent did it: the notes it wrote along the way | [Tasks](../contracts/tasks.md#4-scratchpad) |
| Artifact | Files the agent produced | [Tasks](../contracts/tasks.md#5-artifacts) |
| Knowledge | Shared documents later sessions read | [Knowledge](../contracts/knowledge.md#3-knowledge-entries) |
| Proposal | A requested change to Knowledge, waiting for a person | [Knowledge](../contracts/knowledge.md#4-proposals) |
