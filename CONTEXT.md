# Glossary

Canonical terms for Lorebay's docs and code. Use them exactly, and don't
introduce synonyms. Knowledge and Scratchpad are capitalized; the other terms
are written in lowercase in running text.

## Product

**Lorebay**: The product, and one deployment of it: a single `lorebayd`
process with its data, its web pages, its Web API and its MCP server.

**`lorebayd`**: The Lorebay server process.

**Space**: An isolated workspace for one team, with its own participants,
tasks, Knowledge and proposals. A Lorebay holds many spaces.

**Participant**: A human or an agent in a space. Each participant has a type
and a role.

**Role**: What a participant may do in one space: owner, admin, contributor
or viewer. A person's role differs from space to space.

**Users file**: The file of usernames and passwords that `lorebayd` reads at
startup. It defines who can log in.

## Agents

**Agent**: One OAuth authorization a person grants to a client, such as
Claude Code. An agent acts for that person. External programs that call the
Web API are agents too.

**Party**: A person together with all of that person's agents.

**Harness**: The program that runs a model and connects it to Lorebay over
MCP, such as Claude Code.

**Session**: One agent conversation: one cloud run, or one conversation of a
local agent. A subagent's work belongs to its parent's session.

**Skill**: The agent skill shipped with Lorebay. It tells agents when and how
to call the MCP tools.

## Data

**Task**: The record of one piece of work an agent does for a person in a
space: its title, status, result, Scratchpad and artifacts.

**Result**: A task's Markdown summary of the work so far.

**Note**: A short, timestamped Markdown entry an agent appends to a task
while it works.

**Scratchpad**: A task's notes, in time order.

**Artifact**: An immutable file an agent uploads to a task.

**Knowledge**: A space's shared Markdown documents, kept with their history.

**Knowledge entry**: One named document in Knowledge, with a summary and its
versions.

**Knowledge version**: One stored state of a knowledge entry: its content and
summary, or its deletion.

**Proposal**: A request to create, change or delete a knowledge entry. It
takes effect only when a person approves it.

## Identifiers

**Key**: A short identifier with a type prefix, such as `TK-12`, shown to
people and used in references. Keys of tasks, knowledge entries, artifacts
and proposals count up per space; space keys count up across the Lorebay.

**Task ID**: A random string that identifies a task in calls that write into
it. It is not a key.

**Reference**: A `#` followed by a key in text, such as `#KN-1`, which pages
show as a link.
