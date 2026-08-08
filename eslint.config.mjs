// One rule, on purpose: no-undef.
//
// Two bugs in one evening (2026-08-08) were both an identifier that did not
// exist, and neither was caught by anything we had:
//
//   * `window.xnautActiveProjectKey` — never defined. An undefined property read
//     is a silent no-op, so a feature quietly did nothing.
//   * `roleFrontierModel` reached for `project`, a parameter of its callers. An
//     undeclared IDENTIFIER throws ReferenceError, which killed the NAUT-Flow
//     render and made the tab look unclickable.
//
// `node --check`, a clean Rust build and 295 Rust tests all passed — none of them
// resolve scopes. A Playwright smoke test did not catch it either: reaching that
// code needs a mounted panel with real data, and faking enough of the app to get
// there is a bigger fiction than the bug is worth. Static analysis gets it for
// free, in milliseconds, with no fixtures.
//
// Deliberately NOT a style config. Everything about how this codebase is written
// — quotes, semicolons, line length — is left alone; adding opinions would bury
// the one signal that matters in a thousand warnings nobody reads.

import globals from 'globals';

export default [
  {
    files: ['src/js/**/*.js'],
    languageOptions: {
      ecmaVersion: 2022,
      sourceType: 'script', // classic <script> tags, not modules
      globals: {
        ...globals.browser,
        // Injected by dependencies loaded ahead of our own scripts.
        Terminal: 'readonly',
        FitAddon: 'readonly',
        WebLinksAddon: 'readonly',
        SearchAddon: 'readonly',
        marked: 'readonly',
        hljs: 'readonly',
        mermaid: 'readonly',
        XnautRete: 'readonly',
        Chart: 'readonly',
      },
    },
    linterOptions: { reportUnusedDisableDirectives: true },
    rules: {
      // The whole point.
      'no-undef': 'error',
      // Its quieter sibling: an assignment that creates an implicit global.
      'no-implicit-globals': 'off',
      // Catches `if (x = 1)` and similar, which are the same shape of typo.
      'no-cond-assign': 'error',
      'no-dupe-keys': 'error',
      'no-unreachable': 'error',
    },
  },
  {
    // Vendored bundles are not ours to lint.
    ignores: ['src/js/vendor/**'],
  },
];
