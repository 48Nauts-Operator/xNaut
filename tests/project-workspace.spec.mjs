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
// Everything above mounts the Projects panel on its own, with no project in the
// argument, which is how the sidebar's More menu still opens it: it keeps its
// project rail, its dropdown and its eight-tab nav, because with no project
// given there is nothing else here to pick one with.
//
// Below is the same panel folded into the workspace (XNAUT-342). Handed a
// project it renders none of those three, and the sections it owns are the
// workspace's tabs and its three-dot sheet instead.
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

const PROJECTS = [
  { key: 'SMOKE', name: 'Smoke Test', source_path: '/tmp/smoke', stage: 'build', flow_type: '', revision: 1 },
  // A project registered without a checkout on this machine. Its tree cannot
  // be rendered and must say so rather than look like an empty repository.
  { key: 'NOSRC', name: 'No Checkout', source_path: '', stage: '', flow_type: '', revision: 1 },
];

const WORKSPACE_STUB = {
  pm_project_list: PROJECTS,
  // The Projects panel's first load calls import_existing, not list. A fixture
  // that answers only one of them hands the folded surfaces a different set of
  // projects than the workspace itself has.
  pm_project_import_existing: PROJECTS,
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
  // The header's numbers are read from the machine, so the fixture has to be a
  // machine that answers. last_commit_ms is relative or the assertion would
  // rot: an absolute timestamp reads as "1h ago" today and "417d ago" later.
  await page.evaluate(() => {
    window.__xnautStub.project_facts = {
      is_repo: true, branch: 'main', changes: 3, worktrees: 2, last_commit_ms: Date.now() - 3600000,
    };
  });
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
  // `.xcr-ln` is the shared gutter from code-render.js, which every code
  // surface in the app now paints through (2026-09-13). It replaced this
  // panel's own `.wsp-ln`, and the point of the move is that the Code tab and
  // the Vault diff cannot drift apart again.
  await expect(page.locator('.wsp-view .xcr-ln')).toHaveCount(APP_JS.split('\n').length);
  await expect(page.locator('.wsp-view .xcr-ln').first()).toHaveText('1');

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

const MENU = [['designer', 'Designer'], ['artifacts', 'Artifacts'],
  ['settings', 'Settings'], ['details', 'Project details']];

test('the three-dot menu opens each entry as a sheet, not as a tab', async ({ page }) => {
  await openWorkspace(page);
  await page.locator('.wsp-dots').click();
  await expect(page.locator('.wsp-menu .wsp-menu-item')).toHaveText(MENU.map(([, label]) => label));

  const dead = [];
  for (const [key, label] of MENU) {
    if (await page.locator('.wsp-menu').count() === 0) await page.locator('.wsp-dots').click();
    await page.locator(`.wsp-menu .wsp-menu-item[data-wsp-menu="${key}"]`).click();

    // A configuration surface, named, over the body. Not a tab: the strip still
    // holds its six and the tab underneath is still the selected one, so
    // closing the sheet puts the reader back where they were.
    await expect(page.locator('.wsp-sheet')).toBeVisible();
    await expect(page.locator('.wsp-sheet-title')).toHaveText(label);
    await expect(page.locator('.wsp-tabs button')).toHaveCount(6);
    await expect(page.locator('.wsp-tabs button.active')).toHaveText('Code');
    await page.waitForTimeout(400);
    if (!(await page.locator('.wsp-sheet-body').innerText()).trim()) dead.push(label);
  }
  expect(dead, `menu entries that opened nothing: ${dead.join(', ')}`).toEqual([]);

  await page.locator('.wsp-sheet-close').click();
  await expect(page.locator('.wsp-sheet')).toBeHidden();
  await expect(page.locator('.wsp-code')).toBeVisible();
});

test('the workspace header carries the project live numbers', async ({ page }) => {
  await openWorkspace(page);

  // The four the Overview tab was read for. Overview is not a tab any more.
  await expect(page.locator('.wsp-tabs button[data-wsp-tab="overview"]')).toHaveCount(0);
  const head = page.locator('.wsp-head');
  await expect(head.locator('[data-fact="lastcommit"]')).toHaveText('1h ago');
  await expect(head.locator('[data-fact="changes"]')).toHaveText('3');
  await expect(head.locator('[data-fact="worktrees"]')).toHaveText('2');
  // One SMOKE ticket in the fixture. The count is of rows belonging to THIS
  // project, so a backend that ignored the filter cannot inflate it.
  await expect(head.locator('[data-fact="tickets"]')).toHaveText('1');
});

test('a project with no checkout reports the numbers it can and no others', async ({ page }) => {
  await openWorkspace(page, { project: 'NOSRC' });

  const head = page.locator('.wsp-head');
  // No source path means no git to read, so three of the four stay absent
  // rather than becoming a plausible zero.
  await expect(head.locator('[data-fact="lastcommit"]')).toHaveText('—');
  await expect(head.locator('[data-fact="changes"]')).toHaveText('—');
  await expect(head.locator('[data-fact="worktrees"]')).toHaveText('—');
  await expect(head.locator('[data-fact="tickets"]')).toHaveText('0');
});

test('no folded surface carries the panel project selector, rail or tab row', async ({ page }) => {
  await openWorkspace(page);

  // The three pieces of the Projects panel's own chrome. Removed rather than
  // hidden, so counting them is the assertion: a CSS-hidden copy still counts.
  //
  // The fourth was the Delivery tab's own project dropdown, which survived
  // XNAUT-342 because it lives in delivery-panel.js. XNAUT-435 switches it off
  // inside the workspace (`hideProjectSelect`) and puts one switcher above the
  // tabs for every tab, so a select found INSIDE the body is now a second one.
  const CHROME = ['.pmw-project-select', '.pmw-rail', '.pmw-project-nav', '.xps-select'];
  const found = [];

  // Each surface is also asked to have rendered: absence proves nothing about a
  // tab that painted nothing at all.
  for (const tab of ['work', 'nautflow', 'delivery', 'vault', 'memory']) {
    await page.locator(`.wsp-tabs button[data-wsp-tab="${tab}"]`).click();
    await page.waitForTimeout(400);
    if (!(await page.locator('.wsp-surface').innerText()).trim()) found.push(`${tab}: rendered nothing`);
    for (const selector of CHROME) {
      if (await page.locator(`.wsp-body ${selector}`).count()) found.push(`${tab}: ${selector}`);
    }
  }
  for (const [key, label] of MENU) {
    await page.locator('.wsp-dots').click();
    await page.locator(`.wsp-menu .wsp-menu-item[data-wsp-menu="${key}"]`).click();
    await page.waitForTimeout(400);
    if (!(await page.locator('.wsp-sheet-body').innerText()).trim()) found.push(`${label}: rendered nothing`);
    for (const selector of CHROME) {
      if (await page.locator(`.wsp-body ${selector}`).count()) found.push(`${label}: ${selector}`);
    }
  }

  expect(found, `a second project selector survived in:\n  ${found.join('\n  ')}`).toEqual([]);
});

// ─────────────────────────────────────────────────────────────────────────────
// XNAUT-435: the workspace's project switcher.
//
// XNAUT-342 removed the project dropdown on the grounds that the sidebar tree is
// the selector. The Sessions list can take that tree's place, and then there is
// no selector at all — which is how the workspace ended up with no front door
// and no way across. André, 2026-09-22, on the Delivery tab: "This dropdown has
// to be on the code tab too. Same dropdown."
//
// The failure this guards is quiet in the usual way: a select that renders and
// changes nothing looks exactly like one that works. So the assertions are about
// what the switch DID — the header repainted, the tab survived, the tree re-rooted
// — and not about the control existing.
const switcher = (page) => page.locator('.wsp-projbar .xps-select');

test('the project switcher lists every project with the open one selected', async ({ page }) => {
  await openWorkspace(page);

  await expect(switcher(page)).toBeVisible();
  await expect(switcher(page)).toHaveValue('SMOKE');
  // The list is the projects the app knows, named the way the sidebar names
  // them, and nothing else: a switcher that invents an "All" row would switch
  // the workspace to a project that does not exist.
  await expect(switcher(page).locator('option')).toHaveText(['Smoke Test', 'No Checkout']);
});

test('the switcher is on every tab, at the head of the left column', async ({ page }) => {
  await openWorkspace(page);

  // "At the head of the left column" is a claim about layout, so it is measured
  // against the file tree rather than asserted about the markup.
  const bar = await page.locator('.wsp-projbar .xps').boundingBox();
  const tree = await page.locator('.wsp-tree').boundingBox();
  expect(bar.x, 'the switcher is not at the workspace left edge').toBeCloseTo(tree.x, 0);
  expect(bar.width, 'the switcher is not the width of the left column').toBeCloseTo(tree.width, 0);
  expect(bar.y + bar.height, 'the switcher is not above the file tree').toBeLessThanOrEqual(tree.y + 1);

  // Every tab, because on all but Code this is the ONLY way to change project:
  // the Delivery tab's own select is gone and the sidebar tree may be covered.
  for (const tab of ['work', 'delivery', 'nautflow', 'vault', 'memory', 'code']) {
    await page.locator(`.wsp-tabs button[data-wsp-tab="${tab}"]`).click();
    await page.waitForTimeout(300);
    await expect(switcher(page), `the switcher is missing on the ${tab} tab`).toBeVisible();
  }
});

test('switching project keeps the tab, repaints the header and re-roots the tree', async ({ page }) => {
  await openWorkspace(page);

  // On Work, because "a person on Work stays on Work" is the requirement and
  // switching from Code could pass by accident: Code is the default.
  await page.locator('.wsp-tabs button[data-wsp-tab="work"]').click();
  await expect(page.locator('.wsp-tabs button[data-wsp-tab="work"]')).toHaveClass(/active/);

  await switcher(page).selectOption('NOSRC');

  await expect(page.locator('.wsp-name')).toHaveText('No Checkout');
  await expect(page.locator('.wsp-tabs button[data-wsp-tab="work"]'),
    'switching project threw the reader back to Code').toHaveClass(/active/);
  await expect(switcher(page)).toHaveValue('NOSRC');

  // The Code tab follows the switch rather than keeping the previous project's
  // checkout: NOSRC has no source path, and the tree has to say so.
  await page.locator('.wsp-tabs button[data-wsp-tab="code"]').click();
  await expect(page.locator('.wsp-tree')).toContainText('no source path');
  await expect(page.locator('.wsp-row')).toHaveCount(0);
});

test('Cmd+P reaches the switcher from anywhere in the workspace', async ({ page }) => {
  await openWorkspace(page);

  // Focus starts somewhere else entirely, which is the case the shortcut is
  // for: a reader deep in a file who wants another project.
  await page.locator('.wsp-row[data-file="/tmp/smoke/README.md"]').click();
  await page.keyboard.press('ControlOrMeta+p');

  await expect(switcher(page)).toBeFocused();
});

test('NAUT-Flow puts its stages in the left column and its detail in the centre', async ({ page }) => {
  await openWorkspace(page);

  // "Exactly where the file tree sits under Code" is a claim about the layout,
  // so it is measured against the tree rather than asserted about the markup.
  const tree = await page.locator('.wsp-tree').boundingBox();
  await page.locator('.wsp-tabs button[data-wsp-tab="nautflow"]').click();

  const rail = page.locator('.wsp-surface .pmw-nf-rail');
  await expect(rail).toBeVisible();
  const railBox = await rail.boundingBox();
  expect(railBox.x, 'the stage rail is not at the workspace left edge').toBeCloseTo(tree.x, 0);
  expect(railBox.y, 'the stage rail does not start where the tree does').toBeCloseTo(tree.y, 0);
  expect(railBox.width, 'the stage rail is not the width of the file tree').toBeCloseTo(tree.width, 0);

  // Fifteen stages, the counter and Reset at the top of the column.
  await expect(rail.locator('.pmw-vstage')).toHaveCount(15);
  await expect(rail.locator('.pmw-nf-rail-count')).toHaveText('12 / 15');
  await expect(rail.locator('.pmw-nf-reset')).toBeVisible();

  // And the stage itself fills the centre, beside the column rather than under
  // it. 'build' is the twelfth standard stage, which is the execution view.
  const centre = page.locator('.wsp-surface .pmw-nf-center');
  await expect(centre).toContainText('Build');
  const centreBox = await centre.boundingBox();
  expect(centreBox.x).toBeGreaterThanOrEqual(railBox.x + railBox.width - 1);
});

test('switching the project changes what the header and every tab reads', async ({ page }) => {
  await openWorkspace(page);
  await expect(page.locator('.wsp-head [data-fact="tickets"]')).toHaveText('1');

  await page.locator('.wsp-tabs button[data-wsp-tab="work"]').click();
  await expect(page.locator('.wsp-surface [data-id="SMOKE-1"]')).toHaveCount(1);
  await page.locator('.wsp-tabs button[data-wsp-tab="nautflow"]').click();
  await expect(page.locator('.wsp-surface .pmw-nf-rail-count')).toHaveText('12 / 15');

  // The workspace is a singleton, so asking for another project re-points the
  // one that is open. Every surface has to follow, because none of them has a
  // selector of its own to disagree with any more.
  await page.evaluate(() => window.xnautOpenWorkspace({ project: 'NOSRC' }));

  await expect(page.locator('.wsp-name')).toHaveText('No Checkout');
  await expect(page.locator('.wsp-head [data-fact="tickets"]')).toHaveText('0');
  await expect(page.locator('.wsp-surface .pmw-nf-rail-count')).toHaveText('Not started');
  await page.locator('.wsp-tabs button[data-wsp-tab="work"]').click();
  await expect(page.locator('.wsp-surface [data-id="SMOKE-1"]')).toHaveCount(0);
  // One panel, not the old one left mounted behind the new one.
  await expect(page.locator('.wsp-surface .pmw')).toHaveCount(1);
});

test('a project with no checkout says so rather than showing an empty tree', async ({ page }) => {
  await openWorkspace(page, { project: 'NOSRC' });

  await expect(page.locator('.wsp-name')).toHaveText('No Checkout');
  await expect(page.locator('.wsp-root')).toHaveText('no source path');
  await expect(page.locator('.wsp-tree')).toContainText('no source path');
  await expect(page.locator('.wsp-row')).toHaveCount(0);
});
