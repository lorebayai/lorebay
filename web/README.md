# @lorebay/web

Plain TypeScript web pages built into static files that the lorebayd
executable embeds and serves.

## Usage

Run from the repository root:

```bash
moon run web:format web:lint web:typecheck web:build
moon run lorebayd:build
```

`web:build` compiles the page script with TypeScript and copies its HTML
into `web/out/`. The output is ignored by git. The server build and test
tasks build these files first.

## Docs

Start at the [docs overview](../docs/overview.md).
