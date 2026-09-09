// XNAUT-75: a startup failure has to be visible, and say WHAT failed.
//
// The defect this guards is not a crash. XNAUT-74 threw inside init(), the
// throw WAS caught, and the stack DID reach debug.log — and it was invisible
// for as long as it existed, because the only thing pointed at the user was
// alert(), a no-op in Tauri's WKWebView. The window looked broken rather than
// errored, so the report that came back was "nothing works".
//
// The first test reproduces exactly that failure: initChatSessions() throwing
// on a fresh profile, which is the real a132140 case. It is installed from a
// DOMContentLoaded listener registered by addInitScript — that runs before any
// page script, so this listener is registered before app.js's and therefore
// fires first, replacing the function before init() ever reads it. A polled
// setInterval would race init and flake.
import { test, expect } from '@playwright/test';

const THROWN = 'chat-messages is not in the DOM yet';

/** Break one named init step, the way a fresh profile broke it for real. */
async function breakChatSessions(page) {
  await page.addInitScript((message) => {
    document.addEventListener('DOMContentLoaded', () => {
      // app.js is a classic script, so its top-level `function initChatSessions`
      // IS window.initChatSessions, and step() resolves the name at call time.
      window.initChatSessions = () => { throw new Error(message); };
    });
  }, THROWN);
}

async function open(page) {
  await page.goto('/?stub=1');
  // init() is async and its steps await; the seal is the end of the run. The
  // poll answers with WHERE it got to rather than a bare false, because "init
  // never finished" on its own sends you looking for a hang that is really a
  // script that did not load.
  await expect
    .poll(() => page.evaluate(() => {
      const health = window.xnautStartupHealth;
      if (!health) return 'startup-health.js never loaded';
      if (health.sealed()) return 'sealed';
      const steps = health.steps();
      return steps.length ? `stalled after: ${steps[steps.length - 1].step}` : 'no step recorded yet';
    }), { message: 'init() never finished, so nothing can be asserted about it', timeout: 30000 })
    .toBe('sealed');
}

test('a failed init step is visible in the window, and names the step', async ({ page }) => {
  await breakChatSessions(page);
  await open(page);

  const banner = page.locator('#startup-error-banner');
  await expect(banner, 'a step failed and the window said nothing — the XNAUT-74 silence').toHaveCount(1);
  await expect(banner).toContainText('chat sessions');

  // Not a toast: dialogs.js dismisses those after 8 seconds, and a startup
  // failure is a state the app is in, not a passing notice.
  await page.waitForTimeout(9000);
  await expect(banner, 'the startup banner expired like a toast').toHaveCount(1);
});

test('the failure banner does not cover the top bar', async ({ page }) => {
  // The defect update-banner shipped (XNAUT-70): position:fixed at top:0 with
  // no layout offset made every top-bar control unclickable. An error surface
  // that eats the toolbar is worse than the error.
  await breakChatSessions(page);
  await open(page);
  await expect(page.locator('#startup-error-banner')).toHaveCount(1);

  const covered = await page.evaluate(() => {
    const bar = document.querySelector('.top-bar');
    const out = [];
    for (const el of bar.querySelectorAll('button, [role="button"]')) {
      const r = el.getBoundingClientRect();
      if (!r.width || !r.height) continue;
      const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
      if (hit && !el.contains(hit) && !hit.contains(el)) {
        out.push(el.getAttribute('aria-label') || el.title || el.textContent.trim());
      }
    }
    return out;
  });
  expect(covered, `top-bar controls covered by the startup banner: ${JSON.stringify(covered)}`).toEqual([]);
});

test('the record is structured: timestamp, step, error and stack', async ({ page }) => {
  await breakChatSessions(page);
  await open(page);

  const failed = await page.evaluate(() => window.xnautStartupHealth.failures());
  expect(failed.map((f) => f.step)).toContain('chat sessions');
  const entry = failed.find((f) => f.step === 'chat sessions');
  expect(entry.error, 'the error was recorded without what was thrown').toContain(THROWN);
  expect(entry.stack, 'no stack, so the record cannot say WHERE it failed').toBeTruthy();
  expect(Number.isNaN(Date.parse(entry.at)), `unparseable timestamp: ${entry.at}`).toBe(false);
});

test('the detail reports which subsystems came up, not only which failed', async ({ page }) => {
  await breakChatSessions(page);
  await open(page);

  await page.locator('#startup-error-details').click();
  const detail = page.locator('#startup-health-detail');
  await expect(detail).toHaveCount(1);
  // The self-check: a list of only the broken things cannot tell "settings
  // loaded" apart from "settings never ran because we died earlier".
  await expect(detail).toContainText(/\d+\/\d+ subsystems came up/);

  expect(await page.locator('.startup-health-step').count(), 'no steps listed at all').toBeGreaterThan(1);
  await expect(page.locator('.startup-health-step[data-ok="false"][data-step="chat sessions"]')).toHaveCount(1);
  expect(await page.locator('.startup-health-step[data-ok="true"]').count(),
    'not one passing step was recorded, so the detail cannot say what came up').toBeGreaterThan(0);
  await expect(detail, 'the stack is in the record but not on the surface').toContainText(THROWN);
});

test('debug.log is readable from inside the app', async ({ page }) => {
  await breakChatSessions(page);
  await open(page);
  await page.locator('#startup-error-details').click();

  await page.locator('#startup-health-show-log').click();
  const log = page.locator('#startup-health-log');
  await expect(log).toBeVisible();
  await expect(log, 'the tail of debug.log never reached the pane').toContainText('carrying on');

  // And the file itself is reachable, for the case where the tail is not enough.
  await page.locator('#startup-health-reveal').click();
  await expect
    .poll(() => page.evaluate(() => window.__xnautInvokes.filter((i) => i.cmd === 'debug_log_reveal').length))
    .toBe(1);
});

test('startup diagnostics is reachable when nothing failed', async ({ page }) => {
  await open(page);
  // Nothing broken, so no banner — an error surface that is always up is
  // noise, and noise is ignored.
  await expect(page.locator('#startup-error-banner')).toHaveCount(0);

  // A log nobody can find until something breaks is not discoverable, which is
  // the other half of this ticket.
  await page.locator('#btn-more-menu').click();
  await page.locator('#more-menu-dropdown .menu-item[data-action="diagnostics"]').click();

  const detail = page.locator('#startup-health-detail');
  await expect(detail).toHaveCount(1);
  await expect(detail).toContainText(/\d+\/\d+ subsystems came up/);
  await expect(page.locator('.startup-health-step[data-ok="false"]'),
    'a clean startup reported a failed step').toHaveCount(0);

  await page.locator('#startup-health-close').click();
  await expect(detail).toHaveCount(0);
});
