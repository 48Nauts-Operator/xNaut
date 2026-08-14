import { test, expect } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.setItem('xnaut-sidebar-visible', '1');
    localStorage.setItem('xnaut-right-pane-visible', '1');
  });
  await page.goto('/?stub=1');
  await page.waitForTimeout(900);
});

test('agent library opens a bounded conversation and quick view', async ({ page }) => {
  await page.getByText('Agent Space', { exact:true }).first().click();
  await expect(page.locator('.agent-space')).toBeVisible();
  await expect(page.locator('.as-title h1')).toHaveText('Builder');
  await expect(page.locator('.as-handle')).toHaveText('@builder');
  await expect(page.getByLabel('Message @builder')).toBeVisible();
  await expect(page.getByRole('group', { name:'Agent view' })).toContainText('Turns product intent into working software.');
  await expect(page.getByRole('group', { name:'Agent view' })).toContainText('Not recorded');
});

test('agent settings and the global create menu use the Agent Space shell', async ({ page }) => {
  await page.getByText('Agent Space', { exact:true }).first().click();
  await page.getByRole('button', { name:'Settings', exact:true }).click();
  await expect(page.getByRole('heading', { name:'Agent settings.' })).toBeVisible();
  await expect(page.locator('input[name="handle"]')).toHaveValue('builder');

  await page.getByRole('button', { name:'Create' }).click();
  await expect(page.getByText('New Agent', { exact:true })).toBeVisible();
  await page.getByText('New Agent', { exact:true }).click();
  await expect(page.getByRole('heading', { name:'Create a new agent.' })).toBeVisible();
  await expect(page.locator('input[name="tagline"]')).toBeVisible();
});
