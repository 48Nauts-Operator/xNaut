// The Sessions section above the projects (2026-09-15).
//
// André: "I have x sessions open and work in parallel on stuff." Every live
// zellij session in one place: the app's own with the agent's status word,
// the owner's cx-* with only their age, exited ones dimmed and last. Click
// opens the tab, right-click offers Close. The three sources are stubbed
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
    { name: 'cx-geo', created: '22h 29m 40s', created_ms: NOW - 80_000_000, last_active_ms: NOW - 3_600_000, exited: false },
    { name: 'xnaut-claude-01m2gyfegj8', created: '5h 42m 34s', created_ms: NOW - 20_000_000, last_active_ms: NOW - 1000, exited: false },
    { name: 'cx-Keep', created: '6days 4h', created_ms: NOW - 500_000_000, last_active_ms: NOW - 400_000_000, exited: true },
  ],
  agent_sessions_list: [
    { session_id: 'xnaut-claude-01m2gyfegj8', agent_id: 'claude', label: 'Claudi · CHESSTRAINER-4', pane_key: 'p1',
      status: 'working', started_at_ms: 1, last_output_at_ms: 1, status_changed_at_ms: 1, zellij_session: 'xnaut-claude-01m2gyfegj8' },
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
    window.xnautFocusTabForSession = () => false;
    window.xnautOpenZellijSession = (name, opts) => { window.__opened.push([name, opts]); };
  }, STUB);
  await page.waitForTimeout(1200);
  await page.evaluate(() => window.xnautSidebarRefresh());
  await expect(page.locator('.sbar-sessions .sbar-sess').first()).toBeVisible();
}

const errors = (page) => page.evaluate(() => window.__xnautErrors || []);

test('every session is listed once, the app\'s own first with its status word, exited last', async ({ page }) => {
  await openSidebar(page);
  const rows = page.locator('.sbar-sessions .sbar-sess');
  await expect(rows).toHaveCount(3);
  await expect(rows.nth(0)).toHaveAttribute('data-session', 'xnaut-claude-01m2gyfegj8');
  await expect(rows.nth(0)).toHaveAttribute('data-state', 'working');
  await expect(rows.nth(0).locator('.sbar-name')).toHaveText('Claudi · CHESSTRAINER-4');
  await expect(rows.nth(1)).toHaveAttribute('data-session', 'cx-geo');
  await expect(rows.nth(1)).toHaveAttribute('data-state', 'live');
  await expect(rows.nth(1).locator('.sbar-name')).toHaveText('cx-geo');
  await expect(rows.nth(2)).toHaveAttribute('data-state', 'exited');
  await expect(rows.nth(2)).toHaveClass(/sbar-exited/);
  // The header counts what is alive, not what can be resurrected.
  await expect(page.locator('[data-sess-count]')).toHaveText('2');
  expect(await errors(page)).toEqual([]);
});

test('a click opens the session through the app, and the header folds the list', async ({ page }) => {
  await openSidebar(page);
  await page.locator('.sbar-sessions .sbar-sess[data-session="cx-geo"]').click();
  expect(await page.evaluate(() => window.__opened)).toEqual([['cx-geo', { focus: true }]]);

  await page.locator('.sbar-section-head', { hasText: 'Sessions' }).click();
  await expect(page.locator('.sbar-sessions')).toBeHidden();
  expect(await page.evaluate(() => localStorage.getItem('xnaut-sessions-collapsed'))).toBe('1');
  expect(await errors(page)).toEqual([]);
});

test('right-click offers Close, and closing a live session asks first', async ({ page }) => {
  await openSidebar(page);
  await page.evaluate(() => {
    window.__deleted = [];
    window.__xnautStub.zellij_delete_session = (args) => { window.__deleted.push(args); return null; };
  });
  page.on('dialog', (d) => d.dismiss());
  await page.locator('.sbar-sessions .sbar-sess[data-session="cx-geo"]').click({ button: 'right' });
  await page.locator('.sbar-menu-item', { hasText: 'Close session' }).click();
  // Dismissed the question: nothing was deleted.
  expect(await page.evaluate(() => window.__deleted)).toEqual([]);
  expect(await errors(page)).toEqual([]);
});
