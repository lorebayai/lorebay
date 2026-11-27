# Deployment tests

Qualify release files as an operator, a person and an agent through native
Debian 12, native Ubuntu 26.04, Compose and the Alpine archive recipe.
The suite drives executables, deployment tools, the CLI, HTTP and Chromium.
Each install runs the same nine [team journeys](../e2e/README.md): HTTPS for
native and Compose installations, loopback HTTP for the Alpine archive.
Journeys have independent people, spaces and data. Separate journey clients
preserve the proxy's per-caller limits; streaming, restart and revocation
qualification stays in this suite.

## Usage

Use x86_64 Linux, Python 3.12+, Docker Engine with Compose and buildx,
GitHub CLI with `gh attestation`, OpenSSL, curl and passwordless sudo.
Docker access and privileged systemd containers are required. The client
image installs Chromium and Playwright 1.56.0. Local builds also need the
[repository toolchain](../../CONTRIBUTING.md#set-up-the-tools), musl-tools,
binutils, CMake, pkg-config and Skopeo; release tasks install their notice tools.

Sign in to GitHub with release/package access. Copy `example.json` outside
this checkout, fill in its inputs, then run from the repository root:

```bash
moon run tests/deploy:deploy -- /absolute/path/to/config.json
```

The direct equivalent is `python3 -B tests/deploy/run.py CONFIG.json`.
For a local native build, run these commands from the repository root:

```bash
export RELEASE_TARGET="$(uname -m)-unknown-linux-musl"
moon run lorebayd:release-build cli:release-build
moon run lorebayd:release-package cli:release-package
moon run deploy:image-build
moon run tests/deploy:local-files
```

Set `target` to `{"directory": "target"}` in the configuration. The suite
reads the native archives, deployment bundle and `SHA256SUMS` from
`target/release-archives/<target>/`, and the compressed Docker-format image
and recorded digest from `target/release-images/<target>/`. It requires only
the running architecture's archives and imports the image directly with Skopeo.
`local-files` prepares the deployment bundle and hashes the archives present.
Local files receive checksum/digest checks; hosted releases also require
attestations for every download and the manifest. Draft downloads use the
existing GitHub sign-in, and hosted images are pulled by their recorded digest.

Allow time for image builds and transfers larger than 1 GiB. Every run has
its own `lorebay-deploy-` prefix for containers, networks, images, a BuildKit
builder/cache, process files under `/tmp/<user>/lorebay/` and data under `/srv/`.
Only nginx publishes a random port on `127.0.0.1`; clients use private networks.
The command reports checks, total duration and a cleanup listing. `finally`
removes owned resources, including the builder and its cache; existing images
and other builders' cache remain. Failures stop the checks and exit nonzero.
SIGINT/SIGTERM trigger cleanup. After a crash or SIGKILL, remove resources
identified by the printed prefix; never prune Docker globally.

Output and diagnostics are redacted. Raw container/service logs and temporary
files are scanned for generated passwords and the GitHub token. The temporary
Docker credential helper reads the token from memory; its configuration holds
no credential. No raw credential output is saved as an audit artifact.

Harness acceptance stays manual, once per method: install the same release's
skill in fresh Claude Code and Codex profiles, trust the supplied authority,
and authorize through the browser. Give each a natural work request without
naming Lorebay; verify a task, plan/checkpoints, a file, a Knowledge read,
a request and a result. Inspect/download the output with the CLI, decide
the requests, list agent sign-ins, revoke one and verify refusal. Record
credential-free transcripts, then remove those profiles.

## Configuration

The JSON file is the only input and contains no password or token. Paths
resolve from the command's working directory; `~` is supported.

| Key | Value |
| --- | --- |
| `target.version` | GitHub release tag, including prereleases; excludes `directory` |
| `target.directory` | Local build output root (`target/`); excludes `version` |
| `domain` | Lowercase DNS name covered by the certificate |
| `certificate`, `key` | PEM certificate chain and unencrypted private key |
| `authority` | PEM authority trusted by clients and browser NSS databases |
| `people` | Two distinct operation-check usernames; journey people and all passwords are generated per run |
| `registry.github_account` | Existing GitHub CLI account with package access |

## Docs

See the [testing levels](../../playbook/topics/testing.md).
