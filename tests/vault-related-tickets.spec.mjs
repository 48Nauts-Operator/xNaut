// The ticket pane follows what the reader has open.
//
// Andre, 2026-09-13, with a design note open and all 56 of NautGate's tickets
// listed beside it: "I see all tickets not just the ones related to the
// highlighted feature on the left pane."
//
// The link already existed and nothing was reading it: every ticket carries
// `documentation: ["work:<project>/Development/features/<note>.md"]`, which is
// the note it was written against. That is a link the app maintains, so it is
// what "related" means here, not a guess at filenames.
import { test, expect } from '@playwright/test';

const NOTE = 'xnaut/Development/features/vault-filter.md';

const TICKETS = [
  { id: 'SMOKE-1', project: 'SMOKE', title: 'Written against the note', type: 'feature',
    status: 'ready', priority: 'high', owner: 'Builder', body: 'Linked by documentation.',
    documentation: [`work:${NOTE}`], updated_at: '2026-08-22T20:00:00Z' },
  { id: 'SMOKE-2', project: 'SMOKE', title: 'A different piece of work', type: 'task',
    status: 'inbox', priority: 'low', owner: '', body: 'Nothing to do with that note.',
    documentation: ['work:xnaut/Development/features/something-else.md'],
    updated_at: '2026-08-21T20:00:00Z' },
  { id: 'SMOKE-3', project: 'SMOKE', title: 'Also unrelated', type: 'task',
    status: 'inbox', priority: 'low', owner: '', body: 'Nor this one.',
    documentation: [], updated_at: '2026-08-20T20:00:00Z' },
];

async function openVaultTickets(page, tickets = TICKETS) {
  await page.addInitScript(() => {
    localStorage.setItem('xnaut-sidebar-visible', '1');
    localStorage.setItem('xnaut-right-pane-visible', '1');
  });
  await page.goto('/?stub=1');
  await page.waitForSelector('#btn-help');
  await page.evaluate((tickets) => { window.__xnautStub.pm_ticket_list = tickets; }, tickets);
  await page.waitForTimeout(2000);
  await page.getByRole('button', { name: 'More surfaces', exact: true }).click();
  await page.locator('.sbar-menu-item', { hasText: 'Vault' }).click();
  await expect(page.locator('.sbar-submenu')).toBeVisible();
}

test('Vault keeps navigation visible and the right Journal follows the open document project', async ({ page }) => {
  await openVaultTickets(page);
  await expect(page.getByRole('button', { name: 'Projects', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Sessions', exact: true })).toBeVisible();
  await page.evaluate(() => {
    window.__xnautStub.project_journal_read = {
      project: { key: 'SMOKE', name: 'Smoke Test' }, path: 'Development/journal/2026-10-08.md',
      documents: [], opening: '', entries: [], runs: [], groups: [], observed_at: '2026-10-08T13:00:00Z',
      continuity: { project: 'SMOKE', tickets: [], assignments: [], diagnostics: [] },
    };
    window.xnautRightPaneShow('workspace');
    window.xnautRightPaneSetRoot('/home/previous-project');
  });
  await page.locator('.vp-filter-input').fill('pm-space');
  await page.locator('.vp-body').getByText('PM Space', { exact: true }).first().click();
  await expect(page.locator('.rpane-title')).toHaveAttribute('title', '/tmp/smoke');
  await page.locator('.rpws-nav [data-sub="journal"]').click();
  await expect(page.locator('.pj [data-title]')).toHaveText('Smoke Test · Live Journal');
  const calls = await page.evaluate(() => window.__xnautInvokes.filter(i => i.cmd === 'project_journal_read'));
  expect(calls.at(-1).args.project).toBe('/tmp/smoke');
  // Opening a document in Chat must not start any ticket history scan.
  expect(await page.evaluate(() => window.__xnautInvokes.filter(i => i.cmd === 'git_ticket_files').length)).toBe(0);
  expect(await page.evaluate(() => window.xnautActiveProjectKey())).toBe('SMOKE');
});

test('large Vault ticket lists render immediately, bound Git work and pause when left', async ({ page }) => {
  const tickets = Array.from({ length: 500 }, (_, i) => ({ ...TICKETS[1], id: `SMOKE-${i + 1}`, body: 'Large ticket evidence. '.repeat(1000) }));
  await openVaultTickets(page, tickets);
  await page.evaluate(() => {
    const invoke = window.__TAURI__.core.invoke;
    window.__pendingGit = [];
    window.__TAURI__.core.invoke = (cmd, args) => {
      if (cmd !== 'git_ticket_files') return invoke(cmd, args);
      window.__xnautInvokes.push({ cmd, args });
      return new Promise(resolve => window.__pendingGit.push(resolve));
    };
  });
  await openNote(page, 'pm-space', 'PM Space');
  await expect(page.locator('.vp-ticket')).toHaveCount(500);
  await expect(page.locator('.vp-ticket-text').first()).toBeEmpty();
  expect(await page.evaluate(() => window.__pendingGit.length)).toBe(1);
  const tab = await page.locator('.tab.active').getAttribute('data-session-id');
  await page.evaluate(() => window.xnautAttachMarkdownTab({ filename: 'Other document' }));
  await page.evaluate(() => window.__pendingGit[0]([]));
  await page.waitForTimeout(100);
  expect(await page.evaluate(() => window.__pendingGit.length)).toBe(1);
  await page.locator(`.tab[data-session-id="${tab}"]`).click();
  await expect.poll(() => page.evaluate(() => window.__pendingGit.length)).toBe(2);
});

// The ticket pane is scoped to a project, and the project comes from the open
// note, so a note has to be open before there is anything to list.
async function openNote(page, filter, title) {
  // The filename filter rather than expanding folders: the linked note is
  // three levels deep, which is the case vault-filename-filter.spec.mjs exists
  // for, and clicking through the tree is not what this test is about.
  await page.locator('.vp-filter-input').fill(filter);
  await page.locator('.vp-body').getByText(title, { exact: true }).first().click();
  await page.waitForTimeout(400);
  await page.getByRole('button', { name: 'Tickets', exact: true }).click();
  await page.waitForTimeout(400);
}

test('a note nothing links to shows every ticket', async ({ page }) => {
  await openVaultTickets(page);
  await openNote(page, 'pm-space', 'PM Space');
  await expect(page.locator('.vp-run-body > .vp-ticket')).toHaveCount(3);
  expect(await page.evaluate(() => window.__xnautErrors || [])).toEqual([]);
});

test('opening a note leads with the tickets written against it', async ({ page }) => {
  await openVaultTickets(page);
  await openNote(page, 'vault-fil', 'Vault Filter');

  // The one linked ticket is above the fold, named by the note it belongs to.
  await expect(page.locator('.vp-ticket-group').first()).toContainText('1 linked to vault-filter.md');
  const shown = page.locator('.vp-run-body > .vp-ticket');
  await expect(shown).toHaveCount(1);
  await expect(shown.first()).toContainText('SMOKE-1');

  // The other two are one click away, never hidden: "related" is a heuristic
  // and a ticket you cannot reach is worse than a list you have to scroll.
  const rest = page.locator('.vp-ticket-rest');
  await expect(rest).toContainText('2 other tickets in this project');
  await rest.locator('> summary').click();   // its own, not each ticket's
  await expect(rest.locator('.vp-ticket')).toHaveCount(2);

  expect(await page.evaluate(() => window.__xnautErrors || [])).toEqual([]);
});

test('the empty case names what it looked for', async ({ page }) => {
  await openVaultTickets(page);
  await openNote(page, 'pm-space', 'PM Space');

  await expect(page.locator('.vp-ticket-group').first()).toContainText('Nothing links to pm-space.md, showing all 3');
  await expect(page.locator('.vp-run-body > .vp-ticket')).toHaveCount(3);
  await expect(page.locator('.vp-ticket-rest')).toHaveCount(0);

  expect(await page.evaluate(() => window.__xnautErrors || [])).toEqual([]);
});
