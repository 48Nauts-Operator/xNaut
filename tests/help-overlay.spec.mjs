import { test, expect } from '@playwright/test';

// The main app (app.js) blocks on window.__TAURI__ and will alert() when it's
// missing in a plain browser. Auto-dismiss any dialog so it never blocks the run.
// The Help overlay is intentionally independent of app.js, so it works regardless.
test.beforeEach(async ({ page }) => {
  page.on('dialog', (d) => d.dismiss().catch(() => {}));
  await page.goto('/index.html');
  await page.waitForSelector('#btn-help');
});

test('help button is the last icon in the top bar', async ({ page }) => {
  const ids = await page.$$eval('.top-bar-right > *', (els) =>
    els.map((e) => (e.id || e.querySelector('button')?.id || '')));
  expect(ids[ids.length - 1]).toBe('btn-help');
});

test('clicking the help button slides out the overlay with shortcuts', async ({ page }) => {
  const overlay = page.locator('#help-overlay');
  await expect(overlay).toBeHidden();

  await page.click('#btn-help');

  await expect(overlay).toBeVisible();
  await expect(overlay).toHaveClass(/open/);
  await expect(page.locator('#btn-help')).toHaveAttribute('aria-expanded', 'true');

  // It has actually slid into view: once the transition settles the open
  // transform has no horizontal offset (closed state pushes it fully off-screen).
  await expect.poll(async () => overlay.evaluate((el) =>
    Math.abs(new DOMMatrixReadOnly(getComputedStyle(el).transform).m41)
  ), { timeout: 2000 }).toBeLessThan(2);

  // Shows real shortcuts.
  await expect(page.locator('#help-overlay-title')).toHaveText('Keyboard Shortcuts');
  await expect(page.getByText('New Tab', { exact: true })).toBeVisible();
  const kbds = page.locator('#help-overlay-list .help-kbd');
  expect(await kbds.count()).toBeGreaterThan(3);
  // Ctrl+T for New Tab is rendered as separate kbd chips.
  await expect(page.locator('#help-overlay-list .help-kbd', { hasText: 'Ctrl' }).first()).toBeVisible();
});

test('closes via the close button', async ({ page }) => {
  await page.click('#btn-help');
  await expect(page.locator('#help-overlay')).toHaveClass(/open/);
  await page.click('#btn-help-close');
  await expect(page.locator('#help-overlay')).not.toHaveClass(/open/);
  await expect(page.locator('#help-overlay')).toBeHidden();
});

test('Escape key closes the overlay', async ({ page }) => {
  await page.click('#btn-help');
  await expect(page.locator('#help-overlay')).toHaveClass(/open/);
  await page.keyboard.press('Escape');
  await expect(page.locator('#help-overlay')).not.toHaveClass(/open/);
});

test('"?" key toggles the overlay open', async ({ page }) => {
  await expect(page.locator('#help-overlay')).toBeHidden();
  await page.keyboard.press('Shift+Slash'); // "?"
  await expect(page.locator('#help-overlay')).toHaveClass(/open/);
});

test('clicking the backdrop closes the overlay', async ({ page }) => {
  await page.click('#btn-help');
  await expect(page.locator('#help-overlay')).toHaveClass(/open/);
  await page.click('#help-overlay-backdrop', { position: { x: 10, y: 10 } });
  await expect(page.locator('#help-overlay')).not.toHaveClass(/open/);
});

test('reflects a user-customized keybinding from localStorage', async ({ page }) => {
  await page.evaluate(() => {
    localStorage.setItem('xnaut-keybindings', JSON.stringify({
      newTab: { key: 'n', ctrl: true, shift: true, label: 'New Tab' },
    }));
  });
  await page.reload();
  page.on('dialog', (d) => d.dismiss().catch(() => {}));
  await page.click('#btn-help');
  const row = page.locator('.help-row', { hasText: 'New Tab' });
  await expect(row.locator('.help-kbd', { hasText: 'Shift' })).toBeVisible();
});
