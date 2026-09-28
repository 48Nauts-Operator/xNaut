import { defineConfig } from '@playwright/test';
import base from './playwright.config.mjs';

// Native macOS uses WebKit. Chromium alone misses its pointer/focus ordering.
export default defineConfig({
  ...base,
  testMatch: /(?:voice-live|agent-space-voice)\.spec\.mjs/,
  projects: [
    { name: 'chromium', use: { browserName: 'chromium' } },
    { name: 'webkit', use: { browserName: 'webkit' } },
  ],
});
