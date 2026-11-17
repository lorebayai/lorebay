# lorebay

The command-line client for people using Lorebay.

## Usage

Run from the repo root:

```bash
moon run cli:format cli:lint cli:build cli:integration
cargo run -p lorebay-cli -- login https://lorebay.example
cargo run -p lorebay-cli -- login https://lorebay.example --no-browser
cargo run -p lorebay-cli -- agent list --page-size 20 --output json
cargo run -p lorebay-cli -- agent list --cursor '<cursor>'
cargo run -p lorebay-cli -- agent revoke 'Claude Code (home)' --yes
cargo run -p lorebay-cli -- logout
cargo run -p lorebay-cli -- --help
```

## Configuration

| Environment variable | Default | Values |
| --- | --- | --- |
| `LOREBAY_HOME` | `$HOME/.lorebay` | Nonempty directory for `credentials.json` and `credentials.lock` |
| `HOME` | Operating environment | Home directory when `LOREBAY_HOME` is unset |
| `SSL_CERT_FILE` | System trust store | PEM certificate authorities; replaces system trust |
| `BROWSER` | System browser | Browser opener preference on Linux |

## Docs

Start at the [docs overview](../docs/overview.md).
