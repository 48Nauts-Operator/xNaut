// smoke.feature: "A project workspace opens and shows its tabs".
//
// The Then that matters is the second one. "The workspace shows its tabs" is
// nearly free; "each tab I open renders content belonging to that tab" is the
// one that catches a tab wired to nothing, which is the failure that actually
// ships. So every tab and every three-dot entry is opened and checked for
// content of its own, not merely for the strip being present.
//
// These three used to mount the Projects panel standalone and drive ITS nav,
// because that panel had one. It does not (XNAUT-342): the panel is a surface
// of one project now and the tab strip belongs to the workspace, so the same
// three questions are asked of the workspace, which is the thing that answers
// them in the app.
import { test, expect } from '@playwright/test';

// ─────────────────────────────────────────────────────────────────────────────
// XNAUT-336: the project workspace, code first, surfaces as tabs.
//
// The Projects panel is mounted FOR a project and nothing else (XNAUT-342). It
// renders no project rail, no dropdown and no tab nav; the sections it owns are
// the workspace's tabs and its three-dot sheet.
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
  { name: 'lib.rs', path: '/tmp/smoke/src/lib.rs', is_directory: false },
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

async function openWorkspace(page, opts, facts) {
  await page.addInitScript(() => localStorage.setItem('xnaut-sidebar-visible', '1'));
  await page.goto('/?stub=1');
  await page.waitForSelector('#btn-help');
  await page.evaluate((stub) => { Object.assign(window.__xnautStub, stub); }, WORKSPACE_STUB);
  // The header's numbers are read from the machine, so the fixture has to be a
  // machine that answers. last_commit_ms is relative or the assertion would
  // rot: an absolute timestamp reads as "1h ago" today and "417d ago" later.
  //
  // `facts` overrides that answer, which is how the XNAUT-341 cases below put
  // a plain folder, an empty repository and a failing command on screen.
  // `null` cannot be passed through as itself, because it is also the signal
  // for "no override", so the one case needing a null answer asks by name.
  await page.evaluate((given) => {
    if (given && given.__setNull) { window.__xnautStub.project_facts = null; return; }
    window.__xnautStub.project_facts = given || {
      is_repo: true, branch: 'main', changes: 3, worktrees: 2, last_commit_ms: Date.now() - 3600000,
    };
  }, facts || null);
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

const RUST_SOURCE = [
  'pub fn greeting() -> &\'static str {',
  '    "hello"',
  '}',
].join('\n');

async function openRustFile(page) {
  await openWorkspace(page);
  await expandSrc(page);
  await page.evaluate((source) => { window.__xnautStub.read_file = source; }, RUST_SOURCE);
  await page.locator('.wsp-row[data-file="/tmp/smoke/src/lib.rs"]').click();
  await expect(page.locator('.wsp-view .xcr')).toBeVisible();
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

test('Edit lazy-loads themed Monaco for Rust with the viewer font', async ({ page }) => {
  await openRustFile(page);
  const before = await page.evaluate(() => performance.getEntriesByType('resource')
    .filter((entry) => entry.name.includes('monaco.bundle'))
    .map((entry) => entry.name));
  expect(before, 'the read-only viewer loaded Monaco').toEqual([]);

  const viewerFace = await page.locator('.wsp-view .xcr').evaluate((element) => {
    const style = getComputedStyle(element);
    return { family: style.fontFamily, size: style.fontSize, lineHeight: style.lineHeight };
  });
  const started = Date.now();
  await page.getByRole('button', { name: 'Edit', exact: true }).click();
  await expect(page.locator('.wsp-monaco[data-monaco-mode="edit"] .monaco-editor')).toBeVisible();
  const elapsed = Date.now() - started;

  const editor = await page.evaluate(() => {
    const lines = document.querySelector('.wsp-monaco[data-monaco-mode="edit"] .view-lines');
    const style = getComputedStyle(lines);
    const host = document.querySelector('.wsp-monaco[data-monaco-mode="edit"] .monaco-editor');
    const probe = document.createElement('span');
    probe.style.cssText = 'position:fixed;visibility:hidden;background:var(--editor-surface)';
    document.body.appendChild(probe);
    const expectedBackground = getComputedStyle(probe).backgroundColor;
    probe.remove();
    return {
      family: style.fontFamily,
      size: style.fontSize,
      lineHeight: style.lineHeight,
      background: getComputedStyle(host).backgroundColor,
      expectedBackground,
      language: window.XnautMonacoBundle.monaco.editor.getModels()[0].getLanguageId(),
      measured: Number(document.querySelector('.wsp-view').dataset.monacoLoadMs),
    };
  });
  expect(editor.language).toBe('rust');
  expect(editor.family).toBe(viewerFace.family);
  expect(editor.size).toBe(viewerFace.size);
  expect(editor.lineHeight).toBe(viewerFace.lineHeight);
  expect(editor.background).toBe(editor.expectedBackground);
  // The budget is the app's own measurement, click to editor ready. The
  // wall-clock round trip through Playwright also counts the driver, video
  // and trace recording: 959 ms in the GitVM sandbox on 2026-09-20 while the
  // page measured under 500, which failed the verify for a number the
  // acceptance never promised.
  expect(editor.measured, 'the Code tab measured first Edit over budget').toBeLessThan(500);
  test.info().annotations.push({ type: 'first-edit-ms', description: `page ${editor.measured}, wall clock ${elapsed}` });
});

/// André, 2026-09-22: "why is the button Open in editor doing nothing?" It
/// sent workingDir: null, the backend rejected the call, and nothing showed.
test('Open in editor asks for a login shell in the file\'s directory, sized like the terminal', async ({ page }) => {
  await openRustFile(page);
  await page.evaluate(() => { window.__xnautStub.create_command_session = { session_id: 'cs-editor-1' }; window.xnautLastTermSize = { cols: 132, rows: 40 }; });
  await page.getByRole('button', { name: 'Open in editor', exact: true }).click();
  await page.waitForFunction(() => (window.__xnautInvokes || []).some((i) => i.cmd === 'create_command_session'));
  const call = await page.evaluate(() => (window.__xnautInvokes || []).find((i) => i.cmd === 'create_command_session').args.config);
  expect(call.workingDir).toBe('/tmp/smoke/src');
  expect(call.program).toBe('zsh');
  expect(call.args[0]).toBe('-lc');
  expect(call.args[1]).toContain('/tmp/smoke/src/lib.rs');
  expect(call.cols).toBe(132);
  expect(call.rows).toBe(40);
});

test('Save shows Monaco diff before the confirmed write and marks the tree dirty', async ({ page }) => {
  await openRustFile(page);
  await page.evaluate(() => {
    window.__xnautStub.git_uncommitted_files = [{ path: 'src/lib.rs', status: 'M', additions: 1, deletions: 1 }];
  });
  await page.getByRole('button', { name: 'Edit', exact: true }).click();
  await expect(page.locator('.wsp-monaco[data-monaco-mode="edit"] .monaco-editor')).toBeVisible();
  await page.evaluate(() => {
    const model = window.XnautMonacoBundle.monaco.editor.getModels()
      .find((candidate) => candidate.uri.path.includes('/edit/'));
    model.setValue('pub fn greeting() -> &\'static str {\n    "edited"\n}\n');
  });

  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.locator('.wsp-monaco[data-monaco-mode="diff"] .monaco-diff-editor')).toBeVisible();
  let saves = await page.evaluate(() => window.__xnautInvokes.filter((call) => call.cmd === 'code_edit_save'));
  expect(saves, 'opening the diff wrote before confirmation').toEqual([]);

  await page.getByRole('button', { name: 'Write the file', exact: true }).click();
  await expect(page.locator('.wsp-reader')).toContainText('edited');
  saves = await page.evaluate(() => window.__xnautInvokes.filter((call) => call.cmd === 'code_edit_save'));
  expect(saves).toHaveLength(1);
  expect(saves[0].args).toEqual({
    path: '/tmp/smoke/src/lib.rs',
    content: 'pub fn greeting() -> &\'static str {\n    "edited"\n}\n',
  });
  await expect(page.locator('.wsp-row[data-file="/tmp/smoke/src/lib.rs"]')).toHaveClass(/dirty/);
});

test('a lease refusal stays in the diff and names its live run', async ({ page }) => {
  await openRustFile(page);
  await page.evaluate(() => {
    window.__xnautStub.code_edit_save = {
      __reject: 'Save refused: @builder holds this worktree\'s writer lease for live run 01RUN379 on SMOKE-1. Use the existing takeover path before saving.',
    };
  });
  await page.getByRole('button', { name: 'Edit', exact: true }).click();
  await expect(page.locator('.wsp-monaco[data-monaco-mode="edit"] .monaco-editor')).toBeVisible();
  await page.evaluate(() => {
    const model = window.XnautMonacoBundle.monaco.editor.getModels()
      .find((candidate) => candidate.uri.path.includes('/edit/'));
    model.setValue('fn blocked() {}\n');
  });
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.locator('.wsp-monaco[data-monaco-mode="diff"] .monaco-diff-editor')).toBeVisible();
  await page.getByRole('button', { name: 'Write the file', exact: true }).click();

  await expect(page.locator('.wsp-edit-error')).toContainText('@builder');
  await expect(page.locator('.wsp-edit-error')).toContainText('01RUN379');
  await expect(page.locator('.wsp-edit-error')).toContainText('SMOKE-1');
  await expect(page.locator('.wsp-monaco[data-monaco-mode="diff"] .monaco-diff-editor')).toBeVisible();
  await expect(page.locator('.wsp-row[data-file="/tmp/smoke/src/lib.rs"]')).not.toHaveClass(/dirty/);
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

// ── XNAUT-341: a dash says why ───────────────────────────────────────────────
//
// The four tests above prove the numbers are read. These prove the ABSENCES are
// explained, which is the half that was missing: a bare dash looked the same
// whether the folder was not a repository, the command had failed, or the
// value had simply not arrived. Two of those are fine and one is a bug, and
// the header gave a reader no way to tell them apart.
//
// Each case asserts the sentence, not merely that some text appeared. A chip
// reading the wrong reason confidently is the failure being prevented, and an
// assertion on visibility alone would pass for it.
const WHY = '.wsp-head [data-fact-why]';

test('a git project explains nothing, because nothing is missing', async ({ page }) => {
  await openWorkspace(page);

  // The control. A reason shown beside four readable numbers would be noise,
  // and would train the reader to ignore the one time it matters.
  await expect(page.locator(WHY)).toBeHidden();
  await expect(page.locator('.wsp-head [data-fact="lastcommit"]')).toHaveText('1h ago');
  await expect(page.locator('.wsp-head [data-fact="lastcommit"]')).not.toHaveAttribute('title');
});

test('a plain folder says it is not a git repository', async ({ page }) => {
  // The backend's own words for the case: a real directory, tracked by nothing.
  await openWorkspace(page, { project: 'SMOKE' }, { is_repo: false, unavailable: 'not a git repository' });

  await expect(page.locator(WHY)).toBeVisible();
  await expect(page.locator(WHY)).toHaveText('not a git repository');

  const head = page.locator('.wsp-head');
  for (const fact of ['lastcommit', 'changes', 'worktrees']) {
    await expect(head.locator(`[data-fact="${fact}"]`)).toHaveText('—');
    // The reason is on the dash as well as beside it, so hovering the empty
    // value answers the question where the reader asked it.
    await expect(head.locator(`[data-fact="${fact}"]`)).toHaveAttribute('title', 'not a git repository');
  }
  // The ticket count comes from the ticket store, not from git, and a folder
  // that is not a repository has no bearing on it.
  await expect(head.locator('[data-fact="tickets"]')).toHaveText('1');
  await expect(head.locator('[data-fact="tickets"]')).not.toHaveAttribute('title');
});

test('a project with no checkout says so instead of blaming git', async ({ page }) => {
  await openWorkspace(page, { project: 'NOSRC' });

  // Nothing failed here: the project is registered on a machine that does not
  // have its code. "not a git repository" would be a true sentence about the
  // wrong thing, so the reason names the actual situation.
  await expect(page.locator(WHY)).toHaveText('no checkout on this machine');
});

test('a repository with no commits reads never, not a dash', async ({ page }) => {
  await openWorkspace(page, { project: 'SMOKE' }, {
    is_repo: true, branch: 'main', changes: 0, worktrees: 0, last_commit_ms: null, no_commits: true,
  });

  const head = page.locator('.wsp-head');
  // "never" is an answer; a bare dash would read as a number that failed.
  await expect(head.locator('[data-fact="lastcommit"]')).toHaveText('never');
  await expect(page.locator(WHY)).toHaveText('this repository has no commits yet');
  // The other two were readable and are shown, zeros included: in a repository
  // that answered, zero is a fact rather than a missing value.
  await expect(head.locator('[data-fact="changes"]')).toHaveText('0');
  await expect(head.locator('[data-fact="worktrees"]')).toHaveText('0');
});

test('two absences at once are both reported, not just the last one', async ({ page }) => {
  // A repository with no commits whose worktree list also came back empty.
  // These are different failures and a chip that kept only the last reason
  // assigned would report one of them and silently drop the other.
  await openWorkspace(page, { project: 'SMOKE' }, {
    is_repo: true, branch: 'main', changes: 4, worktrees: null, last_commit_ms: null, no_commits: true,
  });

  const why = page.locator(WHY);
  await expect(why).toContainText('git answered only in part');
  await expect(why).toContainText('this repository has no commits yet');

  const head = page.locator('.wsp-head');
  await expect(head.locator('[data-fact="worktrees"]')).toHaveText('—');
  await expect(head.locator('[data-fact="lastcommit"]')).toHaveText('never');
  // The number that WAS read is still shown; one absence does not cost another.
  await expect(head.locator('[data-fact="changes"]')).toHaveText('4');
});

test('a failing project_facts is reported, not swallowed into dashes', async ({ page }) => {
  await openWorkspace(page, { project: 'SMOKE' }, { __reject: 'git exploded' });

  // This is the case the silent catch hid: a broken command looked exactly
  // like a folder with no git in it.
  await expect(page.locator(WHY)).toHaveText('the git facts could not be read');
  await expect(page.locator('.wsp-head [data-fact="changes"]')).toHaveText('—');
});

test('a project_facts that answers nothing is reported too', async ({ page }) => {
  // An unregistered command resolves null through the stub rather than
  // throwing, so the null path needs its own case: it used to fall through the
  // `!facts` guard and leave three dashes with no reason at all.
  await openWorkspace(page, { project: 'SMOKE' }, { __setNull: true });

  await expect(page.locator(WHY)).toHaveText('the git facts could not be read');
});

test('a failing ticket store explains the ticket count and leaves git alone', async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem('xnaut-sidebar-visible', '1'));
  await page.goto('/?stub=1');
  await page.waitForSelector('#btn-help');
  await page.evaluate((stub) => { Object.assign(window.__xnautStub, stub); }, WORKSPACE_STUB);
  await page.evaluate(() => {
    window.__xnautStub.project_facts = {
      is_repo: true, branch: 'main', changes: 3, worktrees: 2, last_commit_ms: Date.now() - 3600000,
    };
    window.__xnautStub.pm_ticket_list = { __reject: 'control repo is locked' };
  });
  await page.waitForTimeout(2500);
  await page.evaluate(() => window.xnautOpenWorkspace({ project: 'SMOKE' }));
  await expect(page.locator('.wsp')).toBeVisible();

  const head = page.locator('.wsp-head');
  // The two halves are independent: a ticket store that cannot be read must not
  // cost the three git numbers, which were read fine.
  await expect(head.locator('[data-fact="tickets"]')).toHaveText('—');
  await expect(head.locator('[data-fact="tickets"]')).toHaveAttribute('title', 'the ticket store could not be read');
  await expect(head.locator('[data-fact="changes"]')).toHaveText('3');
  await expect(head.locator('[data-fact="lastcommit"]')).toHaveText('1h ago');
});

// The three questions the standalone panel's nav used to be asked, asked of the
// workspace instead (XNAUT-342). Every tab and every three-dot entry is opened,
// and the assertion is about CONTENT: a strip that switches the active class and
// paints the same body would satisfy every structural check and be broken.
const WORKSPACE_TABS = ['code', 'work', 'delivery', 'nautflow', 'vault', 'memory'];

/** Open every tab and every menu entry; hand back key -> rendered text. */
async function readEverySurface(page) {
  const seen = new Map();
  for (const tab of WORKSPACE_TABS) {
    await page.locator(`.wsp-tabs button[data-wsp-tab="${tab}"]`).click();
    await page.waitForTimeout(400);
    await expect(page.locator(`.wsp-tabs button[data-wsp-tab="${tab}"]`),
      `${tab} did not become the active tab`).toHaveClass(/active/);
    const body = tab === 'code' ? page.locator('.wsp-code') : page.locator('.wsp-surface');
    seen.set(tab, (await body.innerText()).trim());
  }
  for (const [key, label] of MENU) {
    await page.locator('.wsp-dots').click();
    await page.locator(`.wsp-menu .wsp-menu-item[data-wsp-menu="${key}"]`).click();
    await page.waitForTimeout(400);
    seen.set(label, (await page.locator('.wsp-sheet-body').innerText()).trim());
  }
  return seen;
}

test('every workspace surface renders content of its own', async ({ page }) => {
  await openWorkspace(page);
  const seen = await readEverySurface(page);

  const empty = [...seen].filter(([, body]) => !body).map(([key]) => key);
  expect(empty, `surfaces that rendered nothing at all: ${empty.join(', ')}`).toEqual([]);

  // Not all painting the same thing. Ten surfaces that produced one view would
  // mean the strip switches nothing.
  const distinct = new Set(seen.values());
  expect(distinct.size,
    `${seen.size} surfaces produced only ${distinct.size} distinct view(s)`).toBe(seen.size);
});

test('no workspace surface shows a raw object, undefined or NaN', async ({ page }) => {
  await openWorkspace(page);
  const seen = await readEverySurface(page);

  const bad = [];
  for (const [key, body] of seen) {
    if (/\[object \w+\]/.test(body)) bad.push(`${key}: [object Object]`);
    if (/(^|[\s:>(])undefined([\s.,)<]|$)/.test(body)) bad.push(`${key}: undefined`);
    if (/(^|[\s:>(])NaN([\s.,)<]|$)/.test(body)) bad.push(`${key}: NaN`);
  }
  expect(bad, `raw values in the workspace:\n  ${bad.join('\n  ')}`).toEqual([]);
});

test('no folded surface carries the panel project selector, rail or tab row', async ({ page }) => {
  await openWorkspace(page);

  // The three pieces of the Projects panel's own chrome. Removed rather than
  // hidden, so counting them is the assertion: a CSS-hidden copy still counts.
  //
  // NOT asserted here, and it is the one that remains: the Delivery tab brings
  // its own project dropdown (delivery-panel.js:496, `.dlv-proj-select`), which
  // is that panel's file and not this ticket's.
  const CHROME = ['.pmw-project-select', '.pmw-rail', '.pmw-project-nav'];
  const found = [];

  // Each surface is also asked to have rendered: absence proves nothing about a
  // tab that painted nothing at all.
  for (const tab of ['work', 'nautflow', 'delivery', 'vault', 'memory']) {
    await page.locator(`.wsp-tabs button[data-wsp-tab="${tab}"]`).click();
    await page.waitForTimeout(400);
    if (!(await page.locator('.wsp-surface').innerText()).trim()) found.push(`${tab}: rendered nothing`);
    for (const selector of CHROME) {
      if (await page.locator(`.wsp ${selector}`).count()) found.push(`${tab}: ${selector}`);
    }
  }
  for (const [key, label] of MENU) {
    await page.locator('.wsp-dots').click();
    await page.locator(`.wsp-menu .wsp-menu-item[data-wsp-menu="${key}"]`).click();
    await page.waitForTimeout(400);
    if (!(await page.locator('.wsp-sheet-body').innerText()).trim()) found.push(`${label}: rendered nothing`);
    for (const selector of CHROME) {
      if (await page.locator(`.wsp ${selector}`).count()) found.push(`${label}: ${selector}`);
    }
  }

  expect(found, `a second project selector survived in:\n  ${found.join('\n  ')}`).toEqual([]);
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
