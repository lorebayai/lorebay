import { defineConfig } from "eslint/config";
import tseslint from "typescript-eslint";

export function typescriptConfig(tsconfigRootDir) {
  return defineConfig([
    tseslint.configs.strictTypeChecked,
    {
      languageOptions: {
        parserOptions: {
          projectService: true,
          tsconfigRootDir,
        },
      },
    },
    {
      files: ["eslint.config.js"],
      extends: [tseslint.configs.disableTypeChecked],
    },
  ]);
}
