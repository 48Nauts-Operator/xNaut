// The Work tab's filter reads the columns, not the body.
//
// Andre, 2026-09-14: "the free-text is not working its showing random
// details. If I enter In Progress I get" five tickets in five different
// statuses, because the old filter searched the body and every long ticket
// mentions the phrase somewhere. Also: the Type column was empty (the panel
// read `ticket_type`, the backend sends `type`) and the release a ticket is
// going into was not shown at all.
import { test, expect } from '@playwright/test';

const T = (id, title, status, extra = {}) => ({
  id, project: 'SMOKE', title, type: 'feature', status, priority: 'medium', owner: 'claude',
  body: 'Long body. Currently in progress on the branch; review later.', release: '', tags: [],
  documentation: [], updated_at: '2026-09-13T20:00:00Z', ...extra,
});
const TICKETS = [
  T('SMOKE-1', 'Really being worked on', 'in_progress', { release: '1.28' }),
  T('SMOKE-2', 'Sitting in review', 'review', { type: 'bug', owner: 'codex', priority: 'high', updated_at: '2026-09-12T20:00:00Z' }),
  T('SMOKE-3', 'Only its body mentions the phrase', 'ready', { priority: 'low', updated_at: '2026-09-14T20:00:00Z' }),
];

async function openWorkList(page) {
  page.on('dialog', (d) => d.dismiss().catch(() => {}));
  await page.goto('/index.html?stub=1');
  await page.waitForFunction(() => typeof window.xnautCreateProjectManagementPanel === 'function');
  await page.evaluate((tickets) => {
    window.__xnautStub.pm_ticket_list = tickets;
    const host = document.createElement('div');
    host.id = 'pm-test-host';
    document.body.appendChild(host);
    window.xnautCreateProjectManagementPanel('pm-test', host, {});
  }, TICKETS);
  const pane = page.locator('#pm-test-host .pmw');
  await pane.locator('[data-project]:not([data-project=""])').first().click();
  await pane.locator('[data-project-section="work"]').click();
  await pane.locator('[data-view="list"]').click();
  await expect(pane.locator('table.pmw-list tbody tr')).toHaveCount(3);
  return pane;
}

const ids = (pane) => pane.locator('table.pmw-list tbody tr').evaluateAll((rows) => rows.map((r) => r.dataset.id));

test('a status typed as words matches the status, not every body that mentions it', async ({ page }) => {
  const pane = await openWorkList(page);
  await pane.locator('.pmw-filter').fill('In Progress');
  expect(await ids(pane)).toEqual(['SMOKE-1']);
  await pane.locator('.pmw-filter').fill('status:review');
  expect(await ids(pane)).toEqual(['SMOKE-2']);
  await pane.locator('.pmw-filter').fill('owner:codex type:bug');
  expect(await ids(pane)).toEqual(['SMOKE-2']);
  await pane.locator('.pmw-filter').fill('release:1.28');
  expect(await ids(pane)).toEqual(['SMOKE-1']);
  // A title word still finds its ticket.
  await pane.locator('.pmw-filter').fill('sitting');
  expect(await ids(pane)).toEqual(['SMOKE-2']);
  expect(await page.evaluate(() => window.__xnautErrors || [])).toEqual([]);
});

test('type and release are shown in the list', async ({ page }) => {
  const pane = await openWorkList(page);
  const row = pane.locator('table.pmw-list tbody tr[data-id="SMOKE-1"]');
  await expect(row.locator('.pmw-c-type')).toHaveText('feature');
  await expect(row.locator('.pmw-c-release')).toHaveText('1.28');
  await expect(pane.locator('table.pmw-list tbody tr[data-id="SMOKE-2"] .pmw-c-type')).toHaveText('bug');
  await expect(pane.locator('table.pmw-list thead')).toContainText('Release');
});

// XNAUT-393: a Copy id button on every row. The row itself opens the ticket,
// so the interesting half of this is that the copy does NOT.
test('every row has a copy button that copies the id without opening the ticket', async ({ page }) => {
  const pane = await openWorkList(page);
  // Stub the clipboard: the real one needs a secure origin and a focused
  // document, and we want to assert the written text anyway.
  await page.evaluate(() => {
    window.__copied = [];
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { writeText: (text) => { window.__copied.push(text); return Promise.resolve(); } },
    });
  });
  await expect(pane.locator('table.pmw-list tbody tr .pmw-copy-id')).toHaveCount(3);
  const button = pane.locator('table.pmw-list tbody tr[data-id="SMOKE-2"] .pmw-copy-id');
  await expect(button).toHaveAttribute('title', 'Copy SMOKE-2');
  await button.click();
  // The toast is asserted first: it removes itself after a second.
  await expect(pane.locator('.pmw-toast')).toHaveText('Copied SMOKE-2');
  expect(await page.evaluate(() => window.__copied)).toEqual(['SMOKE-2']);
  // The detail pane stays shut: the click never reached the row.
  await expect(pane.locator('.pmw-detail-head')).toHaveCount(0);
  // The toast clears itself after about a second.
  await expect(pane.locator('.pmw-toast')).toHaveCount(0, { timeout: 4000 });
  // Keyboard: focus the button and press Enter.
  const first = pane.locator('table.pmw-list tbody tr[data-id="SMOKE-1"] .pmw-copy-id');
  await first.focus();
  await page.keyboard.press('Enter');
  expect(await page.evaluate(() => window.__copied)).toEqual(['SMOKE-2', 'SMOKE-1']);
  await expect(pane.locator('.pmw-detail-head')).toHaveCount(0);
  expect(await page.evaluate(() => window.__xnautErrors || [])).toEqual([]);
});

// Andre, 2026-09-14: "can you make the header of that table sortable".
test('headers sort by rank, flip on a second click, and the choice survives a re-render', async ({ page }) => {
  const pane = await openWorkList(page);
  // Default: newest first.
  expect(await ids(pane)).toEqual(['SMOKE-3', 'SMOKE-1', 'SMOKE-2']);
  await pane.locator('th[data-sort="priority"]').click();
  expect(await ids(pane), 'priority sorts by rank, high first').toEqual(['SMOKE-2', 'SMOKE-1', 'SMOKE-3']);
  await pane.locator('th[data-sort="priority"]').click();
  expect(await ids(pane), 'a second click flips it').toEqual(['SMOKE-3', 'SMOKE-1', 'SMOKE-2']);
  await pane.locator('th[data-sort="status"]').click();
  expect(await ids(pane), 'status follows the board order').toEqual(['SMOKE-3', 'SMOKE-1', 'SMOKE-2']);
  await expect(pane.locator('th[data-sort="status"]')).toHaveAttribute('aria-sort', 'ascending');
  // A filter re-renders the table; the sort stays.
  await pane.locator('.pmw-filter').fill('smoke');
  expect(await ids(pane)).toEqual(['SMOKE-3', 'SMOKE-1', 'SMOKE-2']);
  expect(await page.evaluate(() => localStorage.getItem('xnaut-pm-list-sort'))).toBe('{"key":"status","dir":"asc"}');
});

// XNAUT-395. A filter you have to select-all-and-delete to drop is a filter
// people leave on by accident and then read the short list as the whole list.
// Both clears go through the same path as typing, so the only thing these
// assert beyond "the rows came back" is that the box is usable immediately
// afterwards: still focused, x gone.
test('the x clears the filter, and is there only while the box has text', async ({ page }) => {
  const pane = await openWorkList(page);
  const input = pane.locator('.pmw-filter');
  const clear = pane.locator('.pmw-filter-clear');

  await expect(clear, 'an empty box has nothing to clear').toBeHidden();

  await input.fill('sitting');
  expect(await ids(pane)).toEqual(['SMOKE-2']);
  await expect(clear).toBeVisible();

  await clear.click();
  await expect(input).toHaveValue('');
  expect(await ids(pane), 'every ticket is back').toEqual(['SMOKE-3', 'SMOKE-1', 'SMOKE-2']);
  await expect(clear).toBeHidden();
  await expect(input, 'the next thing typed goes in the box').toBeFocused();

  expect(await page.evaluate(() => window.__xnautErrors || [])).toEqual([]);
});

test('Escape in the filter box clears it the same way', async ({ page }) => {
  const pane = await openWorkList(page);
  const input = pane.locator('.pmw-filter');
  const clear = pane.locator('.pmw-filter-clear');

  await input.fill('status:review');
  expect(await ids(pane)).toEqual(['SMOKE-2']);
  await expect(clear).toBeVisible();

  await input.press('Escape');
  await expect(input).toHaveValue('');
  expect(await ids(pane), 'every ticket is back').toEqual(['SMOKE-3', 'SMOKE-1', 'SMOKE-2']);
  await expect(clear).toBeHidden();
  await expect(input).toBeFocused();

  // Escape on an empty box is not ours to swallow: it belongs to whatever is
  // above us that closes on Escape.
  await input.press('Escape');
  await expect(input).toHaveValue('');
  expect(await ids(pane)).toEqual(['SMOKE-3', 'SMOKE-1', 'SMOKE-2']);

  expect(await page.evaluate(() => window.__xnautErrors || [])).toEqual([]);
});
