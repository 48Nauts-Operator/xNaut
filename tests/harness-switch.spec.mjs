import { test, expect } from '@playwright/test';

// XNAUT-150. The switch is only worth anything if the next run actually goes
// out under the new binary AND carries the conversation. Both are asserted.
test('switching harness persists the runtime and carries history into ticket-aware dispatch', async ({ page }) => {
  await page.goto('/?stub=1');
  await page.waitForTimeout(900);
  await page.getByRole('button', { name: 'More surfaces' }).click();
  await page.getByText('Agent Space', { exact: true }).first().click();
  await page.locator('.asl-agent', { hasText: 'Builder' }).first().click();
  await expect(page.locator('.as-title h1')).toHaveText('Builder');

  // Builder's profile runtime is codex, so that is what the picker starts on.
  const harness = page.getByLabel('Harness for this thread');
  await expect(harness).toHaveValue('codex');

  // Ticket-aware dispatch reads the selected runtime from the persisted thread,
  // not a model-supplied runtime and not an untracked frontend launch.
  await page.getByLabel('Message @builder').fill('Wire the login form');
  await page.getByLabel('Message @builder').press('Enter');
  await expect(page.getByText('The release is tagged and the cask is on 1.15.0.',{exact:true})).toBeVisible();
  await harness.selectOption('claude');
  await expect(page.getByText('Harness switched to')).toBeVisible();
  await page.getByLabel('Message @builder').fill('Now add the reset link');
  await page.getByLabel('Message @builder').press('Enter');
  await expect.poll(()=>page.evaluate(()=>window.__xnautInvokes.filter(i=>i.cmd==='agent_chat_turn').length)).toBe(2);
  const state=await page.evaluate(()=>{
    const request=window.__xnautInvokes.filter(i=>i.cmd==='agent_chat_turn').at(-1).args;
    const saved=JSON.parse(window.xnautConversationStorage.getItem('xnaut-agent-threads:v1'));
    return {request,thread:saved.builder.find(t=>t.id===request.threadId),bareLaunch:window.__xnautInvokes.some(i=>i.cmd==='agent_profile_launch')};
  });
  expect(state.thread.runtime_id).toBe('claude');
  expect(state.thread.conversation_id).toBeNull();
  expect(state.request.messages.some(m=>m.content.includes('Wire the login form'))).toBe(true);
  expect(state.bareLaunch).toBe(false);
});
