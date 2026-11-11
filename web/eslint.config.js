import js from "@eslint/js";
import reactHooks from "eslint-plugin-react-hooks";
import reactRefresh from "eslint-plugin-react-refresh";
import globals from "globals";
import tseslint from "typescript-eslint";

export default tseslint.config(
  // Generated: ts-rs bindings and the router's route tree.
  { ignores: ["dist", "node_modules", "src/api/types", "src/routeTree.gen.ts"] },
  js.configs.recommended,
  tseslint.configs.recommended,
  {
    files: ["src/**/*.{ts,tsx}", "vite.config.ts"],
    languageOptions: { globals: { ...globals.browser } },
    plugins: { "react-hooks": reactHooks, "react-refresh": reactRefresh },
    rules: {
      ...reactHooks.configs.recommended.rules,
      "react-refresh/only-export-components": ["error", { allowConstantExport: true }],
    },
  },
  {
    // Route files export `Route` next to their component; shadcn files export variants next to components.
    files: ["src/routes/**", "src/components/ui/**", "src/shell/theme.tsx"],
    rules: { "react-refresh/only-export-components": "off" },
  },
  {
    files: ["scripts/**/*.mjs"],
    languageOptions: { globals: { ...globals.node } },
  },
);
