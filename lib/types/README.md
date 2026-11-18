# lorebay-lib-types

Shared primitives for library consumers include the redacted `Secret<T>`,
`sha256_hex`, the cursor codec and conversion from `SystemTime` to Unix
milliseconds.
The crate has no database, MCP or async dependency.

## Usage

Run from the repository root:

```bash
moon run lib/types:format lib/types:lint lib/types:build lib/types:test
cargo doc -p lorebay-lib-types --no-deps --open
```

## Invariants

Keep secrets in `Secret<T>` and reveal them only where their value is needed.
Serialization preserves the value; never serialize a secret into a log.

## Docs

Start at the [library overview](../docs/overview.md).
