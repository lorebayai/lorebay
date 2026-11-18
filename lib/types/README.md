# lorebay-lib-types

Shared primitives for library consumers: `Secret<T>`, whose `Debug` output
is redacted, and `sha256_hex`, the lowercase hexadecimal SHA-256 of a string.
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
