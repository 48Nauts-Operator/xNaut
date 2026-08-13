// Delta test for the update banner, from tron's run 20260813-124712.
//
// Two defects, independent of each other, both reachable from here:
//   1. CURRENT_VERSION is a hand-maintained constant that stopped being updated
//      at 1.5.0, so the GitHub fallback concludes an update exists for every
//      release after that one, including the one that is already running.
//   2. The banner is position:fixed at top:0 with no layout offset, so it lands
//      on top of the top bar instead of pushing it down. That one hits every
//      user the moment a genuine update exists, whatever CURRENT_VERSION says.
//
// The stub has no window.__TAURI__.updater, so checkForUpdates() takes the
// GitHub-API path. We answer that fetch ourselves rather than reaching the real
// release, so the test asserts on the comparison and not on what happens to be
// tagged today.
import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';

const APP_VERSION = JSON.parse(
  await readFile(new URL('../src-tauri/tauri.conf.json', import.meta.url), 'utf8'),
).version;

const RELEASES = 'https://api.github.com/repos/48Nauts-Operator/xNaut/releases/latest';

// checkForUpdates() is fired on a 3s timer; the fetch and the banner follow it.
const AFTER_UPDATE_CHECK = 6000;

async function openWithLatestRelease(page, tag) {
  await page.route(RELEASES, (route) => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({ tag_name: tag, html_url: 'https://example.invalid/release' }),
  }));
  await page.goto('/?stub=1');
  await page.waitForTimeout(AFTER_UPDATE_CHECK);
}

// Counting calls to getVersion would not prove much, because the usage footer
// asks for it too. This is the assertion that pins it: if the update check went
// back to comparing against a constant, a rejected getVersion would not stop it
// and the banner would appear anyway.
test('no update is offered when the running version cannot be determined', async ({ page }) => {
  await page.addInitScript(() => {
    const install = setInterval(() => {
      const app = window.__TAURI__?.app;
      if (!app?.getVersion) return;
      clearInterval(install);
      app.getVersion = () => Promise.reject(new Error('no version'));
    }, 10);
  });

  await openWithLatestRelease(page, 'v99.0.0');

  await expect(page.locator('#update-banner'),
    'offered an update without knowing what it was updating from').toHaveCount(0);
});

test('no update is offered when the latest release is the running one', async ({ page }) => {
  await openWithLatestRelease(page, `v${APP_VERSION}`);

  const banner = page.locator('#update-banner');
  await expect(banner, `offered an update from ${APP_VERSION} to ${APP_VERSION}`).toHaveCount(0);
});

test('a real update does not cover the top bar', async ({ page }) => {
  // Far enough ahead that this stays a genuine update whatever we ship next.
  await openWithLatestRelease(page, 'v99.0.0');

  const banner = page.locator('#update-banner');
  await expect(banner, 'expected a banner for a genuinely newer release').toHaveCount(1);

  // Every top-bar control must still be the thing under its own coordinates.
  const covered = await page.evaluate(() => {
    const bar = document.querySelector('.top-bar');
    const out = [];
    for (const el of bar.querySelectorAll('button, [role="button"]')) {
      const r = el.getBoundingClientRect();
      if (!r.width || !r.height) continue;
      const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
      if (hit && !el.contains(hit) && !hit.contains(el)) {
        out.push({ name: el.getAttribute('aria-label') || el.title || el.textContent.trim(), by: hit.id || hit.className });
      }
    }
    return out;
  });

  expect(covered, `top-bar controls covered by the banner: ${JSON.stringify(covered)}`).toEqual([]);
});
