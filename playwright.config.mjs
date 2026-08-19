import { defineConfig } from '@playwright/test';

// Overridable because reuseExistingServer means a suite run from one worktree
// silently attaches to a server another worktree already has on this port, and
// then tests that worktree's frontend instead of its own.
const PORT = Number(process.env.XNAUT_TEST_PORT || 4173);

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
