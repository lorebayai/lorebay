# Agent Skill

- **Status:** Proposed
- **Level:** Contract
- **Parent:** [Lorebay](../product/lorebay.md)
- **Related:** [Agent rules](../../lib/docs/contracts/agent-rules.md),
  [CLI](cli.md), [Access control](access-control.md)

> One skill serves every harness.
>
> Where the library has no backstop, Lorebay's skill is the only
> enforcement, so the skill must state those rules exactly.

## 1. Purpose

Lorebay records what its agents choose to send it. The rules an agent
follows so that those records make sense, and each rule's backstop, are the
library's [Agent rules](../../lib/docs/contracts/agent-rules.md). The skill
Lorebay ships states each of them, and adds Lorebay's part of rules 1, 2, 5
and 9.

## 2. Scope

This doc covers Lorebay's part of the agent rules, by the library's rule
numbers, with what Lorebay adds to a rule's backstop. Every rule, its
library part and the library's backstop are in the library's
[Agent rules](../../lib/docs/contracts/agent-rules.md).

It does not cover:

- packaging the skill and the MCP connection as a plugin or a marketplace
  listing: people install the skill and connect MCP by hand;
- clients that can connect over MCP but cannot install a skill, such as some
  mobile apps: they are not supported.

## 3. Lorebay's part of the rules

Each row adds to the rule of the same number in the library's contract.
Rules 3, 4, 6 to 8 and 10 to 13 have no Lorebay part: the skill states
them as the library's contract does.

| # | Rule | Lorebay's part | Backstop |
| --- | --- | --- | --- |
| 1 | [**Choose the space**](../../lib/docs/contracts/agent-rules.md#3-session-start) | The project's local instructions that record the space key are its committed agent instructions file that the harness loaded, so the whole team shares the key: both `AGENTS.md` and `CLAUDE.md` when the project has them as separate files, and the file the harness loads when the project has neither. If the person is in no space, tell the person to create a space with `lorebay space create`, or to ask a space's owner or an admin to add them; see [CLI](cli.md#3-commands) and [Access control](access-control.md#3-participants). | None |
| 2 | [**Create the task**](../../lib/docs/contracts/agent-rules.md#3-session-start) | In the cases with no task, the person deletes a task already created with `lorebay task delete`; see [CLI](cli.md#3-commands). | Partial, as the [library's rule](../../lib/docs/contracts/agent-rules.md#3-session-start) says |
| 5 | [**Write notes**](../../lib/docs/contracts/agent-rules.md#4-while-working) | The command in the backstop. | None; a person can delete a note that holds a secret with `lorebay note delete`; see [CLI](cli.md#3-commands) |
| 9 | [**Keep working when the MCP server is unreachable**](../../lib/docs/contracts/agent-rules.md#4-while-working) | After an unauthorized answer, also suggest that the person re-authorize Lorebay's MCP server in the harness. | None |
