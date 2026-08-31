// Start build must LOOK disabled while a build is starting or running.
//
// Reported by André (XNAUT-58): "when I click Start build, make it grey and not
// clickable again until it's a new start". The button already set
// `disabled = true` for the whole start sequence — but `.pmw-btn` had no
// `:disabled` rule, so a disabled pmw button kept its accent fill, its border
// and `cursor:pointer`. Nothing changed on screen, which invited a second click
// through the ~2 min planning window.
import { test, expect } from '@playwright/test';

const OK = {
  enabled: true, configured: true, valid: true, repo_path: '/tmp/smoke-control',
  remote_url: '', git_repository: true, project_count: 1, ticket_count: 0,
  error: '', warning: '', branch: 'main', last_commit: '', dirty: false, ahead: 0, behind: 0,
};

async function openBuildStage(page) {
  await page.evaluate((s) => {
    window.__xnautStub.pm_module_status = s;
    document.querySelectorAll('#pm-test-host').forEach((n) => n.remove());
    const host = document.createElement('div');
    host.id = 'pm-test-host';
    document.body.appendChild(host);
    window.xnautCreateProjectManagementPanel('pm-test', host, {});
  }, OK);

  const pane = page.locator('#pm-test-host .pmw');
  await pane.locator('[data-project]:not([data-project=""])').first().click();
  await pane.locator('[data-project-section="nautflow"]').click();
  await pane.locator('[data-flow-stage="build"]').click();
  await expect(pane.locator('.pmw-build-start')).toBeVisible();
  return pane;
}

test.beforeEach(async ({ page }) => {
  page.on('dialog', (d) => d.dismiss().catch(() => {}));
  await page.goto('/index.html?stub=1');
  await page.waitForFunction(() => typeof window.xnautCreateProjectManagementPanel === 'function');
});

test('a disabled Start build reads as grey and unclickable', async ({ page }) => {
  const pane = await openBuildStage(page);
  const start = pane.locator('.pmw-build-start');

  const live = await start.evaluate((b) => {
    const s = getComputedStyle(b);
    return { cursor: s.cursor, background: s.backgroundColor, opacity: s.opacity };
  });
  expect(live.cursor, 'an enabled Start build should still invite a click').toBe('pointer');

  const off = await start.evaluate((b) => {
    b.disabled = true;
    const s = getComputedStyle(b);
    return { cursor: s.cursor, background: s.backgroundColor, opacity: s.opacity };
  });

  expect(off.cursor, 'a disabled Start build still shows a pointer cursor').toBe('not-allowed');
  expect(off.background, 'a disabled Start build keeps its accent fill instead of going grey')
    .not.toBe(live.background);
  expect(Number(off.opacity), 'a disabled Start build is not visually muted').toBeLessThan(1);

  // Hovering must not undo it — .pmw-btn:hover would otherwise light the border.
  await start.hover({ force: true });
  const hovered = await start.evaluate((b) => getComputedStyle(b).borderTopColor);
  const resting = await start.evaluate((b) => { b.disabled = false; return getComputedStyle(b).borderTopColor; });
  expect(hovered, 'hovering a disabled Start build lights it up like a live one').not.toBe(resting);
});
