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

// ---------------------------------------------------------------------------
// XNAUT-70: the download half.
//
// The payload had never been retrieved by any client on any machine, and the
// reason nobody could say WHY is that showUpdateBanner() raced
// downloadAndInstall() against a 60s timer. When the timer won, the real
// rejection lost the race and became a promise with no handler — so the button
// said "Failed - download manually" whether the request had 404ed, stalled on
// a socket, or simply not finished a 26.5MB download yet.
//
// These tests drive a fake update object, because the thing under test is the
// frontend's contract with tauri-plugin-updater: does it hand Rust a timeout,
// does it subscribe to progress, and does it report what actually happened.
//
// The plugin's event shape is fixed by updater/src/commands.rs (DownloadEvent,
// #[serde(tag="event", content="data")] with camelCase fields), so the fakes
// below emit exactly {event:'Started',data:{contentLength}} etc.

const MB = 1048576;

/** Installs a controllable window.__TAURI__.updater before any page script.
 *
 * The stub in tests/static-server.mjs deliberately has no updater, so without
 * this checkForUpdates() takes the GitHub-API path and never produces an
 * updateObj — which is the path the three tests above cover. */
async function withFakeUpdater(page, { withProcess = true } = {}) {
  await page.addInitScript(({ withProcess }) => {
    const probe = { calls: 0, options: null, emit: null, settled: null, relaunched: 0 };
    window.__updateProbe = () => ({ calls: probe.calls, options: probe.options, relaunched: probe.relaunched, subscribed: typeof probe.emit === 'function' });
    window.__updateEmit = (event) => { if (probe.emit) probe.emit(event); };
    window.__updateResolve = () => { if (probe.settled) probe.settled.resolve(); };
    window.__updateReject = (why) => { if (probe.settled) probe.settled.reject(new Error(why)); };

    const install = setInterval(() => {
      if (!window.__TAURI__) return;
      clearInterval(install);
      window.__TAURI__.updater = {
        check: () => Promise.resolve({
          available: true,
          version: '99.0.0',
          downloadAndInstall: (onEvent, options) => {
            probe.calls += 1;
            probe.emit = onEvent;
            probe.options = options || null;
            return new Promise((resolve, reject) => { probe.settled = { resolve, reject }; });
          },
        }),
      };
      if (withProcess) {
        window.__TAURI__.process = { relaunch: () => { probe.relaunched += 1; return Promise.resolve(); } };
      }
    }, 5);
  }, { withProcess });
}

/** Opens the app, waits for the banner, and sets the stall threshold so a test
 * does not have to wait the production 90 seconds for the watchdog.
 *
 * The button is located by position, not by text: its label is the thing under
 * test and changes as the download runs, so a hasText locator would stop
 * matching the moment the assertions get interesting. */
async function openWithBanner(page, stallMs = 20000) {
  await page.goto('/?stub=1');
  const banner = page.locator('#update-banner');
  await expect(banner).toHaveCount(1, { timeout: 15000 });
  await page.evaluate((ms) => { window.xnautUpdateStallMs = ms; }, stallMs);
  return banner.locator('button').first();
}

/** Click, then wait until the download has actually been handed to the fake —
 * dispatching the click is not the same as the handler having run. */
async function startDownload(page, btn) {
  await btn.click();
  await expect.poll(() => page.evaluate(() => window.__updateProbe().subscribed),
    { message: 'the click never reached downloadAndInstall()' }).toBe(true);
}

test('the download is given a request timeout, so a wedged socket cannot hang forever', async ({ page }) => {
  await withFakeUpdater(page);
  const btn = await openWithBanner(page);
  await startDownload(page, btn);

  const probe = await page.evaluate(() => window.__updateProbe());
  expect(probe.calls, 'Update Now did not start a download').toBe(1);
  // tauri-plugin-updater applies a reqwest timeout only when one is passed
  // (updater.rs: `if let Some(timeout) = self.timeout`). Passing none is what
  // let a stalled connect hang with no error for the JS timer to explain away.
  expect(typeof probe.options?.timeout,
    'downloadAndInstall() was called with no timeout, so Rust builds a client that never gives up').toBe('number');
  expect(probe.options.timeout).toBeGreaterThan(60000);
  expect(probe.subscribed, 'no onEvent handler, so progress is unobservable').toBe(true);
});

test('the button reports real bytes instead of a fixed "Downloading..."', async ({ page }) => {
  await withFakeUpdater(page);
  const btn = await openWithBanner(page);
  await startDownload(page, btn);

  await page.evaluate((total) => window.__updateEmit({ event: 'Started', data: { contentLength: total } }), 26 * MB);
  await page.evaluate((chunk) => window.__updateEmit({ event: 'Progress', data: { chunkLength: chunk } }), 3 * MB);

  await expect(btn).toHaveText(/3\.0 MB \/ 26\.0 MB/);
});

test('a download slower than the old 60s timer is not called a failure', async ({ page }) => {
  await withFakeUpdater(page);
  const btn = await openWithBanner(page, 400);
  await startDownload(page, btn);
  await page.evaluate((total) => window.__updateEmit({ event: 'Started', data: { contentLength: total } }), 26 * MB);

  // Bytes keep arriving, slowly, for longer than several watchdog ticks. The
  // old code declared failure on the wall clock alone; this one must not.
  for (let i = 0; i < 16; i++) {
    await page.waitForTimeout(150);
    await page.evaluate((chunk) => window.__updateEmit({ event: 'Progress', data: { chunkLength: chunk } }), MB / 4);
  }

  await expect(btn, 'a healthy but slow download was reported as failed').not.toHaveText(/fail|manually|Stalled|No response/i);
  await expect(btn).toHaveText(/4\.0 MB \/ 26\.0 MB/);
});

test('a request that never answers is reported as no response, not as a generic failure', async ({ page }) => {
  await withFakeUpdater(page);
  const btn = await openWithBanner(page, 400);
  await startDownload(page, btn);

  // Nothing emitted at all: this is the "0 downloads of the payload" case.
  await expect(btn).toHaveText(/No response/, { timeout: 5000 });

  // And when the real rejection finally lands it is still reported, with the
  // phase it died in. Under the old race this error had no handler at all.
  await page.evaluate(() => window.__updateReject('error sending request for url'));
  await expect(page.locator('#update-banner')).toContainText(/request failed: error sending request for url/);
});

test('an install failure is not reported as a download failure', async ({ page }) => {
  await withFakeUpdater(page);
  const btn = await openWithBanner(page);
  await startDownload(page, btn);

  await page.evaluate((total) => window.__updateEmit({ event: 'Started', data: { contentLength: total } }), 26 * MB);
  await page.evaluate((chunk) => window.__updateEmit({ event: 'Progress', data: { chunkLength: chunk } }), 26 * MB);
  await page.evaluate(() => window.__updateEmit({ event: 'Finished' }));
  await expect(btn).toHaveText(/Installing/);

  await page.evaluate(() => window.__updateReject('Failed to move the new app into place'));
  await expect(page.locator('#update-banner')).toContainText(/install failed: Failed to move the new app into place/);
});

test('an installed update offers a restart instead of sitting on "Downloading..."', async ({ page }) => {
  await withFakeUpdater(page);
  const btn = await openWithBanner(page);
  await startDownload(page, btn);
  await page.evaluate(() => window.__updateEmit({ event: 'Finished' }));

  // downloadAndInstall() resolving means the new app is on disk and we are
  // still the old process. tauri-plugin-updater never relaunches on macOS, so
  // the old code's "Tauri will restart automatically" left the button disabled
  // on "Downloading..." forever — on a SUCCESSFUL update.
  await page.evaluate(() => window.__updateResolve());
  await expect(btn).toHaveText(/Restart now/);
  await expect(page.locator('#update-banner')).toContainText(/is installed/);

  await btn.click();
  const probe = await page.evaluate(() => window.__updateProbe());
  expect(probe.relaunched, 'Restart now did not relaunch the app').toBe(1);
});

test('without the process plugin the banner says how to finish, and does not pretend to restart', async ({ page }) => {
  await withFakeUpdater(page, { withProcess: false });
  const btn = await openWithBanner(page);
  await startDownload(page, btn);
  await page.evaluate(() => window.__updateResolve());

  await expect(btn).toHaveText(/Quit and reopen/);
});
