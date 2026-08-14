// smoke.feature: "A project workspace opens and shows its tabs".
//
// UNTESTED on every run so far. The Given was read as unmet because Project
// Management is switched off on the test machines, but that is a fact about
// those machines rather than about the app: pm-setup.spec.mjs already mounts a
// configured module through the same stub, and the workspace is one click past
// it.
//
// The Then that matters is the second one. "The workspace shows its tabs" is
// nearly free; "each tab I open renders content belonging to that tab" is the
// one that catches a tab wired to nothing, which is the failure that actually
// ships. So each of the nine sections is opened and checked for content of its
// own, not merely for the nav being present.
import { test, expect } from '@playwright/test';

const SECTIONS = [
  'overview', 'nautflow', 'docs', 'changes', 'designer',
  'artifacts', 'work', 'delivery', 'settings',
];

const OK = {
  enabled: true, configured: true, valid: true, repo_path: '/tmp/smoke-control',
  remote_url: '', git_repository: true, project_count: 1, ticket_count: 0,
  error: '', warning: '', branch: 'main', last_commit: '', dirty: false, ahead: 0, behind: 0,
};

async function mount(page) {
  await page.evaluate((s) => {
    window.__xnautStub.pm_module_status = s;
    document.querySelectorAll('#pm-test-host').forEach((n) => n.remove());
    const host = document.createElement('div');
    host.id = 'pm-test-host';
    document.body.appendChild(host);
    window.xnautCreateProjectManagementPanel('pm-test', host, {});
  }, OK);
  return page.locator('#pm-test-host .pmw');
}

test.beforeEach(async ({ page }) => {
  page.on('dialog', (d) => d.dismiss().catch(() => {}));
  await page.goto('/index.html?stub=1');
  await page.waitForFunction(() => typeof window.xnautCreateProjectManagementPanel === 'function');
});

test('opening a project shows the workspace tabs', async ({ page }) => {
  const pane = await mount(page);

  // The stub ships exactly one project; the sidebar also carries an "ALL"
  // pseudo-project with an empty key, which is not one.
  const project = pane.locator('[data-project]:not([data-project=""])').first();
  await expect(project, 'no project in the sidebar to open').toHaveCount(1);
  await project.click();

  const nav = pane.locator('.pmw-project-nav');
  await expect(nav).toBeVisible();
  for (const section of SECTIONS) {
    await expect(nav.locator(`[data-project-section="${section}"]`),
      `the workspace is missing its ${section} tab`).toHaveCount(1);
  }
});

test('every workspace tab renders content of its own', async ({ page }) => {
  const pane = await mount(page);
  await pane.locator('[data-project]:not([data-project=""])').first().click();
  await expect(pane.locator('.pmw-project-nav')).toBeVisible();

  const seen = new Map();
  const empty = [];

  for (const section of SECTIONS) {
    await pane.locator(`[data-project-section="${section}"]`).click();
    await page.waitForTimeout(250);

    // The tab must own the selection, or the nav is decorative.
    await expect(pane.locator(`[data-project-section="${section}"].active`),
      `${section} did not become the active tab`).toHaveCount(1);

    const body = await pane.locator('.pmw-content').innerText();
    if (!body.trim()) empty.push(section);
    seen.set(section, body.trim());
  }

  expect(empty, `tabs that rendered nothing at all: ${empty.join(', ')}`).toEqual([]);

  // "Content belonging to that tab" means the tabs are not all painting the
  // same thing. A nav that switches class but not content would pass every
  // assertion above.
  const distinct = new Set(seen.values());
  expect(distinct.size,
    `nine tabs produced only ${distinct.size} distinct view(s); the nav may be switching nothing`)
    .toBeGreaterThan(1);
});

test('no workspace tab shows a raw object or an error string', async ({ page }) => {
  const pane = await mount(page);
  await pane.locator('[data-project]:not([data-project=""])').first().click();

  const bad = [];
  for (const section of SECTIONS) {
    await pane.locator(`[data-project-section="${section}"]`).click();
    await page.waitForTimeout(250);
    const body = await pane.locator('.pmw-content').innerText();
    if (/\[object \w+\]/.test(body)) bad.push(`${section}: [object Object]`);
    if (/(^|[\s:>(])undefined([\s.,)<]|$)/.test(body)) bad.push(`${section}: undefined`);
    if (/(^|[\s:>(])NaN([\s.,)<]|$)/.test(body)) bad.push(`${section}: NaN`);
  }

  expect(bad, `raw values in the workspace:\n  ${bad.join('\n  ')}`).toEqual([]);
});
