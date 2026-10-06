// A Project Management module that is switched off must read as a setup step,
// not as a failure. XNAUT-124.
//
// The bug was an ORDERING bug, not a backend one. load() called the pm_* data
// commands first, and every one of them returns Err while the module is
// disabled — so the very first invoke threw, pm_module_status (the thing that
// explains WHY) was never fetched, and the catch block painted a red error box
// with a Retry button that could never succeed. The delegated 1.13.7 run on
// tron recorded it as the one failing scenario of the release.
//
// Driven through ?stub=1 so the bridge is the server's, and the stub map is
// mutated per scenario rather than the server growing a query flag each time.

import { test, expect } from '@playwright/test';

/** Set pm_module_status, mount a fresh panel, hand back its root. */
async function mountWith(page, status) {
  await page.evaluate((s) => {
    window.__xnautStub.pm_module_status = s;
    document.querySelectorAll('#pm-test-host').forEach((n) => n.remove());
    const host = document.createElement('div');
    host.id = 'pm-test-host';
    document.body.appendChild(host);
    window.xnautCreateProjectManagementPanel('pm-test', host, { project: 'SMOKE' });
  }, status);
  return page.locator('#pm-test-host .pmw');
}

const OK = {
  enabled: true, configured: true, valid: true, repo_path: '/tmp/smoke-control',
  remote_url: '', git_repository: true, project_count: 1, ticket_count: 0,
  error: '', warning: '', branch: 'main', last_commit: '', dirty: false, ahead: 0, behind: 0,
};

test.beforeEach(async ({ page }) => {
  page.on('dialog', (d) => d.dismiss().catch(() => {}));
  await page.goto('/index.html?stub=1');
  await page.waitForFunction(() => typeof window.xnautCreateProjectManagementPanel === 'function');
});

test('a configured module paints the board, not the setup prompt', async ({ page }) => {
  const pane = await mountWith(page, OK);
  await expect(pane.locator('[data-pm-setup]')).toHaveCount(0);
  await expect(pane.locator('.pmw-error')).toHaveCount(0);
});

test('a disabled module offers setup instead of an error', async ({ page }) => {
  const pane = await mountWith(page, { ...OK, enabled: false });

  await expect(pane.locator('[data-pm-setup]')).toBeVisible();
  // The whole point: no red box, and no Retry that cannot work.
  await expect(pane.locator('.pmw-error')).toHaveCount(0);
  await expect(pane.locator('[data-pm-retry]')).toHaveCount(0);
  await expect(pane.locator('.pmw-sync-state')).toHaveText('Module off');
  await expect(pane.locator('.pmw-sync')).toBeDisabled();
});

test('a configured-but-broken module surfaces the backend reason', async ({ page }) => {
  const pane = await mountWith(page, {
    ...OK, valid: false, error: 'control repository has no .git directory',
  });
  await expect(pane.locator('.pmw-content')).toContainText('control repository has no .git directory');
  await expect(pane.locator('.pmw-error')).toHaveCount(0);
  await expect(pane.locator('.pmw-sync-state')).toHaveText('Not configured');
});

test('the setup button opens Settings on the module card', async ({ page }) => {
  const pane = await mountWith(page, { ...OK, enabled: false });
  await page.evaluate(() => {
    window.__pmSection = null;
    window.loadSettingsSection = (s) => { window.__pmSection = s; };
  });

  await pane.locator('[data-pm-setup]').click();

  expect(await page.evaluate(() => window.__pmSection)).toBe('tasksmode');
  expect(await page.evaluate(() =>
    getComputedStyle(document.getElementById('settings-panel')).display)).toBe('flex');
});

test('first hidden Settings opening from PM setup settles the real module panel', async ({ page }) => {
  const pane = await mountWith(page, { ...OK, enabled: false });
  await expect(pane.locator('[data-pm-setup]')).toBeVisible();
  const state = await page.evaluate(() => {
    Object.defineProperty(document, 'hidden', { configurable: true, get: () => true });
    window.requestAnimationFrame = () => 0;
    const focus = document.activeElement;
    document.querySelector('#pm-test-host [data-pm-setup]').click();
    const panel = document.getElementById('settings-panel');
    const s = getComputedStyle(panel);
    return { opacity: s.opacity, animation: s.animationName, display: s.display,
      section: document.querySelector('.settings-nav-item.active')?.dataset.section,
      focusUnchanged: focus === document.activeElement };
  });
  expect(state).toEqual({ opacity: '1', animation: 'none', display: 'flex', section: 'tasksmode', focusUnchanged: true });
  await expect(page.locator('#tm-llm-endpoint')).toBeVisible();
  await page.locator('#btn-close-settings-panel').click();
  await expect(page.locator('#settings-panel')).toBeHidden();
});
