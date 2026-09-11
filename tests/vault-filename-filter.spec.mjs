import { test, expect } from '@playwright/test';

// XNAUT-44: the Notes rail filters by filename. The trigger was a note under
// xnaut/Development/features/ being invisible until every folder was expanded
// by hand, so depth-independence is the assertion that matters here.
test('Vault Notes rail filters the tree by filename', async ({ page }) => {
  page.on('pageerror', (error) => { throw error; });
  await page.addInitScript(() => localStorage.setItem('xnaut-sidebar-visible', '1'));
  await page.goto('/?stub=1');
  await page.getByRole('button', { name: 'More surfaces', exact: true }).waitFor();
  // Vault moved behind the More menu when the rail replaced the rows (XNAUT-335).
  await page.getByRole('button', { name: 'More surfaces', exact: true }).click();
  await page.locator('.sbar-menu-item', { hasText: 'Vault' }).click();

  const filter = page.locator('.vp-filter-input');
  const body = page.locator('.vp-body');
  await expect(filter).toBeVisible();
  await expect(page.locator('.vp-count')).toHaveText('3 notes');

  // The deep note is not reachable without expanding folders.
  await expect(body.getByText('Vault Filter', { exact: true })).toBeHidden();

  await filter.fill('vault-fil');
  await expect(page.locator('.vp-count')).toHaveText('1 of 3 notes');
  await expect(body.getByText('Vault Filter', { exact: true })).toBeVisible();
  await expect(body.getByText('Welcome', { exact: true })).toHaveCount(0);

  // Title matches too, case-insensitively.
  await filter.fill('PM SPACE');
  await expect(body.getByText('PM Space', { exact: true })).toBeVisible();
  await expect(page.locator('.vp-count')).toHaveText('1 of 3 notes');

  await filter.fill('zzz-nothing');
  await expect(body).toContainText('No filename matches');

  // Clearing restores the full tree, collapsed as before.
  await filter.fill('');
  await expect(page.locator('.vp-count')).toHaveText('3 notes');
  await expect(body.getByText('Welcome', { exact: true })).toBeVisible();
  await expect(body.getByText('Vault Filter', { exact: true })).toBeHidden();

  // The filter belongs to Notes only.
  await page.locator('.vp-tabs button[data-tab="tags"]').click();
  await expect(filter).toBeHidden();
  await page.locator('.vp-tabs button[data-tab="notes"]').click();
  await expect(filter).toBeVisible();
});
