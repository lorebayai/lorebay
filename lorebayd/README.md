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
and creates `data/` in its working directory. Configuration errors stop startup
and name the invalid key.

`GET /healthz` and `GET /readyz` need no credential. Success responses are
`{"data":"ok","meta":{}}`. Unknown paths return 404 with RFC 9457
`application/problem+json`: `type`, `title`, `status`, stable `code` and an
`errors` array of `{field, code, message}` objects. Internal errors disclose
no cause. Success bodies use `{data, meta}`; collection cursors belong in
`meta.pagination.next_cursor`.

SIGINT and SIGTERM start shutdown. Readiness returns 503 for one second
before the listener closes; existing requests then finish within a total
ten-second budget from the signal. At the deadline, remaining connections
are canceled. Liveness continues returning 200 while HTTP is being served.

## Configuration

Precedence is defaults, JSON file, dotenv file, then process environment.
Only `--dotenv` loads an env file; it does not change the process environment.
Nested environment keys use `__`. JSON paths are relative to the working
directory. Example:

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
| `logging.level` | `LOREBAYD__LOGGING__LEVEL` | `info` | `off`, `error`, `warn`, `info`, `debug`, `trace` |

Logs are JSON on stderr. Configuration dumps, request bodies, query strings
and authorization headers are never logged.

## Docs

Start at the [docs overview](../docs/overview.md).
