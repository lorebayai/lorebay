# lorebayd

The Lorebay server: one executable that serves the web pages, the Web API
and the MCP server, and stores the data of every space.

## Usage

Run these commands from the repo root:

```bash
moon run lorebayd:format lorebayd:lint lorebayd:build lorebayd:test lorebayd:integration
cargo run -p lorebayd -- hash-password < /path/to/password.txt
cargo run -p lorebayd -- --config /path/to/lorebayd.json --dotenv /path/to/lorebayd.env
```

Run `moon run lorebayd:generate-entities` to regenerate entities from a disposable database.

`hash-password` reads a password from standard input and prints an Argon2id
PHC hash. It removes one final line ending and starts no server.
It accepts no password argument.

Set `users_file` before starting the server without a subcommand. Both flags
are optional. The users file is a YAML list or a JSON array with these fields:

```yaml
- username: alice
  password_hash: '$argon2id$v=19$m=19456,t=2,p=1$...'
```

Replace the abbreviated hash with the command's complete output. Usernames
must be unique, nonempty and have no leading or trailing whitespace.
Edit this file and restart the server to add or remove accounts.

## Configuration

Values are loaded from defaults, the `--config` JSON file, the `--dotenv`
file and the process environment, in that order. Later values win.
Only `LOREBAYD__` process variables are read. Nested keys use `__`.
Dotenv does not change the process environment. Its substitutions use earlier
assignments, then the supplied configuration environment.
Relative paths resolve against the working directory.

| JSON key | Environment variable | Default | Values |
| --- | --- | --- | --- |
| `listen_address` | `LOREBAYD__LISTEN_ADDRESS` | `127.0.0.1:8080` | IP address and port; 0 selects a free port |
| `data_directory` | `LOREBAYD__DATA_DIRECTORY` | `data` | Nonempty directory path; created at startup |
| `users_file` | `LOREBAYD__USERS_FILE` | Required | Nonempty YAML or JSON file path |
| `public_url` | `LOREBAYD__PUBLIC_URL` | `http://127.0.0.1:8080` | Canonical HTTP(S) origin, optional trailing `/`; no credentials, path, query or fragment |
| `access_token_lifetime_seconds` | `LOREBAYD__ACCESS_TOKEN_LIFETIME_SECONDS` | `900` | 1..315360000 seconds |
| `refresh_token_lifetime_seconds` | `LOREBAYD__REFRESH_TOKEN_LIFETIME_SECONDS` | `2592000` | 1..315360000 seconds; renewed on refresh |
| `login_rate_limit` | `LOREBAYD__LOGIN_RATE_LIMIT` | `10` | Positive u32 attempts per peer IP per window |
| `login_rate_window_seconds` | `LOREBAYD__LOGIN_RATE_WINDOW_SECONDS` | `60` | 1..315360000 seconds |
| `registration_rate_limit` | `LOREBAYD__REGISTRATION_RATE_LIMIT` | `20` | Positive u32 attempts per peer IP per window |
| `registration_rate_window_seconds` | `LOREBAYD__REGISTRATION_RATE_WINDOW_SECONDS` | `60` | Positive fixed window in seconds |
| `logging.level` | `LOREBAYD__LOGGING__LEVEL` | `info` | `off`, `error`, `warn`, `info`, `debug`, `trace` |

Set `public_url` to the origin clients reach, including through a TLS proxy
or port forward. Use HTTPS for harness connections outside loopback.
The rate limits use the connection's peer IP. Behind a reverse proxy every
caller shares one rate limit; limits per caller belong on the proxy.
The signing key is kept in `data_directory/signing.key` with mode 0600.
Logs are JSON on stderr and exclude configuration dumps and credentials.

## Docs

Start at the [docs overview](../docs/overview.md).
