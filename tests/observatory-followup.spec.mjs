import { test, expect } from '@playwright/test';

async function boot(page) {
  await page.addInitScript(() => localStorage.setItem('xnaut-sidebar-visible', '1'));
  await page.goto('/?stub=1');
  await page.waitForSelector('#btn-help');
}
test('Jev card is xNaut-only, compact, and distinguishes no calls from failed accounting', async ({ page }) => {
  await boot(page);
  await page.evaluate(() => {
    const invoke = window.__TAURI__.core.invoke;
    window.jevTest = { configured: true, month: '2026-09', calls: 0, inputTokens: 0, outputTokens: 0, costUsd: 0, unpricedCalls: 0 };
    window.__TAURI__.core.invoke = (name, args) => name === 'jev_usage' ? Promise.resolve(window.jevTest) : invoke(name, args);
    window.xnautAttachObservatoryTab();
  });
  const card = page.locator('.obs-jev-cost');
  await expect(card).toBeVisible();
  await expect(card).toContainText('No xNaut calls recorded');
  expect((await card.boundingBox()).width).toBeLessThanOrEqual(210);
  await page.evaluate(() => { window.jevTest = { ...window.jevTest, calls: 2, inputTokens: 1000000, costUsd: .042 }; return window.xnautJevCost.refresh(); });
  await expect(card).toContainText('$0.04200');
  await expect(card).toContainText('2 calls');
  await page.evaluate(() => {
    const invoke = window.__TAURI__.core.invoke;
    window.__TAURI__.core.invoke = (n,a) => n === 'jev_usage' ? Promise.reject(new Error('unreadable')) : invoke(n,a);
    return window.xnautJevCost.refresh();
  });
  await expect(card.locator('[data-jev-price]')).toHaveText('Unavailable');
});

test('adopted remote row attaches its exact run before opening a viewport, and double-clicks deduplicate', async ({ page }) => {
  await boot(page);
  const result = await page.evaluate(async () => {
    const tabs = [];
    window.__xnautStub.agent_sessions_list = [{ session_id: 'xnaut-cortana-old', agent_id: 'cortana', remote_env: 'exe-dev' }];
    window.__xnautStub.agent_remote_attach = 'pty-remote';
    window.xnautAttachAgentTab = (sid,label) => { tabs.push({sid,label}); return 'test-tab'; };
    await Promise.all([window.xnautOpenAgentSession('xnaut-cortana-old','Cortana'),window.xnautOpenAgentSession('xnaut-cortana-old','Cortana')]);
    return {calls: window.__xnautInvokes.filter(c => c.cmd === 'agent_remote_attach').map(c => c.args),tabs};
  });
  expect(result.calls).toHaveLength(1);
  expect(result.calls[0]).toMatchObject({handle:'cortana',sessionName:'xnaut-cortana-old'});
  expect(result.tabs).toEqual([{sid:'pty-remote',label:'Cortana'}]);
});

test('remote attach failure does not create an empty terminal', async ({ page }) => {
  await boot(page);
  const result = await page.evaluate(async () => {
    let created = 0, error = '';
    window.__xnautStub.agent_sessions_list = [{ session_id:'xnaut-cortana-old',agent_id:'cortana',remote_env:'exe-dev' }];
    window.__xnautStub.agent_remote_attach = { __reject: 'Remote host unavailable' };
    window.alert = text => { error = text; };
    window.xnautAttachAgentTab = () => { created++; };
    const opened = await window.xnautOpenAgentSession('xnaut-cortana-old','Cortana');
    return {created,error,opened};
  });
  expect(result.created).toBe(0); expect(result.opened).toBe(false);
  expect(result.error).toContain('Remote host unavailable');
});
