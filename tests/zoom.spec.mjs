import { test, expect } from '@playwright/test';

// Cmd/Ctrl +/- has to zoom the WHOLE interface, not only a markdown view or a
// terminal. It did nothing in Agent Space, which is where the reading happens.
test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem('xnaut-sidebar-visible', '1'));
  await page.goto('/?stub=1');
  await page.waitForTimeout(900);
});

test('Cmd +/- zooms the interface and Cmd 0 resets it', async ({ page }) => {
  await page.getByRole('button', { name: 'More surfaces', exact: true }).click();
  await page.locator('.sbar-menu-item', { hasText: 'Agent Space' }).click();
  const zoomOf = () => page.evaluate(() => document.documentElement.style.zoom || '1');
  expect(await zoomOf()).toBe('1');

  await page.keyboard.press('Meta+=');
  const bigger = Number(await zoomOf());
  expect(bigger).toBeGreaterThan(1);

  await page.keyboard.press('Meta+=');
  expect(Number(await zoomOf())).toBeGreaterThan(bigger);

  await page.keyboard.press('Meta+-');
  expect(Number(await zoomOf())).toBe(bigger);

  await page.keyboard.press('Meta+0');
  expect(Number(await zoomOf())).toBe(1); // reset clears the inline style
});

test('the chosen zoom survives a reload', async ({ page }) => {
  await page.keyboard.press('Meta+=');
  const chosen = await page.evaluate(() => document.documentElement.style.zoom);
  await page.reload();
  await page.waitForTimeout(900);
  expect(await page.evaluate(() => document.documentElement.style.zoom)).toBe(chosen);
});

test('a native browser pane is told bounds in unzoomed points', async ({ page }) => {
  // CSS zoom scales getBoundingClientRect; the child webview is placed in
  // unzoomed points, so without compensation the pane drifts off its
  // placeholder the moment the interface is zoomed.
  await page.keyboard.press('Meta+=');
  const factor = await page.evaluate(() => window.xnautUiZoom);
  expect(factor).toBeGreaterThan(1);
});

// Andre, 2026-09-13, zoomed to 1.25: "the footer bar is missing with the usage
// and the scrolling is not working until the end". body and #app were 100vh,
// and the viewport unit scales with CSS zoom, so the column was 125% of the
// window and the last rows, the usage footer and the status bar sat below it.
test('a zoomed interface still fits the window', async ({ page }) => {
  await page.evaluate(() => {
    window.__xnautStub.pm_project_list = Array.from({ length: 44 }, (_, i) => ({
      key: `P${i}`, name: `Project ${i}`, source_path: `/tmp/p${i}`, stage: '', flow_type: '', revision: 1,
    }));
  });
  await page.setViewportSize({ width: 1400, height: 900 });
  await page.keyboard.press('Meta+=');
  await page.keyboard.press('Meta+=');
  await page.waitForTimeout(1500);
  const m = await page.evaluate(() => {
    const bottom = (sel) => Math.round(document.querySelector(sel).getBoundingClientRect().bottom);
    const list = document.querySelector('.sbar-projects');
    list.scrollTop = 1e9;
    const rows = document.querySelectorAll('.sbar-projects .sbar-row');
    return {
      zoom: Number(document.documentElement.style.zoom),
      win: window.innerHeight,
      app: bottom('#app'),
      usage: bottom('.sbar-usage'),
      lastRow: Math.round(rows[rows.length - 1].getBoundingClientRect().bottom),
    };
  });
  expect(m.zoom).toBeGreaterThan(1);
  expect(m.app).toBeLessThanOrEqual(m.win);
  expect(m.usage).toBeLessThanOrEqual(m.win);
  expect(m.lastRow).toBeLessThanOrEqual(m.usage);
});
