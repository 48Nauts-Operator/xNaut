import { test, expect } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.setItem('xnaut-sidebar-visible', '1');
    localStorage.setItem('xnaut-right-pane-visible', '1');
  });
  await page.goto('/?stub=1');
  await page.waitForTimeout(900);
});

test('Agent Space owns the second-left library and bounded agent threads', async ({ page }) => {
  await page.getByText('Agent Space', { exact:true }).first().click();
  await expect(page.locator('.agent-space')).toBeVisible();
  await expect(page.getByRole('complementary', { name:'Agent Library' })).toBeVisible();
  await expect(page.locator('.asl-agent')).toHaveCount(1);
  await expect(page.locator('.sbar-agent')).toHaveCount(0);
  await expect(page.locator('.as-title h1')).toHaveText('Builder');
  await expect(page.locator('.as-handle')).toHaveText('@builder');
  await expect(page.getByLabel('Message @builder')).toBeVisible();
  await expect(page.locator('[data-rpane-view="agent"]')).toHaveCount(0);
  await expect(page.locator('[data-rpane-view="chat"]')).toHaveCount(0);
});

test('agent settings and the global create menu use the Agent Space shell', async ({ page }) => {
  await page.getByText('Agent Space', { exact:true }).first().click();
  await page.getByRole('button', { name:'Settings', exact:true }).click();
  await expect(page.getByRole('heading', { name:'Agent settings.' })).toBeVisible();
  await expect(page.locator('input[name="handle"]')).toHaveValue('builder');

  await page.getByRole('button', { name:'New terminal' }).click();
  const createMenu = page.locator('#new-tab-menu');
  await expect(createMenu.locator('button').first()).toContainText('New Agent');
  await createMenu.getByText('New Agent', { exact:true }).click();
  await expect(page.getByRole('heading', { name:'Create a new agent.' })).toBeVisible();
  await expect(page.locator('input[name="tagline"]')).toBeVisible();
});

test('sending a message uses the backend snake_case launch contract', async ({ page }) => {
  await page.getByText('Agent Space', { exact:true }).first().click();
  const composer = page.getByLabel('Message @builder');
  await composer.fill('Run the checks');
  await composer.press('Enter');
  await expect(page.getByText('Working…', { exact:true })).toBeVisible();
  await expect(page.getByRole('button', { name:'Open terminal' })).toBeVisible();
  const launch = await page.evaluate(() => window.__xnautInvokes.find((item) => item.cmd === 'agent_profile_launch'));
  expect(launch.args.req).toMatchObject({ handle:'builder', worktree_path:'/tmp/smoke' });
  expect(launch.args.req.prompt).toContain('Run the checks');
  expect(launch.args.req).not.toHaveProperty('worktreePath');

  await page.evaluate(() => {
    window.__xnautEmit('terminal-output:smoke-agent', { data:btoa('Run the checks\r\nChecks passed.\r\n') });
    window.__xnautEmit('agent-status-changed', { session_id:'smoke-agent', status:'idle' });
  });
  await expect(page.getByText('Checks passed.', { exact:true })).toBeVisible();
});

test('terminal inspection explicitly switches to the attached background session', async ({ page }) => {
  await page.getByText('Agent Space', { exact:true }).first().click();
  await page.getByLabel('Message @builder').fill('Run the checks');
  await page.getByLabel('Message @builder').press('Enter');
  await page.getByRole('button', { name:'Open terminal' }).click();
  await expect(page.locator('.terminal-output')).toBeVisible();
  await expect(page.locator('.tab.active')).toHaveAttribute('data-agent-session-id', 'smoke-agent');
});

test('a specialist receives the current Control Center conversation on handoff', async ({ page }) => {
  await page.evaluate(() => window.xnautSetChatHistory('control-center:nautbot', [
    { role:'user', content:'The release target is Friday.' },
    { role:'assistant', content:'I will keep Friday as the release target.' },
  ]));
  await page.getByText('Agent Space', { exact:true }).first().click();
  await page.getByLabel('Message @builder').fill('Continue with the release work');
  await page.getByLabel('Message @builder').press('Enter');
  const prompt = await page.evaluate(() => window.__xnautInvokes.find((item) => item.cmd === 'agent_profile_launch').args.req.prompt);
  expect(prompt).toContain('PORTABLE XNAUT CONVERSATION HANDOFF');
  expect(prompt).toContain('The release target is Friday.');
  expect(prompt).toContain('Active project/worktree: /tmp/smoke');
  expect(prompt).toContain('LATEST USER REQUEST\nContinue with the release work');
});
