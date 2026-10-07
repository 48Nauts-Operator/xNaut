// XNAUT-443: a missing browser is verifier unavailability, never a failed test.
// The fleet sets PLAYWRIGHT_BROWSERS_PATH to persistent application data before
// starting Node. Honor operator/CI overrides and the installed Playwright version.
import { spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';

const require = createRequire(import.meta.url);
const missingBrowser = (error) => /Executable doesn't exist|No Playwright browser is installed/i.test(String(error));

export function installedPlaywrightCli() {
  return join(dirname(require.resolve('playwright/package.json')), require('playwright/package.json').bin.playwright);
}

export async function ensureBrowser(probe, install) {
  try { await probe(); return; }
  catch (error) {
    if (!missingBrowser(error)) {
      throw new Error(`Verifier unavailable: Playwright could not start Chromium: ${error}`);
    }
  }
  // Once per verification process, bounded by install's timeout. Re-probe the
  // real browser after installation: exit zero alone does not prove readiness.
  try { await install(); await probe(); }
  catch (error) {
    throw new Error(`Verifier unavailable: Playwright browser setup failed: ${error}`);
  }
}

export default async function globalSetup() {
  const { chromium } = await import('@playwright/test');
  await ensureBrowser(async () => {
    const browser = await chromium.launch({ headless: true, timeout: 15_000 });
    await browser.close();
  }, () => {
    const cli = installedPlaywrightCli();
    const result = spawnSync(process.execPath, [cli, 'install', 'chromium'], {
      env: process.env, encoding: 'utf8', timeout: 180_000, maxBuffer: 4 * 1024 * 1024,
    });
    if (result.error || result.status !== 0) {
      throw new Error(result.error?.message || `installer exited ${result.status}: ${(result.stderr || result.stdout || '').slice(-4000)}`);
    }
  });
}
