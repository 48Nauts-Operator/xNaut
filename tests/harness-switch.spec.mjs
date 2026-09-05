import { test, expect } from '@playwright/test';

// XNAUT-150. The switch is only worth anything if the next run actually goes
// out under the new binary AND carries the conversation. Both are asserted.
test('switching harness re-launches under the new runtime and carries the thread', async ({ page }) => {
  await page.goto('/?stub=1');
  await page.waitForTimeout(900);
  await page.getByText('Agent Space', { exact: true }).first().click();
  await page.locator('.asl-agent', { hasText: 'Builder' }).first().click();
  await expect(page.locator('.as-title h1')).toHaveText('Builder');

  // Builder's profile runtime is codex, so that is what the picker starts on.
  const harness = page.locator('.as-harness');
  await expect(harness).toHaveValue('codex');

  // One run under the profile's own harness, to have a transcript to carry.
  await page.evaluate(() => { window.__xnautStub.agent_chat_turn = 'BUILD-REQUEST\nDoing the work.'; });
  await page.getByLabel('Message @builder').fill('Wire the login form');
  await page.getByLabel('Message @builder').press('Enter');
  const card = page.locator('.as-build');
  await expect(card).toBeVisible();
  await card.locator('[data-build-path]').fill('/tmp/smoke');
  await card.getByRole('button', { name: 'Open worktree & build' }).click();
  await expect(page.getByText('Working…', { exact: true })).toBeVisible();

  const first = await page.evaluate(() => window.__xnautInvokes.filter((i) => i.cmd === 'agent_profile_launch').pop());
  expect(first.args.req.runtime_id).toBe('codex');

  await harness.selectOption('claude');
  await expect(page.getByText('Harness switched to')).toBeVisible();

  // No second WHERE handshake: the worktree is already resolved for this
  // thread, so the send launches straight away.
  await page.getByLabel('Message @builder').fill('Now add the reset link');
  await page.getByLabel('Message @builder').press('Enter');

  const second = await page.evaluate(async () => {
    for (let i = 0; i < 40; i += 1) {
      const all = window.__xnautInvokes.filter((x) => x.cmd === 'agent_profile_launch');
      if (all.length > 1) return all.pop();
      await new Promise((r) => setTimeout(r, 100));
    }
    return null;
  });
  expect(second, 'the second send never launched').not.toBeNull();
  expect(second.args.req.runtime_id, 'still launching the old harness').toBe('claude');
  // Same workdir, no stale conversation id, and the earlier turn travels along.
  expect(second.args.req.worktree_path).toBe(first.args.req.worktree_path);
  expect(second.args.req.conversation_id).toBeNull();
  expect(second.args.req.prompt).toContain('PORTABLE XNAUT CONVERSATION HANDOFF');
  expect(second.args.req.prompt).toContain('Wire the login form');
});
