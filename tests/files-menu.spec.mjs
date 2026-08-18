import { test, expect } from '@playwright/test';

// Right-click a file, get "Copy path". Reported missing after the app-zoom
// change, which is exactly the kind of thing a fixed-position menu breaks on.
test('the files context menu appears where the click was, zoomed or not', async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.setItem('xnaut-sidebar-visible', '1');
    localStorage.setItem('xnaut-right-pane-visible', '1');
  });
  await page.goto('/?stub=1');
  await page.waitForTimeout(900);

  await page.locator('[data-rpane-view="files"]').click();
  const row = page.locator('.rpf-row').filter({ hasText: 'README.md' }).first();
  await expect(row).toBeVisible();

  // Dispatched rather than clicked: what matters is that the handler is
  // reachable and the menu lands on screen.
  const openMenu = () => page.evaluate(() => {
    const target = [...document.querySelectorAll('.rpf-row')].find((r) => r.textContent.includes('README.md'));
    target.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, clientX: 240, clientY: 220 }));
  });
  await openMenu();
  const menu = page.locator('.rpf-menu');
  await expect(menu).toBeVisible();
  await expect(menu.getByRole('button', { name: 'Copy path', exact: true })).toBeVisible();

  // Unzoomed, it opens at the click.
  const atClick = await page.evaluate(() => {
    const box = document.querySelector('.rpf-menu').getBoundingClientRect();
    return { left: box.left, top: box.top };
  });
  expect(Math.abs(atClick.left - 240)).toBeLessThan(12);
  expect(Math.abs(atClick.top - 220)).toBeLessThan(12);

  // Now zoomed. The interface zooms with CSS `zoom` on the root, so a fixed
  // menu is laid out in a space multiplied by the zoom while clientX/Y arrive
  // already zoomed: assigning one to the other put the menu at click x zoom,
  // a third of the way down the screen from the row that was right-clicked.
  // Reported with a screenshot 2026-08-18. It must still land AT the click.
  await page.keyboard.press('Escape');
  await page.evaluate(() => { window.xnautAdjustAppZoom(1); window.xnautAdjustAppZoom(1); });
  expect(await page.evaluate(() => Number(window.xnautUiZoom))).toBeCloseTo(1.25, 2);
  await openMenu();
  await expect(menu).toBeVisible();
  const zoomed = await page.evaluate(() => {
    const box = document.querySelector('.rpf-menu').getBoundingClientRect();
    return { left: box.left, top: box.top };
  });
  expect(Math.abs(zoomed.left - 240)).toBeLessThan(12);
  expect(Math.abs(zoomed.top - 220)).toBeLessThan(12);
  const placed = await page.evaluate(() => {
    const box = document.querySelector('.rpf-menu').getBoundingClientRect();
    return { left: box.left, top: box.top, right: box.right, bottom: box.bottom,
      w: window.innerWidth, h: window.innerHeight };
  });
  expect(placed.left).toBeGreaterThanOrEqual(0);
  expect(placed.top).toBeGreaterThanOrEqual(0);
  expect(placed.right).toBeLessThanOrEqual(placed.w + 1);
  expect(placed.bottom).toBeLessThanOrEqual(placed.h + 1);
});

test('leaving the agent view takes its native webview down with it', async ({ page }) => {
  // A right-pane view that holds a child webview must hide it on the way out:
  // display:none on the slot does nothing to a native layer, and it then sits
  // over Files eating the right-click.
  await page.addInitScript(() => {
    localStorage.setItem('xnaut-sidebar-visible', '1');
    localStorage.setItem('xnaut-right-pane-visible', '1');
  });
  await page.goto('/?stub=1');
  await page.waitForTimeout(900);

  const hidden = await page.evaluate(() => {
    const calls = [];
    // Stand in for the agent view: prove the right pane calls hide on switch.
    // Registered over an EXISTING key: a view only gets hide/show if the host
    // built a slot for it, and slots come from VIEW_ORDER.
    window.xnautRightPaneRegisterView('decisions', {
      mount() {}, setRoot() {}, destroy() {},
      hide() { calls.push('hide'); }, show() { calls.push('show'); },
    });
    window.xnautRightPaneShow('decisions');
    window.xnautRightPaneShow('files');
    return calls;
  });
  expect(hidden).toContain('show');
  expect(hidden).toContain('hide');
});
