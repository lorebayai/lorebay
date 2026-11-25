# Changelog

Changes that a product depending on the library can observe, for the four
crates `lorebay-lib-types`, `lorebay-lib-core`, `lorebay-lib-migration` and
`lorebay-lib-mcp`. They share one version, and a release is tagged
`lib-v<version>`.

## Unreleased

- **Breaking:** The library's created migration is named
  `m20261125_000001_create_space_tables`. Databases recording
  `m20261118_000001_create_space_tables` must be recreated.

- **Breaking:** Requests support `revise_person_request` and MCP
  `revise_request`, keeping the key and only the latest revision.
  `revise_person_request` returns `RevisedPersonRequest`, containing the
  revised `PersonRequest` and `was_outdated` for its state before revision;
  MCP revision results also expose `was_outdated`. Reads and
  lists expose `revision`, `outdated`, submitting and last revising agents,
  and revision attribution; `outdated` replaces `changed_since_requested`.
  Approval takes the revision read; `PersonRequestRejection` also requires
  it. Stale decisions return `PersonRequestError::RevisionChanged`; outdated
  approval returns `PersonRequestError::Outdated`. Direct changes, deletion
  and rollback take a `VersionedKnowledgeTarget` and refuse stale versions
  with `KnowledgeError::VersionChanged`. The created migration adds request
  snapshots and revision attribution in place; databases must be recreated.
  `WriteOperation` gains `RevisePersonRequest`; `PersonRequest` gains
  `base_name`; `PersonRequestError` gains `InvalidRevision` and
  `KnowledgeInputError` gains `Version`.

- **Breaking:** Each task result write adds a numbered version with its
  agent, person and time. Result updates name the `result_version` they replace
  and return the written version; a mismatch is `task.result_changed`. Task
  reads include `result_version` and optionally paged result history. The
  created migration adds `space_task.result_version` and `space_task_result`
  in place; databases must be recreated. `read_task` takes a `TaskInclude` for
  the Scratchpad or result history; `read_task_results` reads the history
  alone; `TaskChanges` gains `result_version` and `TaskError` gains
  `ResultChanged`.

- **Breaking:** Checkpoints and files expose the writing or registering
  agent's copied `agent_name`. Tasks expose `state_changed_by`,
  `state_changed_agent_name` and `state_changed_at` for the current state,
  including creation and request-driven transitions. The created migration
  adds these columns in place; databases must be recreated.

- **Breaking:** Tasks have six states, `planned`, `working`, `waiting`, `done`,
  `failed` and `canceled`, with reasons and one holder. Claims, same-party
  takeovers and releases enforce exclusive writes; Requests may wait for a
  decision. Task fields use `state` and `state_reason`, MCP adds `claim_task`
  and `release_task`, and people plan or cancel tasks. Repeating the current
  state without a reason preserves its fields. A started task's first release
  records its planner for later cross-party claims. The created migration
  changes in place; databases must be recreated.

- The library's recording contract is named agent practices, with 13
  numbered practices. The practices and their backstops keep their content.

- **Breaking:** Knowledge changes awaiting a person's decision become
  requests throughout the library API and MCP, with `PersonRequest` code
  names, `action`, `requested_by`, `requester_agent_name`,
  `changed_since_requested` and history's `request_key`, `person_request.*`
  errors, `RQ-` keys and the rewritten `space_person_request` schema. The
  created migration changes in place; a database that applied it must be
  recreated.

- **Breaking:** Task notes become checkpoints throughout the core API and
  MCP tools, with `Checkpoint` code names, `add_checkpoint`,
  `checkpoint.not_found` errors and the rewritten `space_checkpoint` schema.
  The created migration changes in place; a database that applied it must
  be recreated.

- **Breaking:** Task uploads become files throughout the library API, MCP
  tools and task fields, with `AgentFile` code names, `agent_file.*` errors,
  `FL-` keys and the rewritten `space_agent_file` schema. The created
  migration `m20261125_000001_create_space_tables` changes in place; a
  database that applied it must be recreated.

- **Breaking:** A product sets `tenant_id` on `Caller`, and its space-roles
  implementation reads `MembershipPage.tenant_id`; the `space` column
  becomes `tenant_id`. The created migration changes in place, and tenant
  isolation checks keep their behavior.

Version 0.1.0, the first version, not yet tagged.

- MCP adds `list_knowledge`, `read_knowledge`, `create_proposal`,
  `list_proposals` and `get_proposal`, completing the 14-tool catalog.
  Every MCP request observes a body limit derived from
  `KNOWLEDGE_FIELDS_MAX_JSON_BYTES`, plus allowances for a target name and
  the JSON-RPC envelope, tool name and task ID, so proposals accept 1 MiB
  of content even when every byte is JSON-escaped.
- Core adds paginated Knowledge history and deleted-entry lists, old-version
  reads with `knowledge.version_not_found`, and direct rollback for owners
  and admins. The new result types are `KnowledgeVersion`, with an optional
  `proposal_key`, and `DeletedKnowledgeEntry`.
  Rollback records `WriteOperation::RollBackKnowledgeEntry`,
  appends a content version, restores deleted entries and keeps their names.
- Core adds Knowledge proposal submission, paginated status and keyword lists,
  reads, approval and rejection, with task titles on agent submissions.
  Decisions record the approver and source proposal in each new version;
  writes record their own operations. Errors include `proposal.not_found`,
  `proposal.decided` and `proposal.own_party`.
- Core adds direct Knowledge create, change and delete operations,
  latest-version reads and paginated prefix or keyword lists, with
  separate name and key inputs. Writes record their own operations;
  errors include `knowledge.not_found` and `knowledge.name_taken`.
  `KNOWLEDGE_FIELDS_MAX_JSON_BYTES` exports the maximal JSON string
  payload derived from the entry input limits for entry points.
- **Breaking:** `Space` gains `caller_person`, the name of the caller's
  person, on create, list, read and change results. `list_spaces` returns
  the same field, accepted as `person` by `list_tasks`.
- **Breaking:** core writes use the composable `run_write` entry without a
  caller argument. Each operation records its own caller, and `create_space`
  takes the caller instead of an independent owner scope.
- **Breaking:** `register_artifact` takes `&mut Write` and returns an
  `ArtifactRegistration`; call `issue_upload` only after commit. Completion
  uses `prepare_artifact_completion` before the write and `complete_artifact`
  inside it, then resolves `ArtifactCompletion::into_result` after commit.
  `remove_abandoned_artifacts` also takes `&mut Write`.
- **Breaking:** an oversize completion commits a final refusal and queues
  file deletion after commit. Its response remains `artifact.too_large`,
  including when deletion fails; later reports return `artifact.refused`
  and require a new registration. Refused files qualify for orphan cleanup.
  Abandoned-registration cleanup retains refused registrations until task
  deletion.
- **Breaking:** the initial artifact migration includes `refused_at` and
  forbids a registration from being both completed and refused. Rebuild
  databases made before this untagged first release.
- **Breaking:** `DatabaseTransactions::commit_write` receives an optional
  write record. A repeated successful completion commits without a record;
  a newly refused completion records `WriteOperation::RefuseArtifact`.
- **Breaking:** public core error enums, `WriteOperation`, `LibrarySettings`
  and `LibraryInterfaces` are non-exhaustive; construct settings and
  interfaces with their builders and handle future enum variants.
- **Breaking:** `UnixMillisError` in types is non-exhaustive; handle future
  variants.
- **Breaking:** `TaskError` and `ArtifactError` route space access failures
  only through `TaskAccessError`; their outer `Space` variants are removed.
- **Breaking:** `TaskFilters::cursor_version` is private to the core domain.
- **Breaking:** migration's `sqlite` feature selects only the driver. Enable
  `entity-generation` for `open_entity_database` and its development binary.
- **Breaking:** the agent rules a product's skill restates change. Rule 5
  states the recording order, with a finished-step note even for a one-step
  task. Rule 6 uploads each file the work produces for the person to take
  away, also an unpushed one inside a Git folder, and the upload matches the
  final file.
  Rule 9 records only new work after re-authorization, with no task for the
  unrecorded stretch.
- **Breaking:** the agent rules a product's skill restates change. When the
  person asks for more on the same work after the agent ended its task in
  the same session, rule 12 reopens that task instead of creating one, also
  while another task is `in progress`, which ends first. Rule 3 lets the agent
  set it back, and its search for a lost task ID includes a task the agent
  ended in this session. Rule 12's backstop says the choice is not checked.
- **Breaking:** the agent rules a product's skill restates change. Rule 6
  uploads no change to a repository's own content, a file tracked there or
  meant to be committed, even when it is unpushed or the person calls it the
  deliverable: commits and pull requests deliver it, the agent commits or
  pushes only as the person or the repository's rules ask, and until then
  the result names the repository, branch and changed files.
- **Breaking:** agent rule 5 defines a step as each thing the person asks
  for, a check included, and each further step the plan names. Each
  reopening starts with a plan, which replaces the reopening note, and a
  continued task's work gets a step's note. Rule 13 records the fix by rule
  5. Rule 6's backstop names a repository change as unchecked.
- **Breaking:** the agent rules a product's skill restates change. Rules 4
  and 11 no longer skip Knowledge. Rule 4 reaches entries by name or by
  keyword search, and reads the rest of an entry a result holds only part of
  before relying on it. Rule 11 proposes after the result is written and the
  person confirms, from an `ended` task without reopening it, then adds the
  proposal's reference to Deliverables. A change the person asks for by name
  counts as confirmed, a session with no task tells the person instead, and
  an entry not read in full gets no content change.
  Rule 8 never proposes repository content, and rule 5's order places the
  proposal after the result.
- **Breaking:** the agent rules a product's skill restates change. Rule 11
  looks for a `pending` or `rejected` proposal about the same thing before
  the agent says anything to the person about a proposal. For a `pending`
  match the agent tells the person it already awaits approval and offers
  nothing; for a `rejected` one it tells the person the reason, and the
  person decides whether to propose again.
- Core parses stored role names through `SpaceRole::try_from(&str)` with
  `SpaceRoleError` for invalid names.
- Entity generation uses separate installer build directories for the two
  migration groups, allowing both generators to start together.
- `lorebay-lib-types`: the secret type, `sha256_hex`, the cursor codec and
  the conversion from `SystemTime` to Unix milliseconds.
- `lorebay-lib-core`: the library state and its settings, the five
  interfaces a product implements, the function that runs one write, one
  function per operation on spaces, tasks, notes and artifacts, the two
  cleanup rounds, and the reads of the highest space number and of a space
  key. The `sqlite` and `postgres` features select the database driver.
- `lorebay-lib-migration`: the library's `Migrator`, with its own `sqlite`
  and `postgres` features.
- `lorebay-lib-mcp`: the router of the MCP tools, which forwards the same
  two features to core.
