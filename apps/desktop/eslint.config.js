import eslint from "@eslint/js";
import reactHooks from "eslint-plugin-react-hooks";
import reactRefresh from "eslint-plugin-react-refresh";
import tseslint from "typescript-eslint";

export default tseslint.config(
  {
    ignores: [
      "dist",
      "src-tauri/target",
      "src/routeTree.gen.ts",
      "vite.config.js",
      "vite.config.d.ts",
      "*.tsbuildinfo",
    ],
  },
  eslint.configs.recommended,
  ...tseslint.configs.recommended,
  {
    files: ["**/*.{ts,tsx}"],
    plugins: {
      "react-hooks": reactHooks,
      "react-refresh": reactRefresh,
    },
    rules: {
      ...reactHooks.configs.recommended.rules,
      // TanStack file routes intentionally export both Route and their page component.
      "react-refresh/only-export-components": "off",
      // TanStack Virtual exposes mutable functions that React Compiler cannot memoize.
      "react-hooks/incompatible-library": "off",
    },
  },
);
