# Tasks

- **Status:** Proposed
- **Level:** Contract
- **Parent:** [Library](../product/library.md)
- **Related:** [Access control](access-control.md),
  [Storage](../architecture/storage.md),
  [Keys and references](keys-and-references.md)

> Only agents create tasks, append notes and upload artifacts.
>
> An `ended` task accepts no new notes or artifact registrations until it is
> set back to `in progress`.

## 1. Purpose

A task shows a reader what one piece of agent work did; its Scratchpad shows
how; its artifacts hold the files it produced. Together they let a person
understand a session without reading the conversation.

## 2. Scope

This doc covers what the library stores and enforces for tasks, notes and
artifacts. When an agent creates a task, what its title and result say, and
when it writes notes are left to the skill a product ships.

It does not cover:

- transcripts: full records of a session, with its reasoning, every tool
  call and its result;
- tasks that people create, such as to-dos.

## 3. Tasks

- **A task records what one session did.** Tasks and sessions usually match
  one to one, with two exceptions: one session may hold several tasks one
  after another, and one task may span several sessions. Subagents are not
  separate sessions.
- **Only agents create tasks.** A person who wants work done tells an agent.
- **A deleted task is gone.** A person may delete their own task, with its
  Scratchpad and artifacts. Later writes to it are rejected as not found.

A task holds:

| Field | Rule |
| --- | --- |
| Key | Such as `TK-12`, assigned on creation; see [Keys and references](keys-and-references.md) |
| Title | One sentence |
| Status | `in progress` or `ended` |
| Person and agent | Whose agent created it; the agent name is copied from the caller at creation |
| Start and end times | The end time is set when the task becomes `ended`, cleared when it returns to `in progress`, and set again when it ends again |
| Last activity time | Set by the library whenever the task receives a note, an artifact or a new result |
| Result | Markdown text, rendered as Markdown |

- **`ended` means nobody is working on the task now**, not that the work is
  done. Whether it is done shows in the result.
- **Only agents and people change the status.** Agents usually do; a person
  may change the status of their own tasks, such as closing one left
  `in progress`. The library never changes a status on its own, so a session
  closed abruptly leaves its task `in progress`.
- **The result is one field.** Each update replaces it whole, and only the
  latest version is kept; the Scratchpad holds the history.
- **An `ended` task rejects notes and artifact registrations**, with a hint
  to set it back to `in progress` first. An upload registered before the
  task ended can still be reported complete. Its title, result and status
  can still change, and it can be set back to `in progress`.
- **Notes and artifacts belong to the task**, not to the agent that wrote
  them. When another agent of the same person continues a task, the task
  still names the agent that created it, and its notes and artifacts are not
  marked with the agent that wrote them.
- **Finding tasks.** A space's tasks can be filtered by person, agent and
  status, and searched by keyword in titles and results. An agent is matched
  by the name copied onto its tasks, which is unique among one person's
  agents; see
  [Library architecture](../architecture/library.md#61-caller).
  An agent authorized again under the same name is filtered together with
  its earlier tasks.

## 4. Scratchpad

The Scratchpad records how the agent did the work. It holds notes only.

- **A note** is a short Markdown entry with a time, rendered as Markdown.
  Times strictly increase within a task, so a note's time names it. A
  task's Scratchpad is its notes in time order.
- **The Scratchpad is append-only.** A note cannot be edited. A person may
  delete single notes in their own tasks; agents cannot delete notes.
- **One tool for every harness.** The library offers one MCP tool for writing
  notes. The tool serves every harness.
- **Notes belong to the agent's current task.**

## 5. Artifacts

Artifacts are files, often large, such as reports, images, exported
documents or data.

- **Uploads take two steps.** The agent gets a short-lived upload URL and
  the headers to send with it, through MCP or a product's entry point, then
  transfers the file over HTTP. Downloads work the same way.
- **Each upload URL writes one file.** The URL can be reused until it
  expires, but only one upload succeeds:
  - an upload without the returned headers is rejected with 403;
  - once a file is uploaded, another upload to the same URL is rejected with
    412, so a 412 means the file is already in storage;
  - a failed or interrupted upload leaves no file, so the client may retry
    with the same URL.
- **An upload is complete only when the client reports it.** Until then the
  artifact is not listed among the task's artifacts, and no download URL is
  issued for it. A report before any file is in storage is refused with
  `artifact.not_uploaded`.
- **An oversized file is refused when the upload is reported complete,
  and the refusal is final.** When the file in storage is over the size
  limit the product sets, the report is refused with `artifact.too_large`
  and the file is deleted. The artifact is never listed, and every later
  report of it is refused with `artifact.refused`, even after a new upload
  to its URL; to upload a smaller file, the client registers a new
  artifact.
- **Each artifact has a file name and a one-sentence summary.**
- **Artifacts are immutable.** To update one, upload a new one.
- **How browsers show artifacts** is in
  [Storage](../architecture/storage.md#3-file-storage).
- **An artifact belongs to the agent's current task.** Opening a task shows
  its Scratchpad and artifacts.
- **Deleting.** A person may delete their own artifacts; agents cannot.
