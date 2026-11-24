# Deployment tests

Qualify a release as an operator, a person and an agent, using the published
executables and deployment bundle. The suite runs native Debian 12, native
Ubuntu 24.04, Docker Compose and the Alpine archive recipe in disposable
containers. It calls no Rust crate function.

## Usage

On an x86_64 Linux host, install Python 3.12+, Docker Engine with Compose and
buildx, GitHub CLI with `gh attestation`, OpenSSL, curl and sudo.
Local image archives also need Skopeo for import under an owned image name.
Docker must support privileged systemd containers; the current user needs
Docker access and passwordless sudo for the suite's own deployment data.
The client image installs Chromium and pins Playwright to 1.56.0.

Sign in to GitHub with access to the release and private container package.
Copy `example.json` outside the checkout and fill in its paths and account.
The domain must resolve to loopback on the host. Run from the repository root:

```bash
moon run tests/deploy:deploy -- /absolute/path/to/config.json
```

The equivalent direct command is `python3 -B tests/deploy/run.py CONFIG.json`.
Allow time for container builds and several transfers larger than 1 GiB.
The command reports each check, its duration, a final resource listing and
an output audit against the generated passwords and GitHub token. Failure
stops the checks, prints filtered diagnostics, cleans up and exits nonzero.
SIGINT and SIGTERM also trigger cleanup. After a host crash or SIGKILL,
use the printed unique prefix to identify and remove the interrupted run's
resources; never prune Docker globally.

Each run creates a unique `lorebay-deploy-` prefix for containers, networks,
images, process files under `/tmp/<user>/lorebay/` and data under `/srv/`.
Only nginx publishes a randomly allocated port, bound to `127.0.0.1`.
The two clients and their browsers use the deployment's private network.
All created resources and newly pulled image references are removed in
`finally`; preexisting images are retained. The existing GitHub sign-in is
read at run time; a temporary Docker credential helper returns the token
from memory, and the Docker configuration contains no credential.
Service readability is checked through container bind mounts, so private
temporary-directory ancestors and restrictive umasks remain supported.

For local release files, replace `target` with:

```json
{"directory": "/absolute/path/to/release"}
```

That directory contains the four CLI/server musl archives, the deployment
bundle, a single-image Docker archive named `image.tar`, and `SHA256SUMS`
covering all six. The archives supply the local version. Local files receive
checksum checks; hosted releases also
require GitHub attestations for every download and the server manifest.
Use Docker-format archives to retain the image health check.
Draft releases use authenticated GitHub downloads. Hosted images are resolved
once and pulled by their recorded manifest digest; Compose uses that digest.

Harness acceptance remains manual, once per method: install the same
release's skill in fresh Claude Code and Codex profiles, trust the supplied
authority, and authorize each through the browser. Give each a natural work
request without naming Lorebay; verify a task, plan and checkpoints, a file,
a Knowledge read, a submitted request and a result. Inspect and download
their output with the CLI, decide their requests, list their agent sign-ins,
revoke one and verify that harness is refused. Record transcripts with
credentials and signed links removed, then remove those profiles.

## Configuration

The JSON file is the only input; it contains no password or token. Relative
paths are resolved from the command's working directory; `~` is supported.

| Key | Value |
| --- | --- |
| `target.version` | GitHub release tag, including prereleases; mutually exclusive with `directory` |
| `target.directory` | Local release directory described above |
| `domain` | Lowercase DNS name covered by the certificate |
| `certificate`, `key` | PEM certificate chain and unencrypted private key |
| `authority` | PEM authority trusted by clients and browser NSS databases |
| `people` | Two distinct test usernames; passwords are generated per run |
| `registry.github_account` | Existing GitHub CLI account with package access |

## Docs

See the [testing levels](../../playbook/topics/testing.md).
