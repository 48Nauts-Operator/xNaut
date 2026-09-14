// Ticket ids in rendered prose are links with a hover card (ticket-links.js).
//
// Andre, 2026-09-14: "is it possible to add links to the tickets in the chat,
// so I could hover over them, and an overlay shows me the ticket?"
import { test, expect } from '@playwright/test';

const TICKETS = [
  { id: 'XNAUT-354', project: 'XNAUT', title: 'NautBot offers the swarm', type: 'feature', status: 'complete',
    priority: 'medium', owner: 'claude', release: '1.27.0', body: 'The Issue\nTwo ways to put agents on a batch.', updated_at: '2026-09-13T20:00:00Z' },
  { id: 'XNAUT-361', project: 'XNAUT', title: 'Parallel-only test failures', type: 'bug', status: 'ready',
    priority: 'low', owner: '', release: '', body: '', updated_at: '2026-09-13T20:00:00Z' },
];

async function boot(page) {
  await page.goto('/index.html?stub=1');
  await page.waitForFunction(() => typeof window.xnautLinkTickets === 'function');
  await page.evaluate((tickets) => {
    window.__xnautStub.pm_ticket_list = tickets;
    const host = document.createElement('div');
    host.id = 'tl-host';
    host.style.cssText = 'position:fixed;top:40px;left:40px;z-index:9999;background:#111;padding:12px;color:#eee';
    host.innerHTML = '<p>Blockers: <strong>XNAUT-354</strong> still needs sign-off; XNAUT-361 remains ready; <code>XNAUT-999</code> in code is left alone.</p>';
    document.body.appendChild(host);
    window.__linked = window.xnautLinkTickets(host);
  }, TICKETS);
}

test('ids become links, code is left alone, hover shows the ticket', async ({ page }) => {
  await boot(page);
  expect(await page.evaluate(() => window.__linked)).toBe(2);
  const links = page.locator('#tl-host a.xtl');
  await expect(links).toHaveCount(2);
  await expect(links.first()).toHaveText('XNAUT-354');
  await expect(page.locator('#tl-host code')).toHaveText('XNAUT-999');

  await links.first().hover();
  const card = page.locator('.xtl-card');
  await expect(card).toBeVisible();
  await expect(card.locator('.xtl-title')).toHaveText('NautBot offers the swarm');
  await expect(card.locator('.xtl-status')).toHaveText('Complete');
  await expect(card.locator('.xtl-meta')).toContainText('release 1.27.0');
  await expect(card.locator('.xtl-body')).toContainText('Two ways to put agents on a batch.');

  // Twenty ids cost one call: the second hover reads the cache.
  await links.nth(1).hover();
  await expect(card.locator('.xtl-title')).toHaveText('Parallel-only test failures');
  const calls = await page.evaluate(() => window.__xnautInvokes.filter((i) => i.cmd === 'pm_ticket_list').length);
  expect(calls).toBe(1);
  expect(await page.evaluate(() => window.__xnautErrors || [])).toEqual([]);
});

test('clicking a link opens the ticket on the Work tab', async ({ page }) => {
  await boot(page);
  await page.evaluate(() => {
    window.__opened = [];
    const real = window.xnautOpenWorkspace;
    window.xnautOpenWorkspace = (o) => { window.__opened.push(o); return real ? real(o) : null; };
  });
  await page.locator('#tl-host a.xtl').first().click();
  expect(await page.evaluate(() => window.__opened)).toEqual([{ project: 'XNAUT', tab: 'work', ticket: 'XNAUT-354' }]);
  // The Workspace lands on Work with that ticket's detail open.
  await expect(page.locator('.wsp .pmw-detail-head')).toContainText('XNAUT-354', { timeout: 8000 });
  expect(await page.evaluate(() => window.__xnautErrors || [])).toEqual([]);
});
