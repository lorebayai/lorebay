## Skill conventions

- `lorebay/SKILL.md` is the skill Lorebay ships, in the Agent Skills format
  that Claude Code and Codex both load. Its front matter holds only `name`,
  which is `lorebay` and matches its directory, and `description`, at most
  1,024 characters.
- Any further file of the skill lives under `lorebay/` and is linked from
  `SKILL.md` by a relative path.
- The skill's text uses no harness-specific syntax, such as a tool namespace
  or a skill mention, so one copy serves every harness. It names the tools by
  their bare names, as [MCP tools](../lib/docs/contracts/mcp-tools.md#4-tools)
  lists them.
- `SKILL.md` states the rules of the library's
  [agent rules](../lib/docs/contracts/agent-rules.md) and Lorebay's
  [agent skill](../docs/contracts/agent-skill.md), and adds none. A change to
  a rule goes first into the contract that owns it, then into `SKILL.md`.
- `SKILL.md` is a shipped file that restates the two contracts on purpose,
  the one exception to "each fact has one home" of
  [documentation.md](../playbook/core/documentation.md#homes).
- The skill's own words are only its `name` and `description`. The
  description says the skill applies to every session in which Lorebay's MCP
  server is registered in the harness, as `lorebay`, reachable or not and
  whatever the first request, so a harness loads it without the person naming
  Lorebay.
- `SKILL.md` states the content rule first, in its own sentence, then the 13
  rules in session order, each with Lorebay's part beside the library's, so
  the split between the contracts does not show. Each requirement of a rule
  without a backstop has its own sentence.
