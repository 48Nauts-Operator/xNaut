import { defineConfig } from '@playwright/test';
import base from './playwright.config.mjs';

// A release test must serve this checkout. Attaching to another worktree's
// already-running development server can make the wrong code appear green.
export default defineConfig({
  ...base,
  retries: 0,
  forbidOnly: true,
  webServer: { ...base.webServer, reuseExistingServer: false },
});
