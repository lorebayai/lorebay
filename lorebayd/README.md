# lorebayd

The Lorebay server: one executable serving the web pages, Web API and MCP,
with the data of every space in its data directory.

## Usage

Run from the repo root:

```bash
moon run lorebayd:format lorebayd:lint lorebayd:build lorebayd:test lorebayd:integration
cargo run -p lorebayd -- hash-password < /path/to/password.txt
cargo run -p lorebayd -- --config /path/to/lorebayd.json --dotenv /path/to/lorebayd.env
```

`hash-password` reads standard input, removes one final line ending, prints
an Argon2id PHC hash and starts nothing. It accepts no password argument.
Set `users_file` before starting the server without a subcommand. Both flags
are optional. Defaults listen on `127.0.0.1:8080` and create `data/`.

The users file is a YAML list (JSON arrays with the same fields also work):

```yaml
- username: alice
  password_hash: '$argon2id$v=19$m=19456,t=2,p=1$...'
```

Replace the abbreviated hash with the command's complete output. Startup
rejects missing, malformed or duplicate accounts and unverifiable hashes,
naming the file and cause. Usernames must be nonempty without outer whitespace.
Restart after editing: removed accounts lose their memberships and credentials;
adding a username back restores the same account, without its old credentials.

`POST /api/v1/login-session` accepts `{username,password}` and returns
`{access_token,refresh_token,token_type,expires_in}` in `data`. Refresh with
`POST /api/v1/login-session/refresh` and `{refresh_token}`; each refresh rotates
the token. `DELETE /api/v1/login-session` logs out with `Authorization: Bearer
<access_token>`. Tokens are checked against their session on every call.
The signing key persists in `data_directory/signing.key`, with mode 0600.

`GET /healthz` and `/readyz` need no credential and return
`{"data":"ok","meta":{}}`. Success bodies use `{data,meta}`. Errors use RFC
9457 `application/problem+json` with `type`, `title`, `status`, stable `code`
and `errors: [{field,code,message}]`. Internal causes are never disclosed.
Rate-limited sign-in returns 429 and `Retry-After` seconds. Behind a reverse
proxy every caller shares one rate limit; limits per caller belong on the proxy.

SIGINT/SIGTERM disable readiness for one second before closing the listener.
In-flight requests have a total ten-second shutdown budget; stalled requests
are canceled at the deadline. Liveness remains available while HTTP is served.
Startup migrates `data_directory/lorebay.sqlite3` before serving HTTP.
To regenerate entities from a disposable database (SeaORM CLI 2.0.4):

```bash
cargo install sea-orm-cli --version 2.0.4 --locked --no-default-features \
  --features codegen,sqlx-sqlite,runtime-tokio --root "$PWD/target/tools" \
  --target-dir "$PWD/target/tools-build"
cargo build -p lorebayd --locked
schema_dir="$(mktemp -d)"
printf '[]\n' > "$schema_dir/users.json"
LOREBAYD__USERS_FILE="$schema_dir/users.json" LOREBAYD__DATA_DIRECTORY="$schema_dir" \
  LOREBAYD__LISTEN_ADDRESS=127.0.0.1:0 target/debug/lorebayd 2>"$schema_dir/server.log" &
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

Precedence: defaults, `--config` JSON, `--dotenv`, process environment.
Only `LOREBAYD__` process keys are read; nested keys use `__`. Dotenv does not
mutate the process environment; substitutions use earlier file assignments,
then supplied configuration environment. Relative paths use the working directory.
Invalid keys, types and values stop startup without echoing values. File errors
name the path and cause; config parse errors include line/column information.

| JSON key | Environment variable | Default | Values |
| --- | --- | --- | --- |
| `listen_address` | `LOREBAYD__LISTEN_ADDRESS` | `127.0.0.1:8080` | IP and port; 0 selects a free port |
| `data_directory` | `LOREBAYD__DATA_DIRECTORY` | `data` | Nonempty directory; created at startup |
| `users_file` | `LOREBAYD__USERS_FILE` | Required | Nonempty YAML or JSON file path |
| `access_token_lifetime_seconds` | `LOREBAYD__ACCESS_TOKEN_LIFETIME_SECONDS` | `900` | 1..315360000 seconds |
| `refresh_token_lifetime_seconds` | `LOREBAYD__REFRESH_TOKEN_LIFETIME_SECONDS` | `2592000` | 1..315360000 seconds, renewed on refresh |
| `login_rate_limit` | `LOREBAYD__LOGIN_RATE_LIMIT` | `10` | 1..4294967295 attempts per peer IP per window |
| `login_rate_window_seconds` | `LOREBAYD__LOGIN_RATE_WINDOW_SECONDS` | `60` | 1..315360000 seconds |
| `logging.level` | `LOREBAYD__LOGGING__LEVEL` | `info` | `off`, `error`, `warn`, `info`, `debug`, `trace` |

Logs are JSON on stderr; no configuration dumps, bodies, query strings or credentials.

## Docs

Start at the [docs overview](../docs/overview.md).
