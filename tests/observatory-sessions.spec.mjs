// XNAUT-340: sessions live in the Observatory, and the agent rows are grouped
// by project.
//
// Two things are being guarded here, and the second is the one Andre asked for.
//
// 1. The session block (Connect, Kill, the provider pair, "Open another
//    session") moved off the Projects Overview tab. Killing an agent from its
//    new home must not be one click easier than it was from the old one, so the
//    arming confirmation is asserted, not assumed.
//
// 2. Every running row belongs to a project, and there are exactly three honest
//    ways to say which: the run registry (`run_registry_list`, joining a run's
//    `project` on its `zellij_session`), the working directory (inside a
//    project's source_path), and the session name (`cx-<project>` with the
//    agent prefix stripped). Each source gets its own row below, and each row
//    would be attributed WRONG if the order were changed: `cx-mislabelled`
//    carries a registry row saying OTHER, and the build row's name says
//    nothing about its cwd.
//
// A row that matches none of the three lands in "No project", last and visible.
import { test, expect } from '@playwright/test';

const PROJECTS = [
  { key: 'XNAUT', name: 'xNaut', source_path: '/Users/x/dev/xnaut', stage: 'build', status: 'active', tickets: [] },
  { key: 'CMGR', name: 'CompanyManager', source_path: '/Users/x/dev/company-manager', stage: 'build', status: 'active', tickets: [] },
  { key: 'OTHER', name: 'Other Thing', source_path: '/Users/x/dev/other', stage: 'build', status: 'active', tickets: [] },
];

// One dispatched run, as `run_registry_list` returns it (XNAUT-345). Its
// session name says "mislabelled", which matches no project at all, so the
// only thing that can attribute it is the registry row.
const RUN_ROW = {
  run_id: 'run-1', kind: 'agent', ticket: null, project: 'OTHER',
  agent_handle: 'claude', runtime_id: 'claude', zellij_session: 'cx-mislabelled',
  worktree_path: '/tmp/nowhere', branch: 'dev', state: 'running',
  started_at: 1000, last_seen_at: 2000,
};

/** Opens the Observatory with a machine described by `world`.
 *
 * The registry answers one command now, so the shared stub can hold it: the
 * panel no longer walks the manifest files, and nothing here has to give two
 * paths two answers (XNAUT-345).
 */
async function openObservatory(page, world) {
  await page.addInitScript(() => localStorage.setItem('xnaut-sidebar-visible', '1'));
  await page.goto('/?stub=1');
  await page.waitForSelector('#btn-help');
  await page.evaluate((w) => {
    Object.assign(window.__xnautStub, {
      pm_project_list: w.projects,
      agent_sessions_list: w.agentSessions || [],
      zellij_sessions_info: w.zellij || [],
      zellij_live_sessions: w.live || [],
      loom_runs_list: w.runs || [],
      run_registry_list: w.registryRows || [],
      max_usage: null,
      codex_usage: null,
    });
  }, world);
  await page.evaluate(() => window.xnautAttachObservatoryTab());
  await expect(page.locator('.obs')).toBeVisible();
}

/** The group headers, in the order they are painted: "<name> <n> sessions". */
async function groups(page) {
  return page.locator('.obs-grp').allInnerTexts()
    .then((rows) => rows.map((t) => t.replace(/[\u25b8\u25be]/g, '').replace(/\s+/g, ' ').trim()));
}

const WORLD = {
  projects: PROJECTS,
  registryRows: [RUN_ROW],
  // Source 3: a session started by hand, named after its project.
  // Source 1: the same list carries the dispatched run's session, whose NAME
  // matches nothing. Only the registry row can place it.
  // And one session nothing can attribute.
  zellij: [
    { name: 'cx-CompanyManager', exited: false, created: '2h ago', created_ms: Date.now() - 7200000 },
    { name: 'cl-company-manager', exited: false, created: '1h ago', created_ms: Date.now() - 3600000 },
    { name: 'cx-mislabelled', exited: false, created: '3h ago', created_ms: Date.now() - 10800000 },
    { name: 'scratch-shell', exited: false, created: '10m ago', created_ms: Date.now() - 600000 },
  ],
  // Source 2: a build row carries a cwd, and its derived session name
  // ("cl-safety-net") says nothing about which project the worktree is in.
  live: ['cl-safety-net'],
  runs: [{
    id: 'build-1', provider: 'build', status: 'started', weave: 'build the thing',
    cwd: '/Users/x/dev/xnaut/.worktrees/safety-net', model: 'claude', started_ms: Date.now() - 1800000,
  }],
  agentSessions: [],
};

test('rows group under their project, with a count on each header', async ({ page }) => {
  await openObservatory(page, WORLD);
  await page.getByRole('tab', { name: 'Agents', exact: true }).click();
  await expect(page.locator('.obs-grp').first()).toBeVisible();

  // CompanyManager has two sessions, xNaut one, Other Thing one, and one row
  // nothing can place. The count is the point of the header: a flat list of
  // five rows never said "this project has two open".
  const heads = await groups(page);
  expect(heads).toContain('CompanyManager 2 sessions');
  expect(heads).toContain('xNaut 1 session');
  expect(heads).toContain('Other Thing 1 session');
  // Every row is still on screen, under exactly one header.
  await expect(page.locator('.obs-row')).toHaveCount(5);
});

test('a dispatched run groups by its registry row, not by its name', async ({ page }) => {
  await openObservatory(page, WORLD);
  await page.getByRole('tab', { name: 'Agents', exact: true }).click();
  await expect(page.locator('.obs-grp').first()).toBeVisible();

  // The registry was actually asked, by its own command and not by reading its
  // files: without that join there is nothing else that could put this row
  // under OTHER.
  const asked = await page.evaluate(() => window.__xnautInvokes
    .filter((i) => i.cmd === 'run_registry_list').length);
  expect(asked).toBeGreaterThan(0);
  const walked = await page.evaluate(() => window.__xnautInvokes
    .filter((i) => (i.cmd === 'read_file' || i.cmd === 'list_directory')
      && String((i.args && i.args.path) || '').includes('xnaut/registry')).length);
  expect(walked).toBe(0);

  // `cx-mislabelled` matches no project key, no project name and no directory.
  // Its registry row says OTHER, and the registry is the first source.
  const rows = await page.evaluate(() => {
    const out = [];
    let head = '';
    for (const el of document.querySelectorAll('.obs-grp, .obs-row')) {
      if (el.classList.contains('obs-grp')) head = el.querySelector('b').textContent;
      else out.push([head, el.querySelector('.c-name .t').textContent]);
    }
    return out;
  });
  expect(rows).toContainEqual(['Other Thing', 'cx-mislabelled']);
});

test('a build row groups by its working directory', async ({ page }) => {
  await openObservatory(page, WORLD);
  await page.getByRole('tab', { name: 'Agents', exact: true }).click();
  await expect(page.locator('.obs-grp').first()).toBeVisible();

  // The row's zellij name is derived from the worktree's last path segment
  // ("cl-safety-net"), which matches no project. Only the cwd, sitting under
  // /Users/x/dev/xnaut, can say this is xNaut work.
  const xnaut = page.locator('.obs-grp', { hasText: 'xNaut' });
  await expect(xnaut).toHaveText(/1 session/);
  const after = page.locator('.obs-grp:has-text("xNaut") + .obs-row');
  await expect(after).toContainText('build the thing');
});

test('a hand-started cx-<project> session groups by the name match', async ({ page }) => {
  await openObservatory(page, WORLD);
  await page.getByRole('tab', { name: 'Agents', exact: true }).click();
  await expect(page.locator('.obs-grp').first()).toBeVisible();

  // No registry row, no cwd: the session name is all there is. Both spellings of
  // the same project land in one group, because the match collapses separators
  // and ignores case.
  const rows = await page.evaluate(() => {
    const out = [];
    let head = '';
    for (const el of document.querySelectorAll('.obs-grp, .obs-row')) {
      if (el.classList.contains('obs-grp')) head = el.querySelector('b').textContent;
      else out.push([head, el.querySelector('.c-name .t').textContent]);
    }
    return out;
  });
  expect(rows).toContainEqual(['CompanyManager', 'cx-CompanyManager']);
  expect(rows).toContainEqual(['CompanyManager', 'cl-company-manager']);
});

test('a row matching none of the three lands in No project, sorted last', async ({ page }) => {
  await openObservatory(page, WORLD);
  await page.getByRole('tab', { name: 'Agents', exact: true }).click();
  await expect(page.locator('.obs-grp').first()).toBeVisible();

  const heads = await groups(page);
  expect(heads[heads.length - 1]).toBe('No project 1 session');
  // Visible, not hidden: an unattributable session is the one worth seeing.
  const after = page.locator('.obs-grp:has-text("No project") + .obs-row');
  await expect(after).toContainText('scratch-shell');
});

test('a group collapses and its rows go with it', async ({ page }) => {
  await openObservatory(page, WORLD);
  await page.getByRole('tab', { name: 'Agents', exact: true }).click();
  await expect(page.locator('.obs-row')).toHaveCount(5);

  const head = page.locator('.obs-grp', { hasText: 'CompanyManager' });
  await expect(head).toHaveAttribute('aria-expanded', 'true'); // open by default
  await head.click();

  await expect(head).toHaveAttribute('aria-expanded', 'false');
  await expect(page.locator('.obs-row')).toHaveCount(3);
  await expect(page.locator('[data-rows]')).not.toContainText('cx-CompanyManager');
  // The header itself stays, with its count, or folding would hide the finding.
  await expect(head).toHaveText(/2 sessions/);
});

test('the session block renders with Connect and Kill, and Kill still asks first', async ({ page }) => {
  await openObservatory(page, WORLD);
  const band = page.locator('[data-sessions]');
  // Scoped to one project, so the band starts on xNaut, which has no zellij
  // session of its own, and says so rather than listing the whole machine.
  await expect(band).toContainText('No session for this project yet');
  await band.locator('[data-sess-project]').selectOption('CMGR');
  await expect(band.locator('[data-sess-attach]')).toHaveCount(2);
  await expect(band.locator('[data-sess-attach]').first()).toHaveText('Connect');
  await expect(band).toContainText('2 running · 0 resumable');
  // The provider pair and the opener came across with the list.
  await expect(band.locator('[data-sess-provider]')).toBeVisible();
  await expect(band.locator('[data-sess-model]')).toBeVisible();
  await expect(band.locator('[data-sess-open]')).toHaveText('Open another session');

  // Kill arms itself first. confirm() is a no-op in Tauri's WKWebView, so this
  // IS the confirmation, and one click must not delete a running agent.
  const kill = band.locator('[data-sess-kill]').first();
  await kill.click();
  await expect(kill).toHaveText('Kill?');
  const deletes = () => page.evaluate(() => window.__xnautInvokes
    .filter((i) => i.cmd === 'zellij_delete_session').length);
  expect(await deletes()).toBe(0);

  await kill.click();
  await expect.poll(deletes).toBe(1);
  expect(await page.evaluate(() => window.__xnautErrors || [])).toEqual([]);
});

test('Connect attaches the session rather than opening a bare shell', async ({ page }) => {
  await openObservatory(page, WORLD);
  const band = page.locator('[data-sessions]');
  await band.locator('[data-sess-project]').selectOption('CMGR');
  await band.locator('[data-sess-attach]').first().click();

  // xnautOpenZellijSession is the app's one attach path; it ends in a command
  // session running `zellij attach`. Anything else would open an empty shell
  // that looks exactly like a working connection.
  await expect.poll(() => page.evaluate(() => window.__xnautInvokes
    .filter((i) => i.cmd === 'create_command_session')
    .map((i) => String(i.args.config.args[1]))
    .filter((c) => c.includes('zellij attach')).length)).toBeGreaterThan(0);
});
