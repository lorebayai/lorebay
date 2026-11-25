# Skill acceptance

Run fixed natural-language scenarios through Claude Code and Codex against a
running deployment, check the recorded work through shared callers, and keep
transcripts for the person's verdict.

## Usage

Use Python 3.12+, the compiled CLI, and the pinned Playwright/Chromium setup
from [the E2E suite](../e2e/README.md#usage). Install the checkout's plugin in
three isolated homes: Claude Code and Codex for one person, and Codex for
another. The person signs each home in to its model and authorizes its agent
beforehand. Trust the configured authority in Chromium as well as the CLI
and harness processes; never disable certificate verification.

Copy `example.json` outside the checkout and replace every placeholder.
Use disposable test people; keep credentials out of the configuration. Run:

```bash
moon run tests/skill:check
moon run tests/skill:skill -- /path/to/config.json
```

The direct equivalent is `python3 -B tests/skill/run.py CONFIG.json`.
The sign-in input is awaiting the person's decision. The command currently
exits 3 before creating runtime files or contacting a deployment. Password
input and existing authorizations are both unselected.

The runner's `callers_for_attempt` seam accepts a factory returning `Callers`:
prepared shared clients, `owner` and `other` scripted agents, and any private
environment names to exclude from model processes. Model-free verification
injects callers for generated fixtures; no production factory is selected.

The suite creates a space and local project per attempt, uses the configured
home without resetting it, and continues each role's non-interactive session
by its explicit session ID. Use a home for only one run at a time. Each new
run needs homes reset by their owner with the checkout's plugin installed.
Remote spaces and recorded work remain for inspection; local runtime files
and temporary CLI sign-ins are removed on success, failure and interruption.

Every expectation prints pass or fail. Each session compares its Lorebay
tools with the server's paginated catalog and records the installed skill's
path and SHA-256. Successful skill-read events establish loading; an
installed or advertised skill alone does not. Claude Code's stream supplies
its tool names; Codex's read-only app-server inventory supplies its names.
No inventory operation starts a model turn.

Failed automation reruns the scenario once with fresh state and identical
turns, retaining both attempts. A second-attempt success says `pass on
rerun`. An interrupted run stops without retrying. Evidence files are private
and contain redacted messages and tool events, with no raw harness stderr.

Automation exits 1 on failure or 2 when only the person's verdict remains.
Read each attempt's `transcript.json`, answer every question in the run's
`verdict.json` with JSON `true` or `false`, then run:

```bash
python3 -B tests/skill/verdict.py /path/to/evidence/run-directory
```

The verdict exits 0 only when automation passes and every answer is `true`.
It preserves the distinction between a first pass and a pass on rerun.
Check the task, plan order, Knowledge use, attribution and final downloaded
file against the transcript before confirming. Questions cover behavior
that end state alone cannot prove. A newcomer installing using only
`skill/README.md` stays entirely by hand, outside this suite.

## Configuration

The external JSON file is the command's only argument. Paths are absolute;
authority and CLI paths may point into an installation or checkout. Homes,
installed skill paths and output directories must be outside the checkout.

| Key | Value |
| --- | --- |
| `origin`, `authority` | Running origin and trusted PEM authority; HTTP may use `null` authority |
| `cli`, `chromium` | CLI and Chromium executables; omit Chromium to use Playwright's |
| `output`, `timeout` | External evidence directory and seconds per model turn (default 600) |
| `scenarios`, `harnesses` | Scenario numbers 1 to 13 and `claude` and/or `codex` |
| `people.owner`, `people.other` | Distinct test usernames; sign-in input is pending |
| `homes.claude`, `homes.codex` | Homes acting as `owner`, one per harness |
| `homes.other` | Codex home acting as `other` |
| Each home | `harness`, `person`, authorized `agent` name, executable `command` array, `environment`, installed `skill` path |
| `environment` | Isolated `HOME`, harness config directory, executable `PATH` and process CA variables |

The suite reads only the configured installed `SKILL.md` from a home. Its
checksum must match this checkout. No credential file is read and no model
sign-in is attempted. Private environment names supplied by the caller
factory are excluded from model processes. Configurations and transcripts
must stay outside the checkout.

## Docs

See the [testing conventions](../../playbook/topics/testing.md).
