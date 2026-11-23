# Testing

How `lorebayd` and the product are tested: the three test levels, what each
one verifies, and where its tests live. Imported by the `AGENTS.md` of every
Rust crate and test suite.

## Levels

| Level | Verifies | Lives in | Runs |
| --- | --- | --- | --- |
| Integration | `lorebayd`'s contract through the Web API, MCP and file URLs, or one crate's contract through its public API | The crate's `tests/` folder | Pull request CI, when the crate is affected |
| E2E | Product features, through the browser | `tests/e2e/` | Nightly |
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
  when one matters: `add_note_fails_when_task_is_ended`.
  TypeScript titles use the same words separated by spaces.

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

## E2E tests

An E2E test verifies a **product feature** through the browser, against a
running `lorebayd`.

- All E2E tests live in one test suite, `tests/e2e/`, written in TypeScript
  with Playwright.
- Each test is one **journey**: a person's goal from start to finish, such
  as approving a proposal an agent submitted.
- A journey asserts what the person sees, with role, label and text locators
  and Playwright's auto-waiting assertions (`await expect(...)`).
- Contract details, such as error responses and edge cases, belong to
  integration tests. A journey verifies that the feature works for the
  person.
- Each journey signs in as its own user from the suite's users file, creates
  its own space and data, and depends on no other journey. It creates agent
  data through MCP with an access token, as an agent would.
- The suite runs against a `lorebayd` built from the latest `main`, with a
  fresh data directory. No other service runs.
- The suite runs nightly. Its moon task sets `runInCI: false`, which keeps it
  out of pull request CI. Run it locally with `moon run tests/e2e:e2e`.
- CI retries a failed journey once and records a trace of the retry. A
  journey that passes only on retry is flaky; fix it.

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
