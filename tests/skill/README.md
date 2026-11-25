# Skill acceptance

Run fixed natural-language scenarios through Claude Code and Codex against a
running deployment, check the recorded work through shared callers, and keep
transcripts for the person's verdict.

## Usage

Use Python 3.12+, the pinned Playwright package from the
[E2E requirements](../e2e/README.md#usage), the compiled CLI and installed
harnesses. Prepare three
CLI homes for the owner, another person and an admin; two script-agent homes
for the owner and other person; and three harness homes: Claude Code and
Codex for the owner, and Codex for the other person. Authorize each once
through the external home scripts. Only the person signs in to a model.
The suite receives no password, reads no credential file and never logs in.

Copy `example.json` outside the checkout and replace every placeholder.
Trust the configured authority in each process; never disable verification.
Run from the repository root:

```bash
moon run tests/skill:check
moon run tests/skill:skill -- /path/to/config.json
```

The direct equivalent is `python3 -B tests/skill/run.py CONFIG.json`.
People use the compiled CLI with their persistent `LOREBAY_HOME`. Script
agents use the configured external command over JSON; that program owns its
OAuth tokens and refreshes. CLI, output parsing, file transfer and MCP call
shapes reuse the shared callers. The browser/password OAuth path belongs to
the E2E and deployment suites, rather than this runner.

A run reserves every configured home with its shared lock before doing work.
CLI commands, script-agent commands and harness processes inherit their home
lock through `LOREBAY_TEST_HOME_LOCK_FD`, including refreshes. A busy home
refuses the run. Parallel runs need different homes. Authorizing, resetting
and interactive harness sessions must use the same locks. Locks live beside
the profiles, outside reset contents.

Each attempt creates a fresh space and project, adds the other person and
admin, and records that space in the project's instructions. It assumes no
clean server. Each role continues by explicit session ID. Homes and remote
work survive the run; temporary projects are removed on success, failure
and interruption. Before a set of real sessions, reset each harness home
with the checkout's plugin while keeping its authorization and model sign-in.
After a server wipe, the external authorizer restores all ended sign-ins.

Every expectation prints pass or fail. Sessions compare their Lorebay tools
with the server's paginated catalog through a script-agent command and record
the installed skill's path and SHA-256. Successful skill reads or a successful
Claude Skill invocation establish loading; installation alone does not.
Claude's init lists tools; Codex's read-only app-server inventory lists them
without starting a model turn. Transcripts are private and sanitized.

Failed automation retries once with a fresh space and the same turns,
retaining both attempts. A second-attempt success says `pass on rerun`.
Interruption stops without retrying; timeout retains partial events and
retries once. Automation exits 1 on failure or 2 pending the person's verdict.
Read each `transcript.json`, answer every question in `verdict.json` with JSON
`true` or `false`, then run:

```bash
python3 -B tests/skill/verdict.py /path/to/evidence/run-directory
```

The verdict exits 0 only when automation and every answer pass; it preserves
a pass on rerun. Questions cover what state cannot show: plan order, actual
Knowledge use and delegation, intent and complete results. A newcomer using
only `skill/README.md` to install stays entirely by hand, outside this suite.

## Configuration

The external JSON file is the only input. Paths are absolute. Home, lock,
installed skill and output paths stay outside the checkout. CLI and authority
paths may point at an installation or checkout. Values below are placeholders.

| Key | Value |
| --- | --- |
| `origin`, `authority` | Running origin and trusted PEM authority; HTTP may use `null` authority |
| `cli`, `output`, `timeout` | CLI executable, external evidence directory, seconds per turn (default 600) |
| `scenarios`, `harnesses` | Scenario numbers 1 to 13; `claude` and/or `codex` |
| `people.owner`, `people.other`, `people.admin` | `username`, persistent CLI `home`, shared `lock` |
| `script_agents.owner`, `script_agents.other` | External `command` array, script-agent `home` and shared `lock` |
| `homes.claude`, `homes.codex`, `homes.other` | `harness`, `person`, agent name, `command`, `environment`, installed `skill`, `lock` |
| `environment` | Harness `HOME`, its config directory, executable `PATH` and process CA variables |

Each role needs a distinct home. Its lock is `<parent>/.locks/<name>.lock`;
external commands must honor that lock and validate inherited ownership.
Each script-agent command reads one MCP JSON-RPC request on stdin and writes
its response on stdout. It validates and holds the inherited lock descriptor
in `LOREBAY_TEST_HOME_LOCK_FD` and refuses an origin other than
`LOREBAY_TEST_EXPECTED_ORIGIN`.
The configured installed `SKILL.md` must match this checkout. The suite reads
only that public file from a home; credential files stay opaque to it.

## Docs

See the [testing conventions](../../playbook/topics/testing.md).
