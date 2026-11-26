# Agent Skill

- **Status:** Proposed
- **Level:** Contract
- **Parent:** [Lorebay](../product/lorebay.md)
- **Related:** [Agent practices](../../lib/docs/contracts/agent-practices.md),
  [CLI](cli.md), [Access control](access-control.md)

> One skill serves every harness.
>
> Where the library has no backstop, Lorebay's skill is the only
> enforcement, so the skill must state those practices exactly.

## 1. Purpose

Lorebay records what its agents choose to send it. The practices an agent
follows so that those records make sense, and each practice's backstop, are
the library's [Agent practices](../../lib/docs/contracts/agent-practices.md).
The skill Lorebay ships states each of them, and adds Lorebay's part of
practices 1, 2, 5 and 9.

## 2. Scope

This doc covers Lorebay's part of the agent practices, by the library's
practice numbers, with what Lorebay adds to a practice's backstop, and what
depends on a harness loading the skill. Every practice, its library part and the
library's backstop are in the library's
[Agent practices](../../lib/docs/contracts/agent-practices.md).

It does not cover:

- installing the skill and connecting MCP: the skill installs as a plugin
  from this repository's marketplaces, and the person registers the MCP
  server by hand, as the [skill's README](../../skill/README.md) says;
- clients that can connect over MCP but cannot install a skill, such as some
  mobile apps: they are not supported.

## 3. Lorebay's part of the practices

Each row adds to the practice of the same number in the library's contract.
Practices 3, 4, 6 to 8 and 10 to 13 have no Lorebay part: the skill states
them as the library's contract does.

| # | Practice | Lorebay's part | Backstop |
| --- | --- | --- | --- |
| 1 | [**Choose the space**](../../lib/docs/contracts/agent-practices.md#3-session-start) | The project's local instructions that record the space key are its committed agent instructions file that the harness loaded, so the whole team shares the key: both `AGENTS.md` and `CLAUDE.md` when the project has them as separate files, and the file the harness loads when the project has neither. If the person is in no space, tell the person to create a space with `lorebay space create`, or to ask a space's owner or an admin to add them; see [CLI](cli.md#3-commands) and [Access control](access-control.md#3-space-members). | None |
| 2 | [**Create the task**](../../lib/docs/contracts/agent-practices.md#3-session-start) | In the cases with no task, when a task was already created: if the person may delete it, tell them to run `lorebay task delete <space> <task-id>`, giving its space key and its task ID, not its key such as `TK-12` (see [CLI](cli.md#3-commands)); if they may no longer delete it, tell them once that the rest of the session goes unrecorded, that they can no longer delete the task, and to ask an owner or an admin of the space to delete it, naming the task by its key, and give no delete command. | Partial, as the [library's practice](../../lib/docs/contracts/agent-practices.md#3-session-start) says |
| 5 | [**Write checkpoints**](../../lib/docs/contracts/agent-practices.md#4-while-working) | The command in the backstop. | None; a person can delete a checkpoint that holds a secret with `lorebay checkpoint delete`; see [CLI](cli.md#3-commands) |
| 9 | [**Keep working when the MCP server is unreachable**](../../lib/docs/contracts/agent-practices.md#4-while-working) | After an unauthorized answer, also suggest that the person re-authorize Lorebay's MCP server in the harness. | None |

## 4. Loading the skill

- **Practice 9's notice depends on the harness loading the skill, and Lorebay
  accepts that limitation** in Claude Code and Codex. Only the skill states
  practice 9, and whether a harness opens the skill is the model's choice.
  Codex was seen not to open it while the server's authorization had
  stopped working, so the person was not told that the work went
  unrecorded. No harness mechanism that loads a skill at session start is
  verified for both harnesses, so Lorebay ships none; the
  [skill's README](../../skill/README.md) states the limitation and how to
  re-authorize.
