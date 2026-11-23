# Glossary

Canonical terms for the library's docs and code. Use them exactly, and
don't introduce synonyms. Knowledge and Scratchpad are capitalized; the
other terms are written in lowercase in running text.

## Library

**Library**: The features every product shares: spaces, tasks, notes,
files, Knowledge, proposals and the MCP tools, built into a product's
process as the `lorebay-lib-*` crates.

**Product**: A program built on the library. It holds people, how they sign
in, who belongs to which space, its own entry points and its deployment.
Lorebay is the first product.

**Operation**: One thing the library does for a caller, such as creating a
task, run as one read or one write.

**Interface**: One of the five traits a product implements for the library:
caller, person names, space roles, database and file storage.

**Entry point**: A way callers reach the library's operations, such as the
MCP layer or a product's own Web API.

**MCP layer**: The library's entry point that serves its operations as MCP
tools, inside the product's process.

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

**Member**: A person with a membership in a space.

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

**Skill**: The agent skill a product ships. It states the agent rules, and
tells agents when and how to call the MCP tools.

**Agent rules**: The rules an agent follows so that the records it writes
through the MCP tools make sense, each with its backstop.

**Backstop**: What the library blocks or detects when an agent breaks an
agent rule.

## Data

**Task**: The record of one piece of work an agent does for a person in a
space: its title, status, result, Scratchpad and files.

**Result**: A task's Markdown summary of the work so far.

**Note**: A short, timestamped Markdown entry an agent appends to a task
while it works.

**Scratchpad**: A task's notes, in time order.

**File**: Immutable content an agent uploads to a task, with a name and a
summary.

**Refused registration**: A file registration whose content was over the
size limit when its upload was reported complete. It never completes; the
client registers a new file instead.

**Knowledge**: A space's shared Markdown documents, kept with their history.

**Knowledge entry**: One named document in Knowledge, with a summary and its
versions.

**Knowledge version**: One stored state of a knowledge entry: its content and
summary, or its deletion.

**Proposal**: A request to create, change or delete a knowledge entry. It
takes effect only when a person approves it.

**Cleanup round**: A library function with no caller that a product runs on
its own schedule to remove what interrupted writes leave behind. There are
two: the abandoned-registration round and the orphaned-file round.

**Abandoned-registration round**: The cleanup round that removes file
registrations neither completed nor refused, and their stored content.

**Orphaned-file round**: The cleanup round that deletes stored content that no
record points to, other than a refused registration.

## Identifiers

**Primary key**: An application-generated UUIDv7 stored as canonical
lowercase, hyphenated text. It identifies an internal row and can break
ordering ties; it never defines business order and never reaches a caller.
A product receives a space's ID only to refer to the space in its own rows.

**Key**: A short identifier with a type prefix, such as `TK-12`, shown to
people and used in references. Keys of tasks, knowledge entries, files
and proposals count up per space; a space's key carries its space number.

**Space number**: The permanent number in a space key, such as the 1 in
`SPACE-1`, which the product gives each space. It is separate from the
space's primary key.

**Task ID**: A random string that identifies a task in calls that write into
it. It is not a key.

**Reference**: A `#` followed by a key in text, such as `#KN-1`, which a
product shows as a link.
