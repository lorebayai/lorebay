# End-to-end tests

Drive Lorebay as a team through the compiled CLI, Chromium authorization and
device pages, MCP and file URLs. Each journey creates its own space and
data; the local runner supplies a separate pair of test people per journey.

## Usage

Use Linux, the [repository toolchain](../../CONTRIBUTING.md#set-up-the-tools)
and Python 3.12 or later. Install the pinned browser driver in a virtual
environment outside the checkout, then install its Chromium:

```bash
python3 -m venv /path/to/venv
. /path/to/venv/bin/activate
pip install -r tests/e2e/requirements.txt
python -m playwright install chromium
moon run tests/e2e:e2e
```

The moon command builds `lorebayd` and `lorebay` from this checkout, starts
the server with a fresh data directory and users file, runs every journey,
and stops it. It reports each journey, elapsed time and cleanup. Failures
exit nonzero; interruption also stops the server and removes run files.
Run files live under the current user's temporary `lorebay/` directory.
Passwords are generated in memory; required users and CLI credential files
stay in the private run directory and are removed with it. Diagnostics are
redacted and server logs are checked for generated credentials before removal.

After building, the direct equivalent is:

```bash
python3 -B tests/e2e/run.py
python3 -B tests/e2e/run.py --journey 9 --journey 1
moon run tests/e2e:check tests/deploy:check
```

The syntax check runs in CI; journeys run by hand for milestone acceptance
and before releases. Journeys cover planning and claims across parties,
takeover, waiting and resuming, final states and reopening, result history,
recording and file readback with revocation, and sign-in and space roles.

## Configuration

| Argument | Default | Value |
| --- | --- | --- |
| `--server` | `target/debug/lorebayd` | Checkout-built server executable |
| `--cli` | `target/debug/lorebay` | Checkout-built CLI executable |
| `--chromium` | Playwright's Chromium | Installed Chromium executable |
| `--journey` | All journeys | Journey number; repeat to choose order |

`journeys.run_journeys` takes the target origin, test people and private
client directory, plus optional CLI, authority and Chromium paths. Journeys
use no local server state, so the same callers can reach a running HTTPS
deployment with its supplied authority. Browser trust must also be installed
in that environment; certificate verification stays enabled.

## Docs

See the [testing conventions](../../playbook/topics/testing.md).
