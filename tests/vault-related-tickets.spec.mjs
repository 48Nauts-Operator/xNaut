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

async function openVaultTickets(page) {
  await page.addInitScript(() => localStorage.setItem('xnaut-sidebar-visible', '1'));
  await page.goto('/?stub=1');
  await page.waitForSelector('#btn-help');
  await page.evaluate((tickets) => { window.__xnautStub.pm_ticket_list = tickets; }, TICKETS);
  await page.waitForTimeout(2000);
  await page.getByRole('button', { name: 'More surfaces', exact: true }).click();
  await page.locator('.sbar-menu-item', { hasText: 'Vault' }).click();
  await expect(page.locator('.sbar-submenu')).toBeVisible();
}

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
