import { test, expect } from '@playwright/test';

// Every close button used to be a bare "×". That is not a name: it is the same
// character on thirteen different controls, so a screen reader announces
// thirteen identical buttons and axui refuses to press any of them (it will not
// guess between ambiguous matches, which is why the release test reported
// NOT PRESSED rather than closing the wrong dialog).
//
// This guards the fix rather than the markup: it fails the moment someone adds
// a new modal with an unlabelled ×, which is exactly how the original eleven
// accumulated.

test.beforeEach(async ({ page }) => {
  page.on('dialog', (d) => d.dismiss().catch(() => {}));
  await page.goto('/index.html');
  await page.waitForSelector('#btn-help');
});

test('every close button has a unique accessible name', async ({ page }) => {
  // title= is deliberately not accepted: it maps to AXHelp, not AXTitle, so it
  // is help text and not a name.
  const named = await page.$$eval('.btn-close, .btn-icon[id^="btn-close"]', (els) =>
    els.map((e) => ({ id: e.id, label: e.getAttribute('aria-label') || '' })));

  expect(named.length).toBeGreaterThan(10);

  const unnamed = named.filter((n) => !n.label).map((n) => n.id);
  expect(unnamed, 'close buttons with no aria-label').toEqual([]);

  const labels = named.map((n) => n.label);
  expect(new Set(labels).size, `duplicate names: ${labels.join(', ')}`).toBe(labels.length);

  // Substring collisions are as bad as duplicates, because that is how the
  // matcher looks names up: pressing "Close settings" must not also match
  // "Close settings dialog".
  const collisions = labels.filter((a) =>
    labels.some((b) => a !== b && b.toLowerCase().includes(a.toLowerCase())));
  expect(collisions, 'names that are substrings of another name').toEqual([]);
});
