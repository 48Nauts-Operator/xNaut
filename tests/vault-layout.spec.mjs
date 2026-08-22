import { test, expect } from '@playwright/test';

test('Vault replaces the master menu and uses the next pane for its navigator', async ({ page }) => {
  page.on('pageerror', (error) => { throw error; });
  await page.addInitScript(() => localStorage.setItem('xnaut-sidebar-visible', '1'));
  await page.goto('/?stub=1');
  await page.waitForSelector('.sbar-nav-row');
  await page.locator('.sbar-nav-row', { hasText: 'Vault' }).click();

  await expect(page.locator('.sbar-submenu')).toBeVisible();
  await expect(page.locator('.sbar-nav')).toBeHidden();
  await expect(page.locator('.sbar-projects')).toBeHidden();
  await expect(page.locator('.sbar-submenu-body .vp-master')).toBeVisible();
  await expect(page.locator('.sbar-submenu-body')).toContainText('Work');
  await expect(page.locator('.sbar-submenu-body')).toContainText('Private');
  await expect(page.locator('.vp-rail')).toBeVisible();
  await expect(page.locator('.vp-chat-section')).toBeVisible();
  await expect(page.locator('.vp-chat-section .chatp-input')).toHaveAttribute('rows', '2');
  expect(await page.locator('.vp-run-switch button').allTextContents()).toEqual(['Chat', 'Tickets', 'Changes']);
  await page.locator('.vp-chat-section .chatp-input').fill('Help with this document');
  await page.locator('.vp-chat-section .chatp-input').press('Enter');
  await expect(page.locator('.vp-chat-section .chatp-list')).toContainText('NautBot reply');

  await page.locator('.vp-tree-label', { hasText: 'Architecture' }).first().click();
  await page.getByText('PM Space', { exact: true }).last().click();
  await page.getByRole('button', { name: 'Tickets', exact: true }).click();
  const ticket = page.locator('.vp-ticket', { hasText: 'SMOKE-1' });
  await expect(ticket).toBeVisible();
  await expect(ticket.locator('.vp-ticket-status')).toHaveText('ready');
  await expect(ticket.locator('.vp-ticket-text')).not.toBeVisible();
  await ticket.locator('summary').click();
  await expect(ticket.locator('.vp-ticket-text')).toContainText('Full ticket text shown after expansion.');
  await expect(ticket.locator('.vp-ticket-text h2')).toHaveText(['The Issue', 'The Fix']);
  await expect(ticket.locator('.vp-ticket-stats')).toContainText('+12 −3');
  await page.getByRole('button', { name: 'Changes', exact: true }).click();
  await expect(page.locator('.vp-run-heading')).toContainText('Changes · SMOKE-1 · 1');
  await expect(page.locator('.vp-code-file')).toContainText('src/example.js');
  const tabsBeforeReview = await page.locator('#tabs-container > *').count();
  await page.locator('.vp-code-file').click();
  await expect(page.locator('#tabs-container > *')).toHaveCount(tabsBeforeReview + 1);
});
