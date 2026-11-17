# lorebayd

The Lorebay server: one executable that serves the web pages, the Web API
and the MCP server, and stores the data of every space.

## Usage

Run these commands from the repo root:

```bash
moon run lorebayd:format
moon run lorebayd:lint
moon run lorebayd:build
moon run lorebayd:test
moon run lorebayd:integration
cargo run -p lorebayd -- --config /path/to/lorebayd.json --dotenv /path/to/lorebayd.env
```

Both flags are optional. With neither, the server listens on `127.0.0.1:8080`
and creates `data/` in its working directory. Configuration errors stop startup,
name the key or file and report parse locations without echoing values.

`GET /healthz` and `GET /readyz` need no credential and return
`{"data":"ok","meta":{}}` when healthy. Unknown paths return 404 with RFC 9457
`application/problem+json`: `type`, `title`, `status`, stable `code` and an
`errors` array of `{field, code, message}` objects. Internal errors disclose
no cause. Web API success bodies use `{data, meta}`.

OAuth JSON metadata is served at `/.well-known/oauth-authorization-server` and
`/.well-known/oauth-protected-resource/mcp`. Open registration at
`POST /oauth/register` returns a client ID and accepted metadata, with OAuth
JSON errors on rejection. Clients are permanent public clients with no secret;
redirect URIs require HTTPS or HTTP on `localhost`, `127.0.0.1` or `::1`.
Metadata fixes `/mcp`, `/oauth/authorize` and `/oauth/token` as endpoint paths.

SIGINT and SIGTERM start shutdown. Readiness returns 503 for one second
before the listener closes; existing requests then finish within a total
ten-second budget from the signal. At the deadline, remaining connections
are canceled. Liveness continues returning 200 while HTTP is being served.

Startup applies pending migrations to `data_directory/lorebay.sqlite3` before
serving HTTP. To apply them to a disposable file and regenerate the entities,
run from the repo root (SeaORM CLI 2.0.4):

```bash
cargo install sea-orm-cli --version 2.0.4 --locked --no-default-features \
  --features codegen,sqlx-sqlite,runtime-tokio --root "$PWD/target/tools" \
  --target-dir "$PWD/target/tools-build"
cargo build -p lorebayd --locked
schema_dir="$(mktemp -d)"
LOREBAYD__DATA_DIRECTORY="$schema_dir" LOREBAYD__LISTEN_ADDRESS=127.0.0.1:0 \
  target/debug/lorebayd 2>"$schema_dir/server.log" &
schema_pid=$!
until grep -q '"message":"lorebayd ready"' "$schema_dir/server.log"; do
  kill -0 "$schema_pid" || { cat "$schema_dir/server.log"; exit 1; }
  sleep 0.1
done
kill -TERM "$schema_pid"
wait "$schema_pid"
target/tools/bin/sea-orm-cli generate entity --compact-format --with-prelude none \
  --database-url "sqlite://$schema_dir/lorebay.sqlite3" --output-dir lorebayd/src/entity
cargo fmt --all
rm -r "$schema_dir"
```

## Configuration

Precedence is defaults, JSON file, dotenv file, then process environment.
Only `--dotenv` loads an env file; it does not change the process environment.
Only process environment keys prefixed `LOREBAYD__` are read; a non-UTF-8
value among them is a configuration error. Nested environment keys use `__`.
Dotenv substitutions use earlier assignments, then the configuration environment.
Relative file paths resolve against the working directory. Example:

```json
{
  "listen_address": "127.0.0.1:8080",
  "data_directory": "data",
  "logging": { "level": "info" }
}
```

| JSON key | Environment variable | Default | Values |
| --- | --- | --- | --- |
| `listen_address` | `LOREBAYD__LISTEN_ADDRESS` | `127.0.0.1:8080` | IP address and port; port 0 selects a free port |
| `data_directory` | `LOREBAYD__DATA_DIRECTORY` | `data` | Nonempty directory path; created at startup |
| `public_url` | `LOREBAYD__PUBLIC_URL` | `http://127.0.0.1:8080` | Canonical HTTP(S) origin, optional trailing `/`; no credentials, path, query or fragment |
| `registration_rate_limit` | `LOREBAYD__REGISTRATION_RATE_LIMIT` | `20` | Positive number of attempts per peer IP per window |
| `registration_rate_window_seconds` | `LOREBAYD__REGISTRATION_RATE_WINDOW_SECONDS` | `60` | Positive fixed window in seconds |
| `logging.level` | `LOREBAYD__LOGGING__LEVEL` | `info` | `off`, `error`, `warn`, `info`, `debug`, `trace` |

Set `public_url` to the origin clients reach, including through a TLS proxy or
port forward; metadata never uses `Host`. Harnesses require HTTPS off loopback.
Limits count all registration attempts, ignore forwarding headers and reset on
restart. Behind a proxy, callers share its peer IP quota; 429 gives `Retry-After`.
Logs are JSON on stderr, excluding configs, bodies, queries and credentials.

## Docs

Start at the [docs overview](../docs/overview.md).
