# lorebay-base

Shared Rust primitives for Lorebay crates: layered configuration, structured
logging, the `Clock` trait for injecting time, and periodic background jobs.

## Usage

Run from the repository root:

```bash
moon run base:format
moon run base:lint
moon run base:build
```

`load_config::<T>(&ConfigSources)` requires `T` to implement `Default`,
`Serialize` and `Deserialize`. Callers supply the file paths, environment
prefix and environment map explicitly. The loader does not mutate process
state. Consumers validate their sections after loading.

File errors name the path and distinguish read failures from parse failures.
JSON parse errors include a line and column; env parse errors include a line
number. Unknown keys are reported separately from invalid values. Diagnostics
never display values or line contents.

`init_logging(&LoggingConfig)` installs JSON logs on stderr once per process.
`Clock::now` returns a `SystemTime`; `SystemClock` uses the system clock.
Consumers can inject their own `Clock` implementation through `Arc<dyn Clock>`.

`RateLimiter::new` takes a positive attempt limit, a window duration and an
injected clock. `check` normalizes caller-supplied `RateLimitKey` values; on
exhaustion it returns the remaining wait. `IpAddr` keys preserve IPv4 addresses
(including mapped IPv6) and group native IPv6 by /64. Clones share counters.
Expired entries are swept at most once per window; each accessed key resets
when its own window expires. Counters are in memory. Consumers validate tunables.

`run_job(interval, shutdown, round)` runs an async closure serially at a
positive `Duration` interval, starting after one interval. Missed ticks are
skipped. Shutdown takes priority over a tick and lets the active round finish
before returning. The caller owns spawning, error reporting and the shutdown
deadline; dropping the runner cancels its active round.

## Configuration

The loader applies defaults, an optional JSON file, an optional dotenv file,
then the supplied environment map, with later values overriding earlier
ones. Environment paths use `<PREFIX>__SECTION__FIELD`.
Dotenv substitutions use earlier file assignments, then the supplied
environment map. They never read the ambient process environment.

| Key | Default | Values |
| --- | --- | --- |
| `LoggingConfig.level` | `info` | `off`, `error`, `warn`, `info`, `debug`, `trace` |

## Invariants

Log selected safe fields only. Never log a whole configuration, request,
token or password. Configuration errors name keys without echoing values.

## Docs

Start at the [docs overview](../docs/overview.md).
