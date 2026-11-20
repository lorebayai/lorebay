# Changelog

Changes that a product depending on the library can observe, for the four
crates `lorebay-lib-types`, `lorebay-lib-core`, `lorebay-lib-migration` and
`lorebay-lib-mcp`. They share one version, and a release is tagged
`lib-v<version>`.

## Unreleased

Version 0.1.0, the first version, not yet tagged.

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
  Abandoned-registration cleanup retains refused registrations until task deletion.
- **Breaking:** the initial artifact migration includes `refused_at` and
  forbids a registration from being both completed and refused. Rebuild
  databases made before this untagged first release.
- **Breaking:** `DatabaseTransactions::commit_write` receives an optional
  write record. A repeated successful completion commits without a record;
  a newly refused completion records `WriteOperation::RefuseArtifact`.
- **Breaking:** public core error enums, `WriteOperation`, `LibrarySettings`
  and `LibraryInterfaces` are non-exhaustive; construct settings and
  interfaces with their builders and handle future enum variants.
- **Breaking:** `TaskError` and `ArtifactError` route space access failures
  only through `TaskAccessError`; their outer `Space` variants are removed.
- **Breaking:** `TaskFilters::cursor_version` is private to the core domain.
- **Breaking:** migration's `sqlite` feature selects only the driver. Enable
  `entity-generation` for `open_entity_database` and its development binary.
- **Breaking:** the agent rules a product's skill restates change. Rule 5
  states the recording order, with a finished-step note even for a one-step
  task. Rule 6 uploads each file the work produces for the person unless its
  final content is in a pushed commit or a pull request, so a committed but
  unpushed file is now uploaded, and the upload matches the final file.
  Rule 9 records only new work after re-authorization, with no task for the
  unrecorded stretch.
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
