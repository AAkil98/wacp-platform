import js from "@eslint/js";
import tseslint from "typescript-eslint";
import reactHooks from "eslint-plugin-react-hooks";
import jsxA11y from "eslint-plugin-jsx-a11y";
import globals from "globals";

// HEALTH-LOG §2.4 / frontend-perf-plan F8 — preventive ruleset for the
// recurring a11y label-binding gap pattern (two incidents before it was
// flagged: `d71c4fe` + `e870018`). The `jsx-a11y/recommended` preset
// fails-fast on missing htmlFor/id bindings, alt text, ARIA role
// correctness, and keyboard-handler pairing.

export default tseslint.config(
  { ignores: ["dist"] },
  {
    extends: [js.configs.recommended, ...tseslint.configs.recommended],
    files: ["**/*.{ts,tsx}"],
    languageOptions: {
      globals: globals.browser,
    },
    plugins: {
      "react-hooks": reactHooks,
      "jsx-a11y": jsxA11y,
    },
    rules: {
      ...jsxA11y.configs.recommended.rules,
      "@typescript-eslint/no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_" },
      ],
      "react-hooks/rules-of-hooks": "error",
      "react-hooks/exhaustive-deps": "warn",
      // Keep the recurring §2.4 label-binding gap at error (two prior
      // incidents: d71c4fe + e870018) — the reason F8 exists.
      "jsx-a11y/label-has-associated-control": "error",
      "jsx-a11y/no-autofocus": "error",
      // Downgraded from error to warn pending a keyboard-navigation sweep
      // across the six `<div onClick>` card + row patterns (ProfilesSidebar
      // row, Wizard vertical/workflow cards, EscalationInbox row,
      // TrailStream row, LoginPage/ChangePasswordPage outer wrappers). Each
      // needs role="button" + tabIndex + onKeyDown{Enter,Space} — tracked
      // as frontend-perf-plan §8 follow-up.
      "jsx-a11y/click-events-have-key-events": "warn",
      "jsx-a11y/no-static-element-interactions": "warn",
    },
  },
);
