# lorebay-base

Shared Rust primitives for Lorebay crates: layered configuration, structured
logging and the `Clock` trait for injecting time into consumers.

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

`init_logging(&LoggingConfig)` installs JSON logs on stderr once per process.
`Clock::now` returns a `SystemTime`; `SystemClock` uses the system clock.
Consumers can inject their own `Clock` implementation through `Arc<dyn Clock>`.

## Configuration

The loader applies defaults, an optional JSON file, an optional dotenv file,
then the supplied environment map, with later values overriding earlier
ones. Environment paths use `<PREFIX>__SECTION__FIELD`.

| Key | Default | Values |
| --- | --- | --- |
| `LoggingConfig.level` | `info` | `off`, `error`, `warn`, `info`, `debug`, `trace` |

## Invariants

Log selected safe fields only. Never log a whole configuration, request,
token or password. Configuration errors name keys without echoing values.

## Docs

Start at the [docs overview](../docs/overview.md).
