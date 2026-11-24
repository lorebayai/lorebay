# Glossary

Canonical terms for the library's docs and code. Use them exactly, and
don't introduce synonyms. Knowledge and Scratchpad are capitalized; the
other terms are written in lowercase in running text.

## Library

**Library**: The features every product shares: spaces, tasks, checkpoints,
files, Knowledge, requests and the MCP tools, built into a product's
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

**Caller**: Who calls an operation: the actor and its tenant.

**Actor**: The identity that performs an operation: a person, or an agent
acting for its sponsor.

**Tenant**: The unit that owns data. The product passes its tenant on every
call, and an operation answers a space of another tenant, and anything in
it, as not found.

**Tenant ID**: The product's ID of the tenant a space belongs to, stored on
the space. It is the tenant, not a role.

## Spaces and roles

**Space**: An isolated workspace for one team, with its own tasks,
Knowledge and requests. A tenant holds many spaces.

**Space membership**: A person's space role in one space, which the product
records and the library reads.

**Space member**: A person with a space membership.

**Space role**: What a person may do in one space: space owner, space admin,
space contributor or space viewer. A person's space role differs from space
to space.

## People and agents

**Person**: Someone a product signs in, known to the library only by the
product's ID.

**Agent**: A program that acts for its sponsor, such as a harness, with a
name unique among its sponsor's agents; a sponsor can have several. Its
space role is its sponsor's, capped at contributor.

**Sponsor**: The person an agent acts for.

**Party**: A person together with all of the agents they sponsor.

**Harness**: The program that runs a model and connects it to the MCP
tools, such as Claude Code.

**Session**: One agent conversation: one cloud run, or one conversation of a
local agent. A subagent's work belongs to its parent's session.

**Skill**: The agent skill a product ships. It states the agent practices,
and tells agents when and how to call the MCP tools.

**Agent practices**: The practices an agent follows so that the records it
writes through the MCP tools make sense, each with its backstop.

**Backstop**: What the library blocks or detects when an agent breaks an
agent practice.

## Data

**Task**: The record of one piece of agent work in a space: its title, state,
holder, result, Scratchpad and files. A task may be planned before an agent
works on it.

**Result**: A task's Markdown summary of the work so far: the task result.

**Task state**: Where a task stands: `planned`, `working`, `waiting`, `done`,
`failed` or `canceled`. The library holds it and refuses any other change.

**Planned**: The task state of a task planned for later, which no agent holds
yet. Any agent in the space, except a space viewer's, may claim it.

**Working**: The task state of a task its holder works on.

**Waiting**: The task state of a held task whose work waits on a person, such as
an answer or a request's decision, with a reason that says on whom.

**Done**: The task state of a task its holder reports done. It is not accepted
or reviewed, and it can be reopened.

**Failed**: The final task state of a task finished without doing the work, with
a reason.

**Canceled**: The final task state of a task deliberately not done, with a
reason.

**Holder**: The one agent that holds a `working` or `waiting` task, and the only
one that writes into it.

**Claim**: Taking hold of a task: of a `planned` one, which moves the task to
the claiming agent's party, or of one another agent of the same party holds,
which is a takeover.

**Takeover**: A claim by an agent of the holder's party, which moves the hold to
it and keeps the task's state.

**Release**: Giving up the hold of a `working` task before any work is recorded
on it, which sets it back to `planned`.

**Planner**: The person or agent that planned a task, or the holder recorded as
planner on its first release.

**Write record**: What the library hands the product's commit hook for one
committed write: the operation, its caller and its space.

**Checkpoint**: A short, timestamped Markdown entry an agent appends to a task
while it works.

**Scratchpad**: A task's working record while it runs: its checkpoints in time
order. Each product presents it its own way.

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

**Request**: A record of a change that waits on a person's decision. The
library has one kind, the Knowledge-change request, to create, change or
delete a knowledge entry; it takes effect only when a person approves it.

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
and requests count up per space; a space's key carries its space number.

**Space number**: The permanent number in a space key, such as the 1 in
`SPACE-1`, which the product gives each space. It is separate from the
space's primary key.

**Task ID**: A random string that identifies a task in calls that write into
it. It is not a key.

**Reference**: A `#` followed by a key in text, such as `#KN-1`, which a
product shows as a link.
