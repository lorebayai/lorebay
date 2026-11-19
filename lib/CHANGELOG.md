# Changelog

Changes that a product depending on the library can observe, for the four
crates `lorebay-lib-types`, `lorebay-lib-core`, `lorebay-lib-migration` and
`lorebay-lib-mcp`. They share one version, and each release is tagged
`lib-v<version>`.

## Unreleased

## 0.1.0 - 2026-11-19

The first versioned release.

- `lorebay-lib-types`: the secret type, `sha256_hex`, the cursor codec and
  the conversion from `SystemTime` to Unix milliseconds.
- `lorebay-lib-core`: the library state and its settings, the five
  interfaces a product implements, the write entry, one function per
  operation on spaces, tasks, notes and artifacts, the two cleanup rounds,
  and the reads of the highest space number and of a space key. The
  `sqlite` and `postgres` features select the database driver.
- `lorebay-lib-migration`: the library's `Migrator`, with its own `sqlite`
  and `postgres` features.
- `lorebay-lib-mcp`: the router of the MCP tools, which forwards the same
  two features to core.
