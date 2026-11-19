# Knowledge

- **Status:** Proposed
- **Level:** Contract
- **Parent:** [Library](../product/library.md)
- **Related:** [Access control](access-control.md),
  [Data model](../architecture/data-model.md)

> Only owners and admins change Knowledge directly. Every other change is a
> proposal that takes effect only when a person approves it.
>
> Agents read only the latest version of a knowledge entry.

## 1. Purpose

Knowledge carries what later sessions need, so each new session doesn't
start from nothing. Because every agent in the space reads it, changes stay
under people's control and every version is kept.

## 2. Scope

This doc covers knowledge entries, their versions, and proposals. When agents
read Knowledge, what belongs in it, and when agents propose changes are left
to the skill a product ships.

It does not cover proposals for anything other than Knowledge, such as
decisions an agent needs from a person in the middle of its work.

## 3. Knowledge entries

- **An entry is Markdown text** with a name, shaped like a file path, and a
  one-sentence summary. Knowledge belongs to the whole space, not to one
  person.
- **Names.** A name is levels separated by `/`, such as `deploy/staging.md`,
  with no `/` at the start or end and no empty level. A level may not be
  `.` or `..`, hold a control character, or start or end with whitespace. An
  extension is only part of the name: `guide.md` and `guide.txt` are two
  entries, and both show as Markdown.
- **Names ignore case.** They are compared ignoring case and shown as
  written, so `Deploy.md` and `deploy.md` are the same name, and reading
  `deploy.md` finds `Deploy.md`. This keeps one topic from splitting into two
  entries that drift apart.
- **Finding entries.** Entries are listed level by level by name prefix, and
  searched by keyword across names, summaries and content. Levels that differ
  only in case are one level, shown as the entry with the lowest key spells
  it.
- **Every version is kept.** A content version stores the content and the
  summary; a deletion version stores neither. People view the history and
  roll back.
- **Agents read only the latest version.** Older versions are kept, but
  agents cannot read them.
- **Rolling back creates a new version** whose content and summary equal the
  chosen old version; the versions in between stay. Rolling back is a change:
  owners and admins roll back directly, and a contributor's rollback is a
  proposal whose content is the old version.
- **Creating, changing and deleting an entry go through a proposal**, except
  for owners and admins; see [section 4](#4-proposals).
- **Deleting.** A deletion is recorded as a version, with who deleted the
  entry and when. The content stays in earlier versions, and rolling back
  restores it. Deleted entries are listed separately for people, who can roll
  them back; agents don't see them when listing or searching.
- **Renaming** is a change: owners and admins rename directly, and anyone else
  includes the new name in a change proposal. A name must be unique within
  the space, deleted entries included: a direct rename to another entry's
  name is rejected, and a proposal that would rename to another entry's name
  cannot be approved. A rename may change only the case. Renaming keeps the
  key and the history, so references by key still resolve. Renaming an entry
  that is not deleted creates no version, and rolling back never restores an
  old name.

## 4. Proposals

```mermaid
flowchart TD
    agent["Agent"]
    contributor["Contributor"]
    admin["Owner or admin"]
    pending["Pending proposal"]
    approved["Approved"]
    rejected["Rejected"]
    applied["Change takes effect"]
    agent -- "submits" --> pending
    contributor --> pending
    pending -- "a person approves" --> approved
    pending -- "a person rejects" --> rejected
    approved --> applied
    admin -- "changes directly" --> applied
```

- **A proposal** asks to create, change or delete one knowledge entry, and
  takes effect only when approved. Its status is `pending`, `approved` or
  `rejected`.
- **Three kinds.** A create carries a name, a summary and content. A change
  carries only the fields it changes, at least one of content, summary and
  name; the rest keep their current values. A delete carries no content.
- **A submitted proposal cannot be edited.** To revise it, a person of the
  proposing party rejects it, which withdraws it, and submits a new one.
- **Who must propose.** Everyone except owners and admins, people and agents
  alike, creates, changes and deletes entries through proposals. Changes by
  owners and admins take effect directly.
- **Who decides.** Owners, admins and contributors may approve and reject.
  Only people decide; agents can neither approve nor reject. Approval records
  who approved and when.
- **Approving one's own.** For approval, a person and that person's agents
  are one party. Owners and admins may approve their own party's proposals. A
  contributor may not, whether the contributor or one of their agents
  proposed it; another person must approve it. Rejecting is not limited: a
  person may reject their own party's proposal, which withdraws it, and must
  give a reason.
- **Changed since proposed.** If someone changes the entry's content or
  summary after the proposal was made, the proposal can still be approved.
  The proposal is marked "changed since proposed", so a product shows the
  difference from the current content, and the approver decides. On approval the proposal's
  content wins; the change in between stays in the history and can be rolled
  back to. A rename in between is not marked, because renaming creates no
  version; if the proposal also renames the entry, its name replaces the
  newer one.
- **The same applies** when a create proposal's name already belongs to an
  entry, which approval then gives a new version, and when the target entry
  was deleted, which approval restores with the proposal's content as its new
  version. A content or summary that a change proposal leaves out then comes
  from the last version before the deletion.
- **Rejecting.** The person who rejects writes a reason. Rejected proposals
  are kept, with the content, the proposer, who rejected them and the reason,
  visible to everyone.
