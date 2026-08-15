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

  // Now zoomed: the menu must still land inside the window, not off it.
  await page.keyboard.press('Escape');
  await page.keyboard.press('Meta+=');
  await page.keyboard.press('Meta+=');
  await openMenu();
  await expect(menu).toBeVisible();
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
