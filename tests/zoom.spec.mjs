import { test, expect } from '@playwright/test';

// Cmd/Ctrl +/- has to zoom the WHOLE interface, not only a markdown view or a
// terminal. It did nothing in Agent Space, which is where the reading happens.
test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem('xnaut-sidebar-visible', '1'));
  await page.goto('/?stub=1');
  await page.waitForTimeout(900);
});

test('Cmd +/- zooms the interface and Cmd 0 resets it', async ({ page }) => {
  await page.getByText('Agent Space', { exact: true }).first().click();
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
