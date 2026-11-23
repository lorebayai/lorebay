# TypeScript

## Packages and dependencies

- The web pages are one Next.js project in `web/`, built into static files
  that the `lorebayd` executable embeds
  ([System architecture](../../docs/architecture/system.md#5-technology)).
- Until the first designed page, `web/` holds plain TypeScript pages built
  to static files.
- A package must be named `@lorebay/<folder>`, and its `package.json` must
  set `"private": true` and `"type": "module"`. Its folder is listed in the
  `packages` of `pnpm-workspace.yaml`.
- Every third-party dependency version must be declared once, as an exact
  version, in the `catalog` of `pnpm-workspace.yaml`. Packages must reference
  it with `"catalog:"` and must not declare a version; workspace packages
  depend on each other with `"workspace:*"`.
- `web/package.json` must declare development dependencies only: the pages
  ship no npm package at runtime. Shipping an npm package requires the
  release check to cover npm licenses first.
- Before adding a dependency, its author must run
  `npm view <package> dist-tags.latest` from outside the repo and use that
  release. An older pin may be used only when the latest cannot work, with a
  one-line comment above its catalog entry naming the cause.
- pnpm's `minimumReleaseAge` must keep its default. A branch may list a
  release under a day old in `minimumReleaseAgeExclude` as an exact
  `<name>@<version>`; `main` must have no `minimumReleaseAgeExclude`.
- After changing dependencies, its author must run `pnpm install` from the
  repo root and commit `pnpm-lock.yaml` with the change.

## Checks and lint policy

- Each project's `tsconfig.json` must extend the root `tsconfig.base.json`,
  which sets `strict` and `noUncheckedIndexedAccess`; a project must not turn
  either off.
- Each project's `eslint.config.js` must include `typescriptConfig` from the
  root `eslint.base.js`; Prettier uses the root `.prettierrc.json`. A project's
  own configs add only what that project needs.
- Authors must run moon commands from the repo root:
  `moon run <project>:format`, `moon run <project>:lint` and
  `moon run <project>:typecheck`, plus `moon run <project>:build` where it
  exists.
- Formatting fixes should use `pnpm --filter <package> exec prettier --write .`;
  automatic lint fixes should use `pnpm --filter <package> exec eslint --fix .`.
  Authors must review automatic fixes and rerun the moon checks.
- Code must pass lint with no warnings. A justified exception must use
  `// eslint-disable-next-line <rule> -- <reason>` on the narrowest line,
  never a file-wide or block-wide disable.
- Prettier formats every file in a project except Markdown, which keeps the
  repo's style from [documentation.md](../core/documentation.md).
- Comments must follow [code-comments.md](../core/code-comments.md).

## Static export

- `next.config.ts` must set `output: "export"`. The build output is static
  files only; no Node.js process runs beside `lorebayd`.
- Code must not use features that need a Next.js server: Server Actions,
  Route Handlers, middleware, `next/headers`, incremental static
  regeneration, or the default `next/image` loader.
- Pages read and write data in the browser, through the Web API on the same
  origin with the person's sign-in
  ([Web API](../../docs/contracts/web-api.md#4-conventions)).
- Code must not read `process.env`. `next build` must succeed with no
  environment variables, and the built files hold no secret and no
  deployment's configuration.

## Next.js

- Before using a Next.js API, authors must read its guide in the project's
  `node_modules/next/dist/docs/`, which matches the installed release.
- `next.config.ts` must set `agentRules: false`, so `next dev` does not edit
  `AGENTS.md`.
- `next dev` and `next build` must leave the git tree unchanged; files they
  generate, such as `next-env.d.ts`, stay gitignored.
