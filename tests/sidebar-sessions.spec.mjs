// The Sessions list behind its own rail icon (2026-09-15).
//
// André: "I have x sessions open and work in parallel on stuff", then "can we
// have them in their own top icon, not mixed with the projects? It will be
// less full." Every live zellij session in one list: the app's own with the
// agent's status word, the owner's cx-* with only their age, exited ones
// folded away under a count. Click opens the tab, right-click offers Close. The three sources are stubbed
// separately on purpose (zellij_sessions_info, agent_sessions_list, the
// app.js open global), because a test wired to one of them would pass with
// the other two pointing at nothing.
import { test, expect } from '@playwright/test';

const NOW = Date.now();
const STUB = {
  pm_project_list: [],
  tasks_list: [],
  git_worktree_list: [],
  loom_runs_list: [],
  zellij_sessions_info: [
    // busy: the server's process subtree is burning CPU, so the agent in it works.
    { name: 'cx-geo', created: '22h 29m 40s', created_ms: NOW - 80_000_000, last_active_ms: NOW - 3_600_000, exited: false, busy: true },
    { name: 'xnaut-claude-01m2gyfegj8', created: '5h 42m 34s', created_ms: NOW - 20_000_000, last_active_ms: NOW - 1000, exited: false },
    { name: 'cx-Keep', created: '6days 4h', created_ms: NOW - 500_000_000, last_active_ms: NOW - 400_000_000, exited: true },
    { name: 'cx-blogs', created: '19h 57m', created_ms: NOW - 70_000_000, last_active_ms: NOW - 7_200_000, exited: false },
    // The owner's own session the app adopted (it carries an agent row): a
    // rename must still show on the row, not the agent's label (2026-09-15).
    { name: 'me-102303', created: '1m', created_ms: NOW - 60_000, last_active_ms: NOW - 10_000_000, exited: false },
    // A dead app session: never listed, the prune removes it within a minute.
    { name: 'xnaut-pi-01m2h2ktsc8v1an', created: '8h 32m', created_ms: NOW - 30_000_000, last_active_ms: NOW - 20_000_000, exited: true },
  ],
  agent_sessions_list: [
    { session_id: 'xnaut-claude-01m2gyfegj8', agent_id: 'claude', label: 'Claudi · CHESSTRAINER-4', pane_key: 'p1',
      status: 'working', started_at_ms: 1, last_output_at_ms: 1, status_changed_at_ms: 1, zellij_session: 'xnaut-claude-01m2gyfegj8' },
    { session_id: 'me-102303', agent_id: 'me', label: 'me-102303', pane_key: 'p2',
      status: 'unknown', started_at_ms: 1, last_output_at_ms: 1, status_changed_at_ms: 1, zellij_session: 'me-102303' },
  ],
};

async function openSidebar(page) {
  await page.addInitScript(() => {
    localStorage.clear();
    localStorage.setItem('xnaut-sidebar-visible', '1');
  });
  await page.goto('/?stub=1');
  await page.waitForSelector('#btn-help');
  await page.evaluate((stub) => {
    Object.assign(window.__xnautStub, stub);
    window.__opened = [];
    window.__created = [];
    window.xnautShowSessionInHost = (name) => { window.__opened.push(name); return 'tab-host'; };
    window.__renamed = [];
    window.xnautSessionAlias = (name) => ({ 'cx-blogs': 'Blog drafts', 'me-102303': 'bin-movement' }[name] || '');
    window.xnautRenameSession = (name, alias) => { window.__renamed.push([name, alias]); };
    window.xnautPromptDialog = async () => 'Geo work';
    window.xnautConfirmDialog = async () => false;
    window.createNewTab = () => { window.__created.push('terminal'); };
  }, STUB);
  await page.waitForTimeout(1200);
  await page.evaluate(() => window.xnautSidebarRefresh());
  await page.getByRole('button', { name: 'Sessions', exact: true }).click();
  await expect(page.locator('.sbar-sessions .sbar-sess').first()).toBeVisible();
}

const errors = (page) => page.evaluate(() => window.__xnautErrors || []);

test('every session is listed once, the app\'s own first with its status word, exited folded', async ({ page }) => {
  await openSidebar(page);
  // The projects list steps aside; the rail icon carries the live count.
  await expect(page.locator('.sbar-projects')).toBeHidden();
  await expect(page.locator('.sbar-rail-btn[data-rail="sessions"] [data-badge]')).toHaveText('4');
  const rows = page.locator('.sbar-sessions .sbar-sess');
  await expect(rows, 'five listed: the dead xnaut-* one is not').toHaveCount(5);
  await expect(page.locator('.sbar-sess[data-session="xnaut-pi-01m2h2ktsc8v1an"]')).toHaveCount(0);
  await expect(rows.nth(0)).toHaveAttribute('data-session', 'xnaut-claude-01m2gyfegj8');
  await expect(rows.nth(0)).toHaveAttribute('data-state', 'working');
  await expect(rows.nth(0).locator('.sbar-name')).toHaveText('Claudi · CHESSTRAINER-4');
  await expect(rows.nth(1)).toHaveAttribute('data-session', 'cx-geo');
  await expect(rows.nth(1), 'his own busy session works, with the snake').toHaveAttribute('data-state', 'working');
  await expect(rows.nth(1).locator('.sbar-dot')).toHaveClass(/sbar-run/);
  await expect(page.locator('.sbar-sess[data-session="cx-blogs"]'), 'a quiet one is only live').toHaveAttribute('data-state', 'live');
  // Busy sticks: the CPU sample dips between an agent's turns, and the word
  // flapped live/working every five seconds (André's recording, 2026-09-16).
  await page.evaluate(() => { window.__xnautStub.zellij_sessions_info[0].busy = false; });
  await page.evaluate(() => window.xnautSidebarRefresh());
  await page.waitForTimeout(300);
  await expect(rows.nth(1), 'one quiet sample does not end working').toHaveAttribute('data-state', 'working');
  await expect(rows.nth(1).locator('.sbar-name')).toHaveText('cx-geo');
  // Blue for the app's own, yellow for the owner's: the class carries it.
  await expect(rows.nth(0)).toHaveClass(/sbar-sess-auto/);
  await expect(rows.nth(1)).toHaveClass(/sbar-sess-manual/);
  await expect(page.locator('[data-sess-plus]')).toBeVisible();
  await expect(rows.nth(4)).toHaveAttribute('data-state', 'exited');
  await expect(rows.nth(4)).toHaveClass(/sbar-exited/);
  // Exited ones are folded, not gone: hidden until the fold is opened.
  await expect(rows.nth(4)).toBeHidden();
  await page.locator('.sbar-sess-fold > summary').click();
  await expect(rows.nth(4)).toBeVisible();
  // The header counts what is alive, not what can be resurrected.
  await expect(page.locator('[data-sess-count]')).toHaveText('4');
  // An alias names the row; the zellij name moves to the second line.
  const blogs = page.locator('.sbar-sess[data-session="cx-blogs"]');
  await expect(blogs.locator('.sbar-name')).toHaveText('Blog drafts');
  await expect(blogs.locator('.sbar-sub')).toHaveText('cx-blogs');
  const me = page.locator('.sbar-sess[data-session="me-102303"]');
  await expect(me.locator('.sbar-name'), 'the alias beats the adopted agent label').toHaveText('bin-movement');
  await expect(me, 'adopted or not, his own session is his colour').toHaveClass(/sbar-sess-manual/);
  expect(await errors(page)).toEqual([]);
});

test('a click opens the session through the app, and the icon toggles back to projects', async ({ page }) => {
  await openSidebar(page);
  await page.locator('.sbar-sessions .sbar-sess[data-session="cx-geo"]').click();
  expect(await page.evaluate(() => window.__opened)).toEqual(['cx-geo']);
  // The list is the switcher: the chosen row is the active one.
  await expect(page.locator('.sbar-sess[data-session="cx-geo"]')).toHaveClass(/sbar-row-active/);
  // The + starts a new session of the owner's own; no project selected, so home.
  await page.locator('[data-sess-plus]').click();
  expect(await page.evaluate(() => window.__created), 'a plain terminal tab, not a zellij session').toEqual(['terminal']);

  await page.getByRole('button', { name: 'Sessions', exact: true }).click();
  await expect(page.locator('.sbar-sessions')).toBeHidden();
  await expect(page.locator('.sbar-projects')).toBeVisible();
  expect(await page.evaluate(() => localStorage.getItem('xnaut-sidebar-view'))).toBe('projects');
  expect(await errors(page)).toEqual([]);
});

test('right-click offers Rename, which stores the name on the session', async ({ page }) => {
  await openSidebar(page);
  await page.locator('.sbar-sessions .sbar-sess[data-session="cx-geo"]').click({ button: 'right' });
  await page.locator('.sbar-menu-item', { hasText: 'Rename' }).click();
  await expect.poll(() => page.evaluate(() => window.__renamed)).toEqual([['cx-geo', 'Geo work']]);
  expect(await errors(page)).toEqual([]);
});

test('right-click offers Close, and closing a live session asks first', async ({ page }) => {
  await openSidebar(page);
  await page.evaluate(() => {
    window.__deleted = [];
    window.__xnautStub.zellij_delete_session = (args) => { window.__deleted.push(args); return null; };
  });
  await page.locator('.sbar-sessions .sbar-sess[data-session="cx-geo"]').click({ button: 'right' });
  await page.locator('.sbar-menu-item', { hasText: 'Close session' }).click();
  await page.waitForTimeout(200);
  // The app's dialog answered no: nothing was deleted.
  expect(await page.evaluate(() => window.__deleted)).toEqual([]);
  expect(await errors(page)).toEqual([]);
});
