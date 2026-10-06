import { test, expect } from '@playwright/test';

// Hold only the real delayed landing callback. All app initialization, pane
// factories and IPC stubs still run normally, so the race is deterministic.
test.beforeEach(async ({ page }) => {
  page.on('dialog', d => d.dismiss().catch(() => {}));
  await page.addInitScript(() => {
    const schedule = window.setTimeout;
    window.setTimeout = (fn, delay, ...args) => {
      if (delay === 400 && String(fn).includes('xnautOpenMesh')) {
        window.finishLanding = () => fn(...args);
        return 0;
      }
      return schedule(fn, delay, ...args);
    };
  });
  await page.goto('/?stub=1');
  await page.waitForFunction(() => typeof window.finishLanding === 'function'
    && eval('tabs').some(t => t.terminals.some(pane => pane.sessionId)));
});

test('untouched startup still opens Mesh', async ({ page }) => {
  await page.evaluate(() => window.finishLanding());
  await expect(page.locator('.mesh')).toBeVisible();
  expect(await page.evaluate(() => eval('tabs').find(t => t.id === eval('activeTabId')).panelFactory)).toBe('xnautCreateMeshPanel');
});

for (const kind of ['Browser', 'Markdown', 'Terminal']) {
  test(`delayed landing preserves a user-created ${kind} tab`, async ({ page }) => {
    const selected = await page.evaluate(async kind => {
      if (kind === 'Browser') await window.xnautAttachBrowserTab('https://example.com');
      if (kind === 'Markdown') await window.xnautAttachMarkdownTab({});
      if (kind === 'Terminal') window.createNewTab();
      return eval('activeTabId');
    }, kind);
    await page.waitForFunction(id => eval('tabs').find(t => t.id === id)?.terminals.length, selected);
    await page.evaluate(() => window.finishLanding());
    expect(await page.evaluate(() => eval('activeTabId'))).toBe(selected);
    expect(await page.evaluate(() => eval('tabs').some(t => t.panelFactory === 'xnautCreateMeshPanel'))).toBe(false);
  });
}
