import { defineConfig } from '@playwright/test';

// Overridable: the static server is shared by port, and playwright reuses any
// server already on it. Two concurrent runs from different worktrees then serve
// each other's src/, so a mutated file is never the one under test.
const PORT = Number(process.env.PW_PORT || 4173);

export default defineConfig({
  globalSetup: './tests/global-setup.mjs',
  testDir: './tests',
  timeout: 30000,
  fullyParallel: false,
  reporter: [['list']],
  use: {
    baseURL: `http://127.0.0.1:${PORT}`,
    trace: 'off',
  },
  webServer: {
    command: `node tests/static-server.mjs`,
    port: PORT,
    reuseExistingServer: true,
    env: { PORT: String(PORT) },
  },
});
