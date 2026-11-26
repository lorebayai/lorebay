import { execFileSync } from "node:child_process";
import { copyFile, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { ESLint } from "eslint";
import * as prettier from "prettier";
import tseslint from "typescript-eslint";
import { typescriptConfig } from "../eslint.base.js";

const require = createRequire(import.meta.url);
const source = fileURLToPath(
  new URL("../lib/mcp/src/request-card.html", import.meta.url),
);
const checker = fileURLToPath(import.meta.url);
const html = await readFile(source, "utf8");
const scripts = [
  ...html.matchAll(/<script\b([^>]*)>([\s\S]*?)<\/script\s*>/gi),
];
if (scripts.length !== 1 || scripts[0][1] !== ' type="module"') {
  throw new Error("The request card must contain one inline module");
}

await checkFormat(source, html);
await checkFormat(checker, await readFile(checker, "utf8"));
await checkLint(
  checker,
  new ESLint({
    cwd: fileURLToPath(new URL("..", import.meta.url)),
    overrideConfigFile: true,
    overrideConfig: tseslint.configs.strict,
  }),
);

const directory = await mkdtemp(join(tmpdir(), "request-card-"));
try {
  const tsconfig = join(directory, "tsconfig.json");
  await copyFile(
    new URL("../tsconfig.base.json", import.meta.url),
    join(directory, "tsconfig.base.json"),
  );
  await writeFile(
    tsconfig,
    `{
    "extends": "./tsconfig.base.json",
    "compilerOptions": {"allowJs": true, "checkJs": true},
    "include": ["request-card.js"]
  }`,
  );
  await writeFile(join(directory, "request-card.js"), scripts[0][2]);
  execFileSync(
    process.execPath,
    [require.resolve("typescript/bin/tsc"), "--project", tsconfig],
    {
      stdio: "inherit",
    },
  );
  await checkLint(
    "request-card.js",
    new ESLint({
      cwd: directory,
      overrideConfigFile: true,
      overrideConfig: typescriptConfig(directory),
    }),
  );
} finally {
  await rm(directory, { recursive: true, force: true });
}

async function checkFormat(filepath, content) {
  const config = await prettier.resolveConfig(filepath);
  if (!(await prettier.check(content, { ...config, filepath }))) {
    throw new Error(`Format ${filepath} with Prettier`);
  }
}

async function checkLint(path, eslint) {
  const results = await eslint.lintFiles(path);
  if (
    results.some(({ errorCount, warningCount }) => errorCount || warningCount)
  ) {
    const formatter = await eslint.loadFormatter("stylish");
    throw new Error(await formatter.format(results));
  }
}
