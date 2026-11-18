# lorebay

The command-line client for people using Lorebay.

## Usage

Run from the repo root:

```bash
moon run cli:format cli:lint cli:build cli:integration
cargo run -p lorebay-cli -- login https://lorebay.example
cargo run -p lorebay-cli -- login https://lorebay.example --no-browser
cargo run -p lorebay-cli -- login https://lorebay.example --device
cargo run -p lorebay-cli -- agent list --page-size 20 --output json
cargo run -p lorebay-cli -- agent list --cursor '<cursor>'
cargo run -p lorebay-cli -- agent revoke 'Claude Code (home)' --yes
cargo run -p lorebay-cli -- space list --page-size 20 --output json
cargo run -p lorebay-cli -- space create --name Team --summary 'Team work'
cargo run -p lorebay-cli -- space show SPACE-1
cargo run -p lorebay-cli -- space change SPACE-1 --name 'New team' --summary 'New work'
cargo run -p lorebay-cli -- user list
cargo run -p lorebay-cli -- participant list SPACE-1 --page-size 20
cargo run -p lorebay-cli -- participant add SPACE-1 alice --role viewer
cargo run -p lorebay-cli -- participant set-role SPACE-1 alice --role contributor
cargo run -p lorebay-cli -- participant remove SPACE-1 alice
cargo run -p lorebay-cli -- task list SPACE-1 --page-size 20 --output json
cargo run -p lorebay-cli -- task list SPACE-1 --person alice --agent 'Claude Code' --status 'in progress' --keyword review
cargo run -p lorebay-cli -- task list SPACE-1 --own-open
cargo run -p lorebay-cli -- task show SPACE-1 TK-1
cargo run -p lorebay-cli -- task scratchpad SPACE-1 TK-1 --page-size 20
cargo run -p lorebay-cli -- task close SPACE-1 '<task-id>'
cargo run -p lorebay-cli -- task delete SPACE-1 '<task-id>' --yes
cargo run -p lorebay-cli -- note delete SPACE-1 '<task-id>' '<note-time>' --yes
cargo run -p lorebay-cli -- artifact url SPACE-1 AR-1
cargo run -p lorebay-cli -- artifact delete SPACE-1 '<task-id>' AR-1 --yes
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
