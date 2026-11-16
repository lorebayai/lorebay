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
and name the invalid key. File errors include the path and distinguish read
failures from parse failures. JSON parse errors include a line and column;
env parse errors include a line number. Unknown keys are reported separately
from invalid values, and diagnostics never echo values or line contents.

`GET /healthz` and `GET /readyz` need no credential and return
`{"data":"ok","meta":{}}` when healthy. Unknown paths return 404 with RFC 9457
`application/problem+json`: `type`, `title`, `status`, stable `code` and an
`errors` array of `{field, code, message}` objects. Internal errors disclose
no cause. Success bodies use `{data, meta}`.

SIGINT and SIGTERM start shutdown. Readiness returns 503 for one second
before the listener closes; existing requests then finish within a total
ten-second budget from the signal. At the deadline, remaining connections
are canceled. Liveness continues returning 200 while HTTP is being served.

## Configuration

Precedence is defaults, JSON file, dotenv file, then process environment.
Only `--dotenv` loads an env file; it does not change the process environment.
Only process environment keys prefixed `LOREBAYD__` are read; a non-UTF-8
value among them is a configuration error. Nested environment keys use `__`.
Dotenv substitutions use earlier file assignments, then the supplied
configuration environment; unprefixed process variables cannot affect them.
Relative paths in `--config`, `--dotenv` and `data_directory` resolve against
the working directory. Example:

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
