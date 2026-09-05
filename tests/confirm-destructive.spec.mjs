// XNAUT-80: destructive actions ask first, in-app.
//
// The bug was not a missing dialog, it was a dialog that could not exist:
// window.confirm returns without rendering in Tauri's WKWebView, so the guard
// on "Remove from list" had been deleted rather than replaced and a mis-click
// dropped the project. These tests hold the two halves: nothing is invoked
// until the confirmation is accepted, and the natives no longer answer
// silently for anyone who reaches for them again.
import { test, expect } from '@playwright/test';

const TASK = { id: 'smoke-task', name: 'Smoke Project', path: '/tmp/smoke', kind: 'project' };

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.setItem('xnaut-sidebar-visible', '1');
  });
  await page.goto('/?stub=1');
  await page.waitForTimeout(900);
  await page.evaluate((task) => {
    window.__xnautStub.tasks_list = [task];
    window.xnautSidebarRefresh();
  }, TASK);
  await expect(page.locator('.sbar-row', { hasText: 'Smoke Project' })).toBeVisible();
});

async function openRemove(page) {
  await page.locator('.sbar-row', { hasText: 'Smoke Project' }).click({ button: 'right' });
  await page.locator('.sbar-menu-item', { hasText: 'Remove from list' }).click();
  await expect(page.locator('.xdlg')).toBeVisible();
}

const removals = (page) => page.evaluate(
  () => window.__xnautInvokes.filter((i) => i.cmd === 'task_remove').length,
);

test('"Remove from list" asks first and says what survives', async ({ page }) => {
  await openRemove(page);
  await expect(page.locator('.xdlg-msg')).toHaveText('Remove “Smoke Project” from the list?');
  // The point of the dialog: the label says "Remove", the copy says what that
  // costs, which is much less than the label implies.
  await expect(page.locator('.xdlg-detail')).toContainText('The folder, its tickets and any running session stay');
  expect(await removals(page)).toBe(0);

  await page.locator('.xdlg-cancel').click();
  await expect(page.locator('.xdlg')).toHaveCount(0);
  expect(await removals(page)).toBe(0);
  await expect(page.locator('.sbar-row', { hasText: 'Smoke Project' })).toBeVisible();
});

test('Escape cancels, and confirming is what removes', async ({ page }) => {
  await openRemove(page);
  await page.keyboard.press('Escape');
  await expect(page.locator('.xdlg')).toHaveCount(0);
  expect(await removals(page)).toBe(0);

  await openRemove(page);
  await page.locator('.xdlg-ok').click();
  await expect(page.locator('.xdlg')).toHaveCount(0);
  expect(await removals(page)).toBe(1);
  expect(await page.evaluate(
    () => window.__xnautInvokes.filter((i) => i.cmd === 'task_remove').pop().args.id,
  )).toBe('smoke-task');
});

test('the natives are no longer silent', async ({ page }) => {
  // alert() keeps its signature, so the ~50 existing call sites become visible
  // without touching one of them.
  await page.evaluate(() => window.alert('saved to /tmp/report.md'));
  await expect(page.locator('.xdlg-toast')).toHaveText('saved to /tmp/report.md');

  // confirm/prompt cannot be shimmed (they have to answer synchronously), so
  // they refuse loudly instead of returning a falsy value nobody sees.
  const answers = await page.evaluate(() => [window.confirm('delete everything?'), window.prompt('name?')]);
  expect(answers).toEqual([false, null]);
  await expect(page.locator('.xdlg-toast')).toHaveCount(3);
});
