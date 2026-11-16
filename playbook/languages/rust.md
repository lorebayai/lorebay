# Rust

## Crates and dependencies

- The root `Cargo.toml` is the workspace. Each crate is a top-level folder
  listed in its `members`, as the
  [repository layout](../../README.md#repository-layout) shows. The crate
  that builds the `lorebayd` executable is named `lorebayd`; any other crate
  is named `lorebay-<folder>`.
- A new crate gets a `moon.yml`, a `README.md` per
  [documentation.md](../core/documentation.md#readmes), and an `AGENTS.md`
  importing this fragment, with `ln -s AGENTS.md CLAUDE.md` beside it, in the
  change that creates it.
- Every third-party dependency version must be declared once in the root
  `Cargo.toml` `[workspace.dependencies]`. Crates must inherit it with
  `name = { workspace = true }`, adding `features` when needed; a crate
  must not declare a dependency version.
- Before adding or porting a dependency, its author must run
  `cargo info <crate>` and use its latest stable release. Ported code must
  adapt to that release rather than copy a version from another repository.
  An older pin may be used only when adaptation is impossible, with a
  one-line comment in the root `Cargo.toml` naming the cause.
- After changing workspace members or dependencies, its author must refresh
  `Cargo.lock` with `cargo update --workspace` before running locked checks.
  It adds missing entries without upgrading the ones already locked.
  `Cargo.lock` must be committed.
- Crates must inherit package fields from `[workspace.package]` and lints
  with `[lints] workspace = true`; they must not define their own lint tables.
- `rust-toolchain.toml` pins one stable Rust release, and `rust-version` in
  `[workspace.package]` names the same release. A toolchain bump changes
  both in one pull request.
- Every dependency must pass `cargo deny`, configured in the root
  [`deny.toml`](../../deny.toml). A license joins its allow list only when it
  lets the `lorebayd` executable ship under Apache-2.0.
- A crate must not declare Cargo `[features]` unless a consumer must be able
  to omit a heavy optional dependency.
- Rust projects must declare `language: rust` and their layer in `moon.yml`.
  Their project dependencies must come from `Cargo.toml`, not `moon.yml`.

## Checks and lint policy

- Rust checks must use stock stable rustfmt and Clippy. A new convention
  must have a stock check unless this fragment states why it stays unchecked.
- Authors must run moon commands from the repo root:
  `moon run <project>:format`, `moon run <project>:lint`,
  `moon run <project>:build`, `moon run <project>:test` and
  `moon run <project>:deny`, plus `moon run <project>:integration` where
  that task exists.
- Formatting fixes should use `cargo fmt`; automatic lint fixes should use
  `cargo clippy --fix`. Authors must review automatic fixes and rerun the
  corresponding moon checks.
- Code must pass the workspace lint policy in the
  [root manifest](../../Cargo.toml) and [Clippy settings](../../clippy.toml),
  including the lint task's `-D warnings`.
- Code must not use unsafe code; the workspace forbids it.
- Production code must not use `unwrap`, `expect`, `panic!`, `todo!`,
  `unimplemented!` or `unreachable!`, or leave `dbg!`, stdout or stderr
  debugging output. Tests may use the panic, unwrap, expect, debug and print
  exemptions configured in `clippy.toml`.
- A justified lint exception must use
  `#[expect(clippy::<lint>, reason = "...")]` on the narrowest item, never
  `#[allow]`.
- Tests must follow [testing.md](../topics/testing.md). Comments and public
  documentation must follow [code-comments.md](../core/code-comments.md);
  READMEs and agent instructions must follow
  [documentation.md](../core/documentation.md).

## Modules and file order

- Public APIs must be flat: modules should stay private with `mod x;` and
  expose their public items through `pub use x::*;`. Consumers must import
  the re-exported paths.
- Types must be imported at the top of the file rather than written as
  inline full paths; code must pass `clippy::absolute_paths` with its
  two-segment limit.
- Files must place `use` declarations first, then public items, then private
  items. Public items must follow this order: modules and re-exports,
  constants, types, traits, enums and structs, their implementations,
  functions. Test placement must follow
  [testing.md](../topics/testing.md#unit-tests).

## Errors and data structures

- Every error type must use `thiserror`. Wrapped dependency errors must use
  `#[error(transparent)]` and `#[from]`; error variants must omit the `Error`
  suffix.
- Public traits must not have an associated `type Error`. Trait boundaries
  whose backends fail in unrelated ways must use `anyhow::Result`; callers
  that branch on variants must receive a concrete shared error type.
- Startup and configuration errors must give operators enough context to
  identify the invalid input and correct it.
- Data structs, including configuration, DTOs, requests, responses and
  options, must derive `TypedBuilder`, `Clone` and `Debug`, with
  `#[builder(field_defaults(setter(into)))]`. Structs holding no data may
  omit `TypedBuilder`.
- Every enum must order variants alphabetically, A to Z, as checked by
  `clippy::arbitrary_source_item_ordering`. An enum whose declaration order
  carries meaning, such as one deriving `PartialOrd` or `Ord`, must preserve
  that order with
  `#[expect(clippy::arbitrary_source_item_ordering, reason = "...")]`.

## Readability and idioms

- Function bodies must stay within Clippy's 60-line limit
  (`clippy::too_many_lines`). Functions must have at most five parameters
  (`clippy::too_many_arguments`); more inputs must become a struct.
- Code must pass `clippy::excessive_nesting` at threshold 5. Clippy counts
  enclosing items: an ordinary `impl` method may have three nested
  control-flow levels, and a fourth fails; a free function may have four.
  This check must not be treated as uniform enforcement of three
  control-flow levels. Authors should flatten nesting with early returns
  and extracted functions.
- Behavior choices must use enums rather than behavior-flipping booleans.
  Functions must have at most one boolean parameter
  (`clippy::fn_params_excessive_bools`), and structs must have at most three
  boolean fields (`clippy::struct_excessive_bools`).
- Tunables, identifiers and shared literals must be named constants. This
  rule stays unchecked because stock tools cannot distinguish those values
  from incidental literals.
- Code should prefer iterators over indexed loops, `?` over
  error-propagating `match`, `Option` combinators over manual unwrapping,
  and `From` or `Into` over custom `to_*` conversions.
