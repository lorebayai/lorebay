# Code Comments

When code gets a comment, and how the comment reads. Applies to code in every
language, including tests, scripts and config files.

## Default: no comment

Code explains itself: names say what things are and do, types carry the
constraints, and small functions show the steps. When code needs a comment to
be understood, rename, extract or restructure it instead.

## Rationale

A comment states **rationale**: why the code is this way, when the code
cannot show it.

- A constraint from outside the code: a protocol rule, a vendor limit, a
  security requirement.
- A choice that looks wrong but is deliberate, with its reason.
- A workaround, with its cause and a link to the upstream issue.
- An invariant or ordering the code relies on but cannot enforce.

Test every comment: if a competent reader would understand the code without
it, and would not "fix" it wrongly, delete it.

## Form

- One line by default, two at most. Rationale that needs more belongs in a
  design doc; link to it.
- Lead with the reason, in plain present tense. Cut filler such as "Note
  that", "This function", "We" and "In order to".
- Put the comment on the line above the code it explains.

```rust
// S3 refuses to overwrite an object only when the presigned URL signs `If-None-Match: *`.
let headers = SignedHeaders::from([(IF_NONE_MATCH, "*")]);
```

```ts
// iOS Safari never fires `beforeunload`; `pagehide` is the reliable exit event.
window.addEventListener("pagehide", saveDraft);
```

## Doc comments

Doc comments on public items state the contract the name and types cannot:
units, allowed values, error cases, side effects. One line by default.

## Homes for everything else

| Content | Its home |
| --- | --- |
| How the code works, step by step | The code: names, types, small functions |
| Restated parameters and return types | The signature |
| Step labels such as `// Arrange`, `// Act`, `// Step 1` | Blank lines between the steps |
| What changed: "now", "previously", "updated to", "fixed" | The commit message and the pull request |
| Notes to the reviewer or the user | The pull request |
| Design and trade-offs | A design doc |
| Code that is no longer used | Git history: delete the code |
| Planned work | A GitHub issue, with a `TODO(#<number>):` comment pointing to it |
| Section banners and dividers | A separate module or file |

## Editing

- A change that makes a comment wrong fixes or deletes it in the same change.
- A change adds no comment about itself.
- Moving code moves its rationale comments with it.
