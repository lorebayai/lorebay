Follow these playbook fragments:

- @../playbook/languages/rust.md

## Library conventions

- Nothing in `lib/` depends on, or links to, a file outside `lib/`, except
  the playbook, which an `AGENTS.md` here may import and a doc may link to.
  The library depends on no other crate of this repository. The root Cargo
  workspace settings and the toolchain stay shared. `moon run lib:boundary`
  checks it.
- The library names nothing of one product, as
  [Library architecture](docs/architecture/library.md#14-rules-for-library-code)
  states.
- The library's public API is its contract with every product that depends
  on it. Each of these is a breaking change:
  - removing or changing a public item, re-export, public field or feature;
  - removing or changing an error variant a product can match on;
  - changing a migration a product has applied;
  - changing the behavior a contract under `docs/contracts/` states;
  - a new major release of a public dependency
    ([Library architecture](docs/architecture/library.md#41-database-backends-and-dependencies));
  - raising the minimum Rust version.
- While the version is `0.x`, a breaking change bumps the minor version and
  any other change bumps the patch version. Features stay additive, have no
  default and select drivers only.
- Every change a product can observe adds an entry under `## Unreleased` in
  [CHANGELOG.md](CHANGELOG.md), in the same pull request, and marks a
  breaking change as breaking.
- A release bumps the four crates to one new version, in their manifests
  and in the root `[workspace.dependencies]`, and moves the entries under
  `## <version> - <date>`. After the merge, the merge commit is tagged
  `lib-v<version>`.
- The library crates set their own `version` and `publish = true`, so that
  `moon run lib:package` can verify them together. Nobody runs
  `cargo publish`; publishing to a registry is out of
  [scope](docs/architecture/library.md#2-scope).
