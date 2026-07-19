import { defineConfig } from '@playwright/test';

const PORT = 4173;

export default defineConfig({
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
