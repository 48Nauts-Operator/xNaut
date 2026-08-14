import { test, expect } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  await page.goto('/?stub=1');
  await page.waitForTimeout(900);
});

test('Control Center is the lean default landing page', async ({ page }) => {
  await expect(page.locator('.control-center')).toBeVisible();
  await expect(page.locator('.cc-greeting')).toContainText(/^Good (morning|afternoon|evening)/);
  await expect(page.locator('.cc-feedback')).toHaveText('No agents running. It is quiet.');
  await expect(page.getByLabel('Ask NautBot')).toHaveAttribute('placeholder', 'Ask NautBot, or message any agent…');
  await expect(page.locator('.cc-actions .cc-action')).toHaveCount(3);
  await expect(page.getByText('Needs attention', { exact:true })).toHaveCount(0);
});

test('Control Center quick actions route into existing product surfaces', async ({ page }) => {
  await page.getByRole('button', { name:'Create an agent' }).click();
  await expect(page.getByRole('heading', { name:'Create a new agent.' })).toBeVisible();
  await page.getByText('Control Center', { exact:true }).first().click();
  await page.getByRole('button', { name:'Open Observatory' }).click();
  await expect(page.locator('.obs')).toBeVisible();
});

test('conversational create command opens the New Agent flow', async ({ page }) => {
  const composer = page.getByLabel('Ask NautBot');
  await composer.fill('Create an agent');
  await composer.press('Enter');
  await expect(page.getByRole('heading', { name:'Create a new agent.' })).toBeVisible();
});
