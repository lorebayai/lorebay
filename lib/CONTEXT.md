# Glossary

Canonical terms for the library's docs and code. Use them exactly, and
don't introduce synonyms. Knowledge and Scratchpad are capitalized; the
other terms are written in lowercase in running text.

## Library

**Library**: The features every product shares: spaces, tasks, notes,
artifacts, Knowledge, proposals and the MCP tools, built into a product's
process as the `lorebay-lib-*` crates.

**Product**: A program built on the library. It holds people, how they sign
in, who belongs to which space, its own entry points and its deployment.
Lorebay is the first product.

**Caller**: Who calls an operation: a person, the agent when an agent calls,
and the scope.

**Scope**: The owner ID a caller carries. An operation answers a space of
another owner, and anything in it, as not found.

**Owner ID**: The product's ID of whom a space belongs to, stored on the
space. It is the scope, not a role.

## Spaces and roles

**Space**: An isolated workspace for one team, with its own tasks,
Knowledge and proposals. A product holds many spaces.

**Membership**: A person's role in one space, which the product records and
the library reads.

**Role**: What a person may do in one space: owner, admin, contributor or
viewer. A person's role differs from space to space.

## People and agents

**Person**: Someone a product signs in, known to the library only by the
product's ID.

**Agent**: A program that acts for one person, such as a harness, with a
name unique among that person's agents. Its role is its person's, capped
at contributor.

**Party**: A person together with all of that person's agents.

**Harness**: The program that runs a model and connects it to the MCP
tools, such as Claude Code.

**Session**: One agent conversation: one cloud run, or one conversation of a
local agent. A subagent's work belongs to its parent's session.

**Skill**: The agent skill a product ships. It tells agents when and how to
call the MCP tools.

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

**Primary key**: An application-generated UUIDv7 stored as canonical
lowercase, hyphenated text. It identifies an internal row and can break
ordering ties; it never defines business order and never reaches a caller.
A product receives a space's ID only to refer to the space in its own rows.

**Key**: A short identifier with a type prefix, such as `TK-12`, shown to
people and used in references. Keys of tasks, knowledge entries, artifacts
and proposals count up per space; a space's key carries its space number.

**Space number**: The permanent number in a space key, such as the 1 in
`SPACE-1`, which the product gives each space. It is separate from the
space's primary key.

**Task ID**: A random string that identifies a task in calls that write into
it. It is not a key.

**Reference**: A `#` followed by a key in text, such as `#KN-1`, which a
product shows as a link.
