# Lorebay

- **Status:** Proposed
- **Level:** System
- **Related:** [System architecture](../architecture/system.md),
  [Access control](../contracts/access-control.md),
  [Agent skill](../contracts/agent-skill.md)

> Lorebay exists to make people's attention count: anyone in a team must be
> able to see quickly what an agent did and how it did it.
>
> Spaces are fully isolated from each other.

## 1. Purpose

Lorebay solves how people and agents work together in a team. Without it:

- What one person's agent did is invisible to the rest of the team; the
  record stays in that person's harness.
- Knowing what a session did, and how, means reading the whole conversation.
- Every new session needs the background explained again, and what an agent
  learns while working is lost.

How the data is shown matters most: a reader must be able to tell quickly
what an agent did and how.

## 2. Scope

This doc covers what Lorebay is, what a space is, and what a deployment
ships. Lorebay does only what its design docs describe; anything they don't
describe is out of scope.

It does not cover:

- the layout of the web pages;
- notifications, such as telling approvers about a new proposal;
- comments or replies on tasks, notes, Knowledge or proposals;
- deleting a space: nobody can delete one, including its owner;
- a Lorebay-wide administrator who can list or delete spaces or reassign
  their owners;
- non-functional requirements, such as supported scale or data retention.

## 3. Spaces

A Lorebay holds many spaces. A space is one team's workspace.

- **Spaces are isolated.** Each space has its own participants, tasks,
  Scratchpads, artifacts, Knowledge and proposals, and no space sees
  another's. A person may belong to several spaces.
- **Anyone who can log in may create a space**, and becomes its owner.
- **Name and summary.** Each space has a name and a one-sentence summary of
  what it is for. Its creator writes them; its owner and admins may change
  them. Names need not be unique.
- **Key.** Each space has a key, such as `SPACE-1`, that never changes. It is
  its public identifier, separate from its internal primary key. See
  [Keys and references](../contracts/keys-and-references.md).
- A space holds many people and many agents. Who may do what is defined in
  [Access control](../contracts/access-control.md).
- From a space, anyone in it can see every member's tasks, what each task
  produced and how the work was done.

## 4. What a deployment ships

- **Web pages.** Deploying Lorebay gives the data, the APIs and the pages
  people read, from one process. See
  [System architecture](../architecture/system.md).
- **Skill.** The skill ships with Lorebay. The web pages explain how to
  install the skill and connect an agent's MCP client. See
  [Agent skill](../contracts/agent-skill.md).

## 5. Data

Lorebay stores five kinds of data in each space:

| Data | What it records | Home |
| --- | --- | --- |
| Task | What one piece of agent work did, and its result | [Tasks](../contracts/tasks.md#3-tasks) |
| Scratchpad | How the agent did it: the notes it wrote along the way | [Tasks](../contracts/tasks.md#4-scratchpad) |
| Artifact | Files the agent produced | [Tasks](../contracts/tasks.md#5-artifacts) |
| Knowledge | Shared documents later sessions read | [Knowledge](../contracts/knowledge.md#3-knowledge-entries) |
| Proposal | A requested change to Knowledge, waiting for a person | [Knowledge](../contracts/knowledge.md#4-proposals) |
