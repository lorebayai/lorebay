# Testing

How `lorebayd` and the product are tested: the three test levels, what each
one verifies, and where its tests live. Imported by the `AGENTS.md` of every
Rust crate and test suite.

## Levels

| Level | Verifies | Lives in | Runs |
| --- | --- | --- | --- |
| Integration | `lorebayd`'s contract through the Web API, MCP and file URLs, or one crate's contract through its public API | The crate's `tests/` folder | Pull request CI, when the crate is affected |
| Deployment | A release's native installation and Compose deployment, operation, removal and product workflows through HTTPS, the browser and CLI | `tests/deploy/` | By hand, with `runInCI: false`, and before a pull request that changes `deploy/` or `tests/deploy/` lands |
| Unit | One intricate pure function | A `#[cfg(test)] mod tests`, see [Unit tests](#unit-tests) | Pull request CI, when affected |

- Integration tests are the level for backend behavior. Unit tests are a rare
  exception with their own bar.
- Write the fewest tests that verify the contract or the feature. Every test
  is code that must be maintained for as long as it exists.
- Coverage is never a target, a gate or a reason to add a test.
- Tests that come with code copied from outside Lorebay are never copied.
  The copied code gets the tests this doc requires, written fresh.
- Name a test `<operation>_<outcome>[_when_<condition>]`, without articles.
  The operation is the API call, crate item or user goal under test; the
  outcome is a present-tense verb phrase; `_when_` adds the condition only
  when one matters: `add_checkpoint_fails_when_task_is_ended`.

## Integration tests

An integration test verifies `lorebayd` or a crate as a **black box**: the
project is the unit, and its API is the only way in. For `lorebayd` that is
what its callers use: the Web API, the MCP tools, the OAuth endpoints and
the file URLs. For a crate, it is its public API.

### Scenarios

Each integration test is one **scenario**: the calls a real client makes to
reach one outcome the contract promises.

- A scenario verifies one promise of the contract: a success path, a
  documented rejection, or a permission boundary from the library's
  [permission matrix](../../lib/docs/contracts/access-control.md#4-permissions)
  or Lorebay's [own rows](../../docs/contracts/access-control.md#4-permissions).
- A scenario asserts the whole observable outcome: the response or returned
  value, and the state read back through the API.
- Input validation gets one scenario per error the contract documents, not
  one per field or rule.
- Before adding a scenario, look for one that already passes through the
  behavior, and extend its assertions instead.
- A test of one function, one field or one rule in isolation, or of framework
  behavior such as routing, request parsing or JSON-RPC framing, is a unit
  test in disguise. Fold the check into a scenario, or leave it out.

### Black box

- A test starts `lorebayd` through one start helper, which builds it through
  the same entry point as `main`, with test configuration, on a random port.
  Only the start helper imports from the crate.
- A test talks to `lorebayd` only over HTTP, as its callers do: the Web API
  with a person's or an agent's access token, MCP tool calls with an access
  token, and uploads and downloads on the file URLs it returns.
- A crate's test calls only the crate's public API, as a consumer would. The
  crate makes nothing public only for its tests.
- Set up state through the API, and read results back through the API. A test
  may write to the database directly only for state the API cannot create,
  such as an expired authorization code.
- Each test gets its own data directory in a temporary folder, holding a
  fresh SQLite file and the file content, and its own users file.
- Third-party services are fakes at the network boundary: a server the test
  controls.
- Each test stands alone: it creates its own data, never reads or changes
  process-global state such as environment variables or the current
  directory, and passes in any order and in parallel. Code under test takes
  such inputs explicitly.

### Regression tests

Every backend bug fix adds a scenario that fails before the fix and passes
after it, in the crate where the bug showed. Regression tests exist only at
this level.

### Layout

Integration tests live in `tests/api/main.rs`, with one module per area of
the API and the start helper in `tests/api/helpers.rs`, so all of them build
as one test binary. `lorebayd` is a library crate with a thin `src/main.rs`,
so the start helper can build it. Another crate uses the same `tests/api/`
binary, without a start helper.
The CLI's scenarios instead run the compiled `lorebay` against a compiled
`lorebayd` that their own start helper starts.

Run them with `moon run <project>:integration`.

## Deployment tests

A deployment test qualifies release files and complete installations as a
black box. It runs the executables and deployment tools as their users do,
and must not import a crate or substitute source builds for release files.

- The Python suite lives in `tests/deploy/`. One configuration file selects
  a release or local release directory, TLS inputs and two test people.
- Every run creates a fresh environment with a unique prefix and removes
  its containers, networks, volumes, images and paths, including on failure.
  It must not change unrelated host resources or existing credentials.
- Check file integrity, install refusals, HTTPS, caller limits, large-file
  streaming, certificate replacement, persistence, removal and the product
  workflows for both deployment methods. Harness sessions remain manual.
- Never print or log credentials or passwords. Clients and browsers must
  trust the supplied authority without disabling certificate verification.
- The deployment task sets `runInCI: false`. Run it with
  `moon run tests/deploy:deploy -- /absolute/path/to/config.json`.
- A pull request that changes `deploy/` or `tests/deploy/` must run the
  suite before it lands, and the pull request's evidence must name the
  run's result.

## Milestone acceptance

A milestone's acceptance checks stay in the repository, so later changes
and releases can rerun them.

- Each deterministic acceptance check must become an automated check before
  the milestone closes: an integration scenario for a contract, or a
  deployment check for a release's installation and operation.
- A check that depends on a real agent harness and model stays by hand, with
  its evidence on the acceptance issue.

## Unit tests

The default number of unit tests in a change is **zero**. Backend behavior is
verified by integration scenarios, including the behavior of the change you
just made.

A unit test is allowed only for a function that meets all three conditions:

1. It is pure: no I/O, database, network, clock or randomness, and testing it
   needs no mock, stub or fake.
2. Its logic is intricate, with many input cases: a parser, a calculation, a
   state machine or a permission rule.
3. Integration scenarios could reach those cases only with one scenario per
   case.

- When in doubt, write a scenario, or nothing.
- A qualifying function gets one table-driven test, with each input case as a
  row, in a `#[cfg(test)] mod tests` at the bottom of the file it tests.

Run them with `moon run <project>:test`.
