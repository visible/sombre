import { defineConfig } from "oxlint";
import core from "ultracite/oxlint/core";
import next from "ultracite/oxlint/next";
import react from "ultracite/oxlint/react";

export default defineConfig({
  env: core.env,
  extends: [core, react, next],
  ignorePatterns: [...core.ignorePatterns, "apps/desktop/src/lib/bindings.ts"],
  overrides: [
    {
      files: ["crates/cli/src/phone.js"],
      rules: { "no-bitwise": "off" },
    },
    {
      files: ["apps/desktop/**"],
      rules: { "nextjs/no-img-element": "off" },
    },
  ],
});
