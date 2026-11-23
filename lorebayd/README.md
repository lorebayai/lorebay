# lorebayd

The Lorebay server: one executable that serves the web pages, the Web API and
the MCP server. Product identity and delivery live in this crate. The embedded
library owns space content and serves the MCP tools.

## Usage

Run moon and Cargo from the repo root. Cargo needs the web files that
`moon run web:build` builds; the moon tasks run it first. A release's
executable takes the same arguments, as the last two lines show:

```bash
moon run lorebayd:format lorebayd:lint lorebayd:build lorebayd:test lorebayd:integration
moon run lorebayd/migration:generate-entities
cargo run -p lorebayd -- hash-password < /path/to/password.txt
cargo run -p lorebayd -- --config /path/to/lorebayd.json --dotenv /path/to/lorebayd.env
lorebayd hash-password < /path/to/password.txt
lorebayd --config /path/to/lorebayd.json --dotenv /path/to/lorebayd.env
```

The second command regenerates Lorebay's entities. Startup applies the
library's migrations, then Lorebay's. Data directories from `v0.1.1` or later
are upgraded in place; those from before `v0.1.1` are refused: delete them
and start empty, and the users file recreates the accounts. Open the
metadata's `/oauth/authorize` endpoint with the client's authorization
parameters to sign in and consent. The CLI's client ID is `lorebay-cli`; it
accepts loopback redirects on any port and takes no agent name.

`hash-password` reads a password from standard input, removes one final line
ending and prints an Argon2id PHC hash; it starts no server and accepts no
password argument. Set `users_file` before starting the server without a
subcommand. Both flags are optional. The users file is a YAML list or a JSON
array with the fields below, each hash the command's complete output. Usernames
must be unique, nonempty and have no leading or trailing whitespace. All hashes
in one file must use the same Argon2 parameters. Restart the server after
editing the file. Changing a hash does not end existing sign-ins or agent
sign-ins; to end them, remove the username, restart, add it back and restart.

```yaml
- username: alice
  password_hash: '$argon2id$v=19$m=19456,t=2,p=1$...'
```

## Configuration

Values load from defaults, the `--config` JSON file, the `--dotenv` file and the
process environment, in that order; later values win. Only `LOREBAYD__` process
variables are read, and nested keys use `__`. Dotenv does not change the process
environment; its substitutions use earlier assignments, then the supplied
configuration environment. Relative paths resolve against the working directory.

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
| `credential_cleanup_interval_seconds` | `LOREBAYD__CREDENTIAL_CLEANUP_INTERVAL_SECONDS` | `300` | 1..315360000 seconds |
| `unused_client_lifetime_seconds` | `LOREBAYD__UNUSED_CLIENT_LIFETIME_SECONDS` | `2592000` | 1..315360000 seconds |
| `registration_rate_limit` | `LOREBAYD__REGISTRATION_RATE_LIMIT` | `10` | Positive u32 attempts per peer IP per window |
| `registration_rate_window_seconds` | `LOREBAYD__REGISTRATION_RATE_WINDOW_SECONDS` | `3600` | Positive fixed window in seconds |
| `registration_client_name_max_bytes` | `LOREBAYD__REGISTRATION_CLIENT_NAME_MAX_BYTES` | `256` | Positive maximum UTF-8 bytes in a client name |
| `registration_redirect_uri_limit` | `LOREBAYD__REGISTRATION_REDIRECT_URI_LIMIT` | `4` | Positive maximum redirect URI count |
| `registration_redirect_uri_max_bytes` | `LOREBAYD__REGISTRATION_REDIRECT_URI_MAX_BYTES` | `2048` | Positive maximum UTF-8 bytes per redirect URI |
| `registration_body_limit_bytes` | `LOREBAYD__REGISTRATION_BODY_LIMIT_BYTES` | `65536` | Positive maximum registration request bytes; excess returns OAuth JSON 413 |
| `device_request_lifetime_seconds` | `LOREBAYD__DEVICE_REQUEST_LIFETIME_SECONDS` | `600` | 1..315360000 seconds |
| `device_poll_interval_seconds` | `LOREBAYD__DEVICE_POLL_INTERVAL_SECONDS` | `5` | 1..315360000 seconds; shorter than `device_request_lifetime_seconds` |
| `device_rate_limit` | `LOREBAYD__DEVICE_RATE_LIMIT` | `10` | Positive u32 device requests per peer IP per window |
| `device_rate_window_seconds` | `LOREBAYD__DEVICE_RATE_WINDOW_SECONDS` | `60` | 1..315360000 seconds |
| `device_verification_rate_limit` | `LOREBAYD__DEVICE_VERIFICATION_RATE_LIMIT` | `10` | Positive u32 read and decision attempts per peer IP per window |
| `device_verification_rate_window_seconds` | `LOREBAYD__DEVICE_VERIFICATION_RATE_WINDOW_SECONDS` | `60` | 1..315360000 seconds |
| `upload_url_lifetime_seconds` | `LOREBAYD__UPLOAD_URL_LIFETIME_SECONDS` | `900` | 1..315360000 seconds; upload and download URLs |
| `upload_size_limit_bytes` | `LOREBAYD__UPLOAD_SIZE_LIMIT_BYTES` | `5000000000` | 1..5000000000 bytes |
| `upload_concurrency_limit` | `LOREBAYD__UPLOAD_CONCURRENCY_LIMIT` | `4` | Positive u32 concurrent uploads |
| `upload_idle_timeout_seconds` | `LOREBAYD__UPLOAD_IDLE_TIMEOUT_SECONDS` | `30` | 1..315360000 seconds without an upload chunk |
| `file_cleanup_interval_seconds` | `LOREBAYD__FILE_CLEANUP_INTERVAL_SECONDS` | `3600` | 1..315360000 seconds; the three file cleanup jobs |
| `abandoned_file_lifetime_seconds` | `LOREBAYD__ABANDONED_FILE_LIFETIME_SECONDS` | `86400` | 1..315360000 seconds; longer than `upload_url_lifetime_seconds` |
| `orphaned_file_lifetime_seconds` | `LOREBAYD__ORPHANED_FILE_LIFETIME_SECONDS` | `86400` | 1..315360000 seconds; longer than `upload_url_lifetime_seconds` and `upload_idle_timeout_seconds`; also the age for the local directory's temporary uploads and out-of-format files |
| `logging.level` | `LOREBAYD__LOGGING__LEVEL` | `info` | `off`, `error`, `warn`, `info`, `debug`, `trace` |

Set `public_url` to the origin clients reach, including through a TLS proxy or
port forward. Use HTTPS for harness connections outside loopback. The rate
limits group peers by IPv4 address or IPv6 /64. Behind a reverse proxy every
caller shares one rate limit; limits per caller belong on the proxy. Logs are
JSON on stderr and exclude configuration dumps and credentials. The data
directory holds two keys with mode 0600: `signing.key` for tokens and
`file-url-signing.key` for file URLs. Losing them ends every sign-in,
agent sign-in and file URL; anyone who reads them can forge those. Back up
the data directory only while `lorebayd` is stopped.

## Docs

Start at the [docs overview](../docs/overview.md).
