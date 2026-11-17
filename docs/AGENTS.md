# Docs

Rules for every design doc under `docs/`. Each topic folder's `AGENTS.md`
adds the rules for that folder.

## Placement

- `docs/` holds Lorebay's design, by topic: `product/`, `architecture/` and
  `contracts/`. Each design has one file, in the folder that owns it; other
  docs link to it.
- File names are kebab-case and name the topic, such as `access-control.md`.
- [overview.md](overview.md) lists every doc with one line. A change that
  adds, renames or deletes a doc updates it.

## Shape

Every doc starts with this header:

```markdown
# <Title>

- **Status:** Proposed | Current
- **Level:** System | Contract | Implementation
- **Parent:** [<doc>](<relative link>)
- **Related:** [<doc>](<relative link>), [<doc>](<relative link>)

> One to three rules that everything below must respect.
```

- **Status:** `Proposed` is agreed but not built; `Current` matches the code.
  The pull request that builds a design sets its doc to `Current`. A
  `Current` doc has no placeholders, open questions, unresolved
  alternatives, roadmaps or review logs.
- **Level:** the one level of abstraction the doc stays at:

  | Level | Covers | Leaves out |
  | --- | --- | --- |
  | System | Goals, boundaries, components, technology choices | Wire formats, table layouts |
  | Contract | Behavior observable from outside `lorebayd`: the web pages, the Web API, MCP tools, the `lorebay` CLI | Internal structure |
  | Implementation | Tables, modules and the rules they keep | Behavior a contract owns |

- **Parent:** only when the doc refines another doc.
- **Related:** direct dependencies and complementary docs only.

Then number the top-level sections, in this order:

1. **Purpose**: why the design exists.
2. **Scope**: what the doc covers, then a list of what it does not cover.
   Each feature Lorebay leaves out is listed once, in the doc that would own
   it.
3. The design, in as many sections as it needs.
4. **References**: the external sources a decision rests on, when there are
   any.

## Writing

- Lead each rule with its decision in bold, then give only the reason needed
  to understand it.
- State another doc's rule only as far as the context needs, and link to the
  section that owns it.
- Add a term to [CONTEXT.md](../CONTEXT.md) once more than one doc uses it.
- Link with relative paths, to a section anchor when citing one rule.
- Draw a Mermaid diagram when a structure, sequence or flow reads more
  clearly as a picture, and keep it in agreement with the prose. Label each
  node, edge and message with a few words and leave the details to the
  prose, so the diagram fits on one screen. Quote every edge label, as in
  `a -- "approve" --> b`.

## Review

- Review as a new reader, not as the author of the discussion behind the
  doc.
- Challenge every component, field, lock and config knob that no
  requirement demands.
- Reliability claims must cover a crash between every two durable steps.
- Fold accepted findings into the design itself; never append a review log.

## Before finishing

- Every relative link and anchor resolves.
- [overview.md](overview.md) lists every doc, with a line that still fits it.
- Terms match [CONTEXT.md](../CONTEXT.md), and no other doc contradicts the
  change.
- The Markdown renders: blank lines around blocks, a language on every fenced
  block, sequential heading levels, and Mermaid that renders in GitHub's rich
  diff.
