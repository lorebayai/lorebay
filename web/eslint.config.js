import { typescriptConfig } from "../eslint.base.js";

export default [
  { ignores: ["out/**"] },
  ...typescriptConfig(import.meta.dirname),
];
