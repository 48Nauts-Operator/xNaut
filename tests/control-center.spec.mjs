import { test, expect } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  await page.goto('/?stub=1');
  await page.waitForTimeout(900);
});

test('Control Center is the lean default landing page', async ({ page }) => {
  await expect(page.locator('.control-center')).toBeVisible();
  await expect(page.locator('.cc-greeting')).toContainText(/^Good (morning|afternoon|evening)/);
  await expect(page.locator('.cc-feedback')).toHaveText('No agents running. It is quiet.');
  await expect(page.getByLabel('Ask NautBot')).toHaveAttribute('placeholder', 'Ask NautBot anything about xNaut…');
  await expect(page.getByText('nautgate · gpt-5.6-sol · high', { exact:true })).toBeVisible();
  await expect(page.getByRole('button', { name:'NautBot settings' })).toBeVisible();
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

test('open-ended NautBot conversation stays inside Control Center', async ({ page }) => {
  const tabCount = await page.locator('#tabs-container > *').count();
  const composer = page.getByLabel('Ask NautBot');
  await composer.fill('How many agents are running?');
  await composer.press('Enter');
  await expect(page.locator('.cc-conversation .chatp-pane')).toBeVisible();
  await expect(page.locator('.cc-conversation .chatp-bar')).toBeHidden();
  await expect(page.locator('.cc-conversation .chatp-input-area')).toBeHidden();
  await expect(page.locator('.cc-greeting')).toBeVisible();
  await expect(composer).toBeVisible();
  await expect(page.getByText('NautBot reply', { exact:true })).toBeVisible();
  await expect(page.locator('.control-center textarea:visible')).toHaveCount(1);
  await expect(page.locator('#tabs-container > *')).toHaveCount(tabCount);
  const request = await page.evaluate(() => window.__xnautInvokes.find((item) => item.cmd === 'chat_send_provider'));
  expect(request.args).toMatchObject({ provider:'nautgate', model:'gpt-5.6-sol', reasoningEffort:'high' });
});

test('NautBot settings are directly reachable from Control Center', async ({ page }) => {
  await page.getByRole('button', { name:'NautBot settings' }).click();
  await expect(page.getByRole('heading', { name:'Agent settings.' })).toBeVisible();
  await expect(page.locator('input[name="handle"]')).toHaveValue('nautbot');
  await expect(page.locator('select[name="model"]')).toHaveValue('gpt-5.6-sol');
  await expect(page.locator('select[name="reasoning_effort"]')).toHaveValue('high');
});

test('NautBot receives portable context left by a specialist responder', async ({ page }) => {
  await page.evaluate(() => localStorage.setItem('xnaut-portable-agent-context:v1', JSON.stringify([
    { id:'handoff-1', role:'assistant', agent:'builder', text:'The backend tests passed.', at:new Date().toISOString() },
  ])));
  const composer = page.getByLabel('Ask NautBot');
  await composer.fill('What happened?');
  await composer.press('Enter');
  const request = await page.evaluate(() => window.__xnautInvokes.find((item) => item.cmd === 'chat_send_provider'));
  expect(request.args.messages.some((message) => String(message.content).includes('@builder: The backend tests passed.'))).toBe(true);
});
