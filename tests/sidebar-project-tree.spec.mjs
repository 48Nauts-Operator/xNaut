// XNAUT-335: the sidebar is an icon rail and a project tree.
//
// Every way this can be wrong is quiet. A rail icon with no accessible name is
// a button nobody can address and nothing reports. A dot that is always the
// same colour still renders, and reads as "everything is clean" forever. A pin
// that is written but never read back looks pinned until the next render. A cap
// that hides the wrong worktrees hides the one with the agent on it.
//
// So the assertions name values: which five icons, which state word on which
// worktree, which pair got recorded. The three dot states are driven from three
// different stubbed sources on purpose, because they come from three different
// commands and a test that only exercised one would pass with the other two
// wired to nothing.
import { test, expect } from '@playwright/test';

const REPO = '/repo/xnaut';
const WT = (path, branch, changes) => ({ path, branch, head: 'abc12345', changes, current: false });

// Eight, which is more than the cap. Two are running (from two different
// registries), one is dirty, five are clean.
const WORKTREES = [
  WT(REPO, 'dev', 0),
  WT(`${REPO}/.worktrees/safety-net`, 'safety-net', 3),
  WT(`${REPO}/.worktrees/feat-330`, 'feat/xnaut-330', 0),
  WT(`${REPO}/.worktrees/rig`, 'rig', 0),
  WT(`${REPO}/.worktrees/old-1`, 'old/1', 0),
  WT(`${REPO}/.worktrees/old-2`, 'old/2', 0),
  WT(`${REPO}/.worktrees/old-3`, 'old/3', 0),
  WT(`${REPO}/.worktrees/old-4`, 'old/4', 0),
];

const STUB = {
  pm_project_list: [
    { key: 'XNAUT', name: 'xnaut', source_path: REPO, stage: '', flow_type: '', revision: 1 },
    { key: 'ANTBOT', name: 'AntBot', source_path: '/repo/antbot', stage: '', flow_type: '', revision: 1 },
  ],
  tasks_list: [],
  git_worktree_list: WORKTREES,
  // Source one for "an agent is running on it": a loom run names the directory
  // it executes in.
  loom_runs_list: [
    { id: 'run-1', weave: 'build', goal: '', provider: 'local', started_ms: 1, status: 'started', pid: 4242, log: '', model: 'claude', cwd: REPO },
    // Finished runs must not light a dot.
    { id: 'run-2', weave: 'build', goal: '', provider: 'local', started_ms: 2, status: 'done', pid: 0, log: '', model: 'claude', cwd: `${REPO}/.worktrees/feat-330` },
  ],
  // Source two: an agent session carries no path, only the zellij session that
  // hosts it, whose name the backend derives from the worktree directory.
  agent_sessions_list: [
    { session_id: 's1', agent_id: 'claude', label: 'rig', pane_key: 's1', status: 'working',
      started_at_ms: 1, last_output_at_ms: 1, status_changed_at_ms: 1, zellij_session: 'cl-rig' },
  ],
};

const GROUP = '.sbar-group[data-group="pm:XNAUT"]';
const rowFor = (page, path) => page.locator(`${GROUP} .sbar-wt[data-wt="${path}"]`);

async function openSidebar(page) {
  await page.addInitScript(() => {
    localStorage.clear();
    localStorage.setItem('xnaut-sidebar-visible', '1');
  });
  await page.goto('/?stub=1');
  await page.waitForSelector('#btn-help');
  await page.evaluate((stub) => { Object.assign(window.__xnautStub, stub); }, STUB);
  await page.waitForTimeout(1200);
  await page.evaluate(() => window.xnautSidebarRefresh());
  await expect(page.locator(GROUP)).toBeVisible();
}

async function expand(page) {
  await page.locator(`${GROUP} [data-twist]`).click();
  await expect(page.locator(`${GROUP} .sbar-wt`).first()).toBeVisible();
}

const errors = (page) => page.evaluate(() => window.__xnautErrors || []);

test('the rail is five named icons with no labels, and drops no destination', async ({ page }) => {
  await openSidebar(page);

  const icons = page.locator('.sbar-rail-btn:not(.sbar-rail-more)');
  await expect(icons, 'the rail is five icons').toHaveCount(5);

  for (const name of ['Search', 'Mesh', 'Automations', 'Observatory', 'Inbox']) {
    const btn = page.getByRole('button', { name, exact: true });
    await expect(btn, `${name} has no accessible name`).toHaveCount(1);
    // Icons only: the name lives in aria-label and the tooltip, never in text.
    expect((await btn.innerText()).trim(), `${name} renders a visible label`).toBe('');
    expect(await btn.locator('svg').count(), `${name} has no inline SVG`).toBeGreaterThan(0);
  }

  // The rows the rail replaced are destinations, not rows: each one is still
  // one click away and still calls what it always called.
  await page.getByRole('button', { name: 'More surfaces', exact: true }).click();
  const items = await page.locator('.sbar-menu-item').allTextContents();
  expect(items).toEqual(['Agent Space', 'Skills', 'Plugins', 'Tasks', 'Projects', 'Delivery', 'Memory', 'Vault']);

  expect(await errors(page)).toEqual([]);
});

// Listing the eight is not the same as reaching them. Two of them are opened
// here for real, because these two were opened by clicking a nav row and the
// nav row is what this change removed: if either has become unreachable, that
// is a bug in this change and not a nav preference.
test('a surface the rail does not carry still opens from the More menu', async ({ page }) => {
  await openSidebar(page);

  await page.getByRole('button', { name: 'More surfaces', exact: true }).click();
  await page.locator('.sbar-menu-item', { hasText: 'Vault' }).click();
  await expect(page.locator('.sbar-submenu')).toBeVisible();
  await page.locator('.sbar-submenu-back').click();

  await page.getByRole('button', { name: 'More surfaces', exact: true }).click();
  await page.locator('.sbar-menu-item', { hasText: 'Memory' }).click();
  await expect(page.locator('.mem')).toBeVisible();

  expect(await errors(page)).toEqual([]);
});

test('a project expands to its worktrees, and the dot says running, dirty or clean', async ({ page }) => {
  await openSidebar(page);
  // Collapsed until asked: forty-four projects must not mean forty-four git
  // calls to paint a tree nobody opened.
  await expect(page.locator(`${GROUP} .sbar-wt`)).toHaveCount(0);
  await expand(page);

  // Running, from the loom run whose cwd is this worktree.
  await expect(rowFor(page, REPO)).toHaveAttribute('data-wt-state', 'running');
  await expect(rowFor(page, REPO).locator('.sbar-word')).toHaveText('1 agent');
  await expect(rowFor(page, REPO).locator('.sbar-dot')).toHaveClass(/sbar-run/);

  // Running, from the agent session matched through its zellij session name.
  await expect(rowFor(page, `${REPO}/.worktrees/rig`)).toHaveAttribute('data-wt-state', 'running');

  // Dirty, from the uncommitted count git_worktree_list already carries.
  const dirty = rowFor(page, `${REPO}/.worktrees/safety-net`);
  await expect(dirty).toHaveAttribute('data-wt-state', 'dirty');
  await expect(dirty.locator('.sbar-word')).toHaveText('dirty');
  await expect(dirty.locator('.sbar-name')).toHaveText('safety-net');

  // Clean, and a FINISHED run on it does not make it running.
  const clean = rowFor(page, `${REPO}/.worktrees/feat-330`);
  await expect(clean).toHaveAttribute('data-wt-state', 'clean');
  await expect(clean.locator('.sbar-word')).toHaveText('clean');

  expect(await errors(page)).toEqual([]);
});

test('the star pins a worktree and the pin survives a re-render', async ({ page }) => {
  await openSidebar(page);
  await expand(page);

  const pinned = page.locator('.sbar-pinned');
  await expect(pinned).toHaveCount(0);

  await rowFor(page, `${REPO}/.worktrees/safety-net`).locator('[data-star]').click();
  // The Pinned group names the PAIR: the same branch name exists in two repos.
  await expect(pinned.locator('.sbar-wt .sbar-name')).toHaveText('xnaut / safety-net');
  await expect(rowFor(page, `${REPO}/.worktrees/safety-net`).locator('[data-star]'))
    .toHaveAttribute('aria-pressed', 'true');

  // A full re-render reads the pin back rather than remembering it in a closure.
  await page.evaluate(() => window.xnautSidebarRefresh());
  await expect(page.locator('.sbar-pinned .sbar-wt .sbar-name')).toHaveText('xnaut / safety-net');
  await expect(rowFor(page, `${REPO}/.worktrees/safety-net`).locator('[data-star]'))
    .toHaveAttribute('aria-pressed', 'true');

  expect(await errors(page)).toEqual([]);
});

test('worktrees past the cap collapse behind one row, and the control reveals them', async ({ page }) => {
  await openSidebar(page);
  await expand(page);

  // Five shown of eight, and the two with an agent on them are not the ones
  // hidden: the cap orders by state before it cuts.
  await expect(page.locator(`${GROUP} .sbar-wt`)).toHaveCount(5);
  await expect(rowFor(page, REPO)).toBeVisible();
  await expect(rowFor(page, `${REPO}/.worktrees/rig`)).toBeVisible();
  await expect(rowFor(page, `${REPO}/.worktrees/old-4`)).toHaveCount(0);

  const more = page.locator(`${GROUP} .sbar-wt-more`);
  await expect(more).toHaveText(/Hiding 3 worktrees/);
  await more.locator('[data-wt-more-btn]').click();

  await expect(page.locator(`${GROUP} .sbar-wt`)).toHaveCount(8);
  await expect(rowFor(page, `${REPO}/.worktrees/old-4`)).toBeVisible();
  await expect(more).toHaveText(/Show fewer worktrees/);

  expect(await errors(page)).toEqual([]);
});

test('selecting a worktree records the project and the tree', async ({ page }) => {
  await openSidebar(page);
  await expand(page);

  await rowFor(page, `${REPO}/.worktrees/safety-net`).locator('.sbar-name').click();

  // The pair, not the project alone. Read as a value and through the getter,
  // because both are exported and a caller may reach for either.
  const scope = await page.evaluate(() => window.xnautActiveScope);
  expect(scope).toMatchObject({
    project: 'XNAUT',
    projectName: 'xnaut',
    repo: REPO,
    worktree: `${REPO}/.worktrees/safety-net`,
    branch: 'safety-net',
  });
  expect(await page.evaluate(() => window.xnautGetActiveScope().worktree))
    .toBe(`${REPO}/.worktrees/safety-net`);

  // The project is selected too: a chosen branch under no project is a tree
  // that has lost half of what it just recorded.
  await expect(rowFor(page, `${REPO}/.worktrees/safety-net`)).toHaveClass(/sbar-row-active/);
  await expect(page.locator(`${GROUP} > .sbar-row[data-entry-id="pm:XNAUT"]`)).toHaveClass(/sbar-row-active/);

  // And it outlives a reload, because a reload is not a change of mind.
  const saved = await page.evaluate(() => JSON.parse(localStorage.getItem('xnaut-active-scope')));
  expect(saved.worktree).toBe(`${REPO}/.worktrees/safety-net`);

  expect(await errors(page)).toEqual([]);
});

// Andre, 2026-09-13: "pin projects so it shows all pinned projects incl. subs
// above projects. then on the projects level I like a hide all (except
// pinned)". A pinned project used to be a bare row copied to the top with its
// worktrees left behind in the list; now the whole group moves up, and the
// Projects gear can hide everything that is not pinned.
test('a pinned project moves to the top with its worktrees, and only-pinned hides the rest', async ({ page }) => {
  await openSidebar(page);
  const pinned = page.locator('.sbar-pinned');
  await expect(pinned).toHaveCount(0);

  await page.locator(`${GROUP} > .sbar-row [data-proj-menu]`).click();
  await page.locator('.sbar-menu-item', { hasText: 'Pin to the top' }).click();

  // Moved, not copied: one group with that key, and it sits inside Pinned.
  await expect(page.locator(GROUP)).toHaveCount(1);
  await expect(pinned.locator(GROUP)).toHaveCount(1);
  // Its worktrees came along.
  await expand(page);
  await expect(pinned.locator(`${GROUP} .sbar-wt`).first()).toBeVisible();
  // The unpinned project is still in the list below.
  await expect(page.locator('.sbar-group[data-group="pm:ANTBOT"]')).toHaveCount(1);

  // Hide all except pinned, from the Projects gear.
  await page.getByRole('button', { name: 'Project list options', exact: true }).click();
  await page.locator('.sbar-menu-item', { hasText: 'Show only pinned' }).click();
  await expect(page.locator('.sbar-group[data-group="pm:ANTBOT"]')).toHaveCount(0);
  await expect(pinned.locator(GROUP)).toHaveCount(1);
  const back = page.locator('.sbar-hidden-toggle[data-only-pinned]');
  await expect(back).toHaveText('1 more — show all');

  // Written, and read back on a full re-render rather than remembered in a
  // closure. (openSidebar clears localStorage on every navigation, so a
  // reload here would test the harness, not the app.)
  expect(await page.evaluate(() => localStorage.getItem('xnaut-projects-only-pinned'))).toBe('1');
  await page.evaluate(() => window.xnautSidebarRefresh());
  await expect(page.locator('.sbar-group[data-group="pm:ANTBOT"]')).toHaveCount(0);
  await expect(pinned.locator(GROUP)).toHaveCount(1);
  // The way back is the row where the rest would be.
  await page.locator('.sbar-hidden-toggle[data-only-pinned]').click();
  await expect(page.locator('.sbar-group[data-group="pm:ANTBOT"]')).toHaveCount(1);

  expect(await errors(page)).toEqual([]);
});
