import { test, expect } from '@playwright/test';

test('Vault replaces the master menu and uses the next pane for its navigator', async ({ page }) => {
  page.on('pageerror', (error) => { throw error; });
  await page.addInitScript(() => localStorage.setItem('xnaut-sidebar-visible', '1'));
  await page.goto('/?stub=1');
  await page.getByRole('button', { name: 'More surfaces', exact: true }).waitFor();
  // Vault moved behind the More menu when the rail replaced the rows (XNAUT-335).
  await page.getByRole('button', { name: 'More surfaces', exact: true }).click();
  await page.locator('.sbar-menu-item', { hasText: 'Vault' }).click();

  await expect(page.locator('.sbar-submenu')).toBeVisible();
  await expect(page.locator('.sbar-nav')).toBeHidden();
  await expect(page.locator('.sbar-projects')).toBeHidden();
  await expect(page.locator('.sbar-submenu-body .vp-master')).toBeVisible();
  await expect(page.locator('.sbar-submenu-body')).toContainText('Work');
  await expect(page.locator('.sbar-submenu-body')).toContainText('Private');
  await expect(page.locator('.vp-rail')).toBeVisible();
  await expect(page.locator('.vp-chat-section')).toBeVisible();
  await expect(page.locator('.vp-chat-section .chatp-input')).toHaveAttribute('rows', '2');
  expect(await page.locator('.vp-run-switch button').allTextContents()).toEqual(['Chat', 'Tickets', 'Changes', 'Files']);
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
  // Two fixed columns rather than one string: the spacing is a flex gap now,
  // so the added and removed counts line up down the list instead of each row
  // placing them wherever its own digits end (2026-09-13).
  await expect(ticket.locator('.vp-ticket-add')).toHaveText('+12');
  await expect(ticket.locator('.vp-ticket-del')).toHaveText('−3');
  await page.getByRole('button', { name: 'Changes', exact: true }).click();
  await expect(page.locator('.vp-run-heading')).toContainText('Changes · SMOKE · 1');
  // fileRow renders basename and directory as separate spans on purpose, so
  // the path is never one contiguous string. Assert the parts it does render.
  await expect(page.locator('.vp-code-file .vp-code-base')).toHaveText('example.js');
  await expect(page.locator('.vp-code-file .vp-code-dir')).toHaveText('src');
  // Clicking a changed file opens it in the pane's own center viewer.
  // vault-pane.js never touches #tabs-container, which is what the original
  // assertion here expected; it had never run, so nothing regressed.
  await page.locator('.vp-code-file').click();
  await expect(page.locator('.vp-cv-body')).toBeVisible();
  await expect(page.locator('.vp-cv-path')).toHaveText('src/example.js');
});
