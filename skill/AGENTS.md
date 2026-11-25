Follow these playbook fragments:

- @README.md
- @../playbook/topics/testing.md

## Skill conventions

- `lorebay/SKILL.md` is the skill Lorebay ships, in the Agent Skills format
  that Claude Code and Codex both load. Its front matter holds only `name`,
  which is `lorebay` and matches its directory, and `description`, at most
  1,024 characters.
- Any further file of the skill lives under `lorebay/` and is linked from
  `SKILL.md` by a relative path.
- The skill installs as the plugin `lorebay`, which this folder is.
  `skill:plugin` checks that the marketplaces
  `../.claude-plugin/marketplace.json` for Claude Code and
  `../.agents/plugins/marketplace.json` for Codex each list only it, that its
  manifests `.claude-plugin/plugin.json` and `.codex-plugin/plugin.json` are
  identical and point `skills` at `./lorebay`, and that none of the four has
  a `version`, so each harness's update follows the ref the person
  installed, or declares an MCP server.
- Plugin files stay out of `lorebay/`.
- The skill's text uses no harness-specific syntax, such as a tool namespace
  or a skill mention, so one copy serves every harness. It names the tools by
  their bare names, as [MCP tools](../lib/docs/contracts/mcp-tools.md#4-tools)
  lists them.
- `SKILL.md` states the practices of the library's
  [agent practices](../lib/docs/contracts/agent-practices.md) and Lorebay's
  [agent skill](../docs/contracts/agent-skill.md), and adds none. A change to
  a practice goes first into the contract that owns it, then into `SKILL.md`.
- `SKILL.md` is a shipped file that restates the two contracts on purpose,
  the one exception to "each fact has one home" of
  [documentation.md](../playbook/core/documentation.md#homes).
- The skill's own words are only its `name` and `description`. The
  description says the skill applies to every session in which Lorebay's MCP
  server is registered in the harness, as `lorebay`, reachable or not and
  whatever the first ask, so a harness loads it without the person naming
  Lorebay.
- `SKILL.md` states the content practice first, in its own sentence, then
  the 13 practices in session order, each with Lorebay's part beside the
  library's, so the split between the contracts does not show. It states
  every requirement of a practice without a backstop, since only the skill
  enforces those practices.
- `SKILL.md` states each requirement once, in the fewest words that keep it
  exact. It repeats no sentence of its own and no tool description, and
  explains only what an agent needs to act.
