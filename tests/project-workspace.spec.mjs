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
// ships. So each of the eight sections is opened and checked for content of its
// own, not merely for the nav being present.
import { test, expect } from '@playwright/test';

const SECTIONS = [
  'overview', 'nautflow', 'docs', 'designer',
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
    `eight tabs produced only ${distinct.size} distinct view(s); the nav may be switching nothing`)
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

// ─────────────────────────────────────────────────────────────────────────────
// XNAUT-336: the project workspace, code first, surfaces as tabs.
//
// Everything above tests the Projects panel's own nine tabs, which XNAUT-342
// folds into this workspace. Below is the workspace itself: a different surface
// in the same file because the ticket names this file.
//
// The failures worth catching here are the silent ones. A tree that renders
// nothing looks the same as a project with no checkout; a file printed without
// highlighting looks the same as one printed with it until you read the markup;
// a deleted file and an empty file both paint an empty pane. So every assertion
// names the thing on screen, and the binary case also asserts that the file was
// never read.
const ROOT_ENTRIES = [
  { name: 'README.md', path: '/tmp/smoke/README.md', is_directory: false },
  { name: 'src', path: '/tmp/smoke/src', is_directory: true },
];

const SRC_ENTRIES = [
  { name: 'app.js', path: '/tmp/smoke/src/app.js', is_directory: false },
  { name: 'gone.js', path: '/tmp/smoke/src/gone.js', is_directory: false },
  { name: 'logo.png', path: '/tmp/smoke/src/logo.png', is_directory: false },
];

const APP_JS = [
  "const greeting = 'hello';",
  'function main() {',
  '  return greeting;',
  '}',
].join('\n');

const WORKSPACE_STUB = {
  pm_project_list: [
    { key: 'SMOKE', name: 'Smoke Test', source_path: '/tmp/smoke', stage: 'build', flow_type: '', revision: 1 },
    // A project registered without a checkout on this machine. Its tree cannot
    // be rendered and must say so rather than look like an empty repository.
    { key: 'NOSRC', name: 'No Checkout', source_path: '', stage: '', flow_type: '', revision: 1 },
  ],
  list_directory: ROOT_ENTRIES,
  read_file: APP_JS,
  memory_find_cmd: [],
  memory_list: [],
};

async function openWorkspace(page, opts) {
  await page.addInitScript(() => localStorage.setItem('xnaut-sidebar-visible', '1'));
  await page.goto('/?stub=1');
  await page.waitForSelector('#btn-help');
  await page.evaluate((stub) => { Object.assign(window.__xnautStub, stub); }, WORKSPACE_STUB);
  await page.waitForTimeout(2500);
  // Errors already on the page belong to whatever else is loaded, so the
  // workspace is measured by the errors it ADDS. Recorded here, read back at
  // the end of the first test.
  await page.evaluate(() => { window.__wspErrorsBefore = (window.__xnautErrors || []).length; });
  await page.evaluate((o) => window.xnautOpenWorkspace(o), opts || { project: 'SMOKE' });
  await expect(page.locator('.wsp')).toBeVisible();
}

// The tree is lazy: a directory's children are fetched when it is expanded, not
// when the root is listed. Swapping the stub's answer between the two clicks is
// what proves that. If the root load had eagerly walked the tree, app.js would
// already be on screen and this helper would be asserting nothing.
async function expandSrc(page) {
  await expect(page.locator('.wsp-row[data-file$="app.js"]')).toHaveCount(0);
  await page.evaluate((entries) => { window.__xnautStub.list_directory = entries; }, SRC_ENTRIES);
  await page.locator('.wsp-row[data-dir="/tmp/smoke/src"]').click();
  await expect(page.locator('.wsp-row[data-file="/tmp/smoke/src/app.js"]')).toBeVisible();
}

test('opening a project shows Code with its tree and no file selected', async ({ page }) => {
  await openWorkspace(page);

  await expect(page.locator('.wsp-name')).toHaveText('Smoke Test');
  await expect(page.locator('.wsp-tabs button[data-wsp-tab="code"]')).toHaveClass(/active/);
  for (const tab of ['code', 'work', 'delivery', 'nautflow', 'vault', 'memory']) {
    await expect(page.locator(`.wsp-tabs button[data-wsp-tab="${tab}"]`)).toHaveCount(1);
  }

  // The tree is the project's checkout, root listing only.
  await expect(page.locator('.wsp-row')).toHaveCount(2);
  await expect(page.locator('.wsp-row[data-file="/tmp/smoke/README.md"]')).toBeVisible();
  await expect(page.locator('.wsp-row[data-dir="/tmp/smoke/src"]')).toBeVisible();

  // Nothing is open, and the pane says so rather than sitting blank.
  await expect(page.locator('.wsp-view')).toContainText('No file open');
  await expect(page.locator('.wsp-ftabs')).toBeHidden();

  await expandSrc(page);
  const added = await page.evaluate(() => (window.__xnautErrors || []).slice(window.__wspErrorsBefore || 0));
  expect(added, `opening the workspace raised: ${added.join(', ')}`).toEqual([]);
});

test('clicking a file renders it highlighted with line numbers', async ({ page }) => {
  await openWorkspace(page);
  await expandSrc(page);
  await page.locator('.wsp-row[data-file="/tmp/smoke/src/app.js"]').click();

  // Highlighted: hljs spans, not escaped text in a bare <pre>.
  await expect(page.locator('.wsp-view pre.hljs code')).toBeVisible();
  await expect(page.locator('.wsp-view .hljs-keyword').first()).toBeVisible();
  await expect(page.locator('.wsp-view')).toContainText('const greeting');

  // Numbered: one per line of the file, starting at 1.
  await expect(page.locator('.wsp-view .wsp-ln')).toHaveCount(APP_JS.split('\n').length);
  await expect(page.locator('.wsp-view .wsp-ln').first()).toHaveText('1');

  // Open files are tabs within Code.
  await expect(page.locator('.wsp-ftab.active')).toHaveText(/app\.js/);
});

test('the surface tabs switch without losing the selected file', async ({ page }) => {
  await openWorkspace(page);
  await expandSrc(page);
  await page.locator('.wsp-row[data-file="/tmp/smoke/src/app.js"]').click();
  await expect(page.locator('.wsp-view')).toContainText('const greeting');

  // Work and NAUT-Flow are the Projects panel mounted here; Delivery, Vault and
  // Memory are their own components. Each must paint something of its own.
  const empty = [];
  for (const tab of ['work', 'delivery', 'nautflow', 'vault', 'memory']) {
    await page.locator(`.wsp-tabs button[data-wsp-tab="${tab}"]`).click();
    await expect(page.locator(`.wsp-tabs button[data-wsp-tab="${tab}"]`)).toHaveClass(/active/);
    await expect(page.locator('.wsp-code')).toBeHidden();
    await page.waitForTimeout(400);
    const body = (await page.locator('.wsp-surface').innerText()).trim();
    if (!body) empty.push(tab);
  }
  expect(empty, `surface tabs that rendered nothing: ${empty.join(', ')}`).toEqual([]);
  await expect(page.locator('.dlv-pane, .mem, .pmw')).not.toHaveCount(0);

  // Back to Code: the same file, still open, still rendered.
  await page.locator('.wsp-tabs button[data-wsp-tab="code"]').click();
  await expect(page.locator('.wsp-code')).toBeVisible();
  await expect(page.locator('.wsp-ftab.active')).toHaveText(/app\.js/);
  await expect(page.locator('.wsp-view')).toContainText('const greeting');
});

test('a file that is gone from disk says so', async ({ page }) => {
  await openWorkspace(page);
  await expandSrc(page);
  await page.evaluate(() => {
    window.__xnautStub.read_file = { __reject: 'No such file or directory (os error 2)' };
  });
  await page.locator('.wsp-row[data-file="/tmp/smoke/src/gone.js"]').click();

  await expect(page.locator('.wsp-view .wsp-msg')).toContainText('gone from disk');
  await expect(page.locator('.wsp-view .wsp-msg')).toContainText('/tmp/smoke/src/gone.js');
  await expect(page.locator('.wsp-view pre')).toHaveCount(0);
});

test('a binary file says so instead of printing bytes', async ({ page }) => {
  await openWorkspace(page);
  await expandSrc(page);
  await page.locator('.wsp-row[data-file="/tmp/smoke/src/logo.png"]').click();

  await expect(page.locator('.wsp-view .wsp-msg')).toContainText('binary file');
  await expect(page.locator('.wsp-view pre')).toHaveCount(0);
  // And it was never read: read_file returns a String, so asking for a PNG
  // would hand the viewer mangled bytes to print.
  const reads = await page.evaluate(() => (window.__xnautInvokes || [])
    .filter((i) => i.cmd === 'read_file' && String(i.args && i.args.path).endsWith('logo.png')));
  expect(reads).toEqual([]);
});

test('the three-dot menu lists its four entries and each opens one', async ({ page }) => {
  await openWorkspace(page);
  await page.locator('.wsp-dots').click();
  await expect(page.locator('.wsp-menu .wsp-menu-item')).toHaveText([
    'Designer', 'Artifacts', 'Settings', 'Project details',
  ]);

  const dead = [];
  for (const [key, label] of [['designer', 'Designer'], ['artifacts', 'Artifacts'],
    ['settings', 'Settings'], ['details', 'Project details']]) {
    if (await page.locator('.wsp-menu').count() === 0) await page.locator('.wsp-dots').click();
    await page.locator(`.wsp-menu .wsp-menu-item[data-wsp-menu="${key}"]`).click();
    // The menu views are not tabs, so the strip grows a chip for the one open:
    // no menu entry leaves the reader unable to tell what they are looking at.
    await expect(page.locator('.wsp-tabs .wsp-chip')).toHaveText(label);
    await page.waitForTimeout(400);
    if (!(await page.locator('.wsp-surface').innerText()).trim()) dead.push(label);
  }
  expect(dead, `menu entries that opened nothing: ${dead.join(', ')}`).toEqual([]);
});

test('a project with no checkout says so rather than showing an empty tree', async ({ page }) => {
  await openWorkspace(page, { project: 'NOSRC' });

  await expect(page.locator('.wsp-name')).toHaveText('No Checkout');
  await expect(page.locator('.wsp-root')).toHaveText('no source path');
  await expect(page.locator('.wsp-tree')).toContainText('no source path');
  await expect(page.locator('.wsp-row')).toHaveCount(0);
});
