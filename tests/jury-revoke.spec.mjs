import { test, expect } from '@playwright/test';

test('a jury approval stays visible and its Revoke action reaches the backend', async ({ page }) => {
  await page.goto('/?stub=1');
  await page.waitForFunction(() => typeof window.xnautCreateMeshPanel === 'function');
  await page.evaluate(() => {
    const item = {
      id: 'in-jury-proof', at: new Date().toISOString(), project: 'XNAUT', from: 'nautbot',
      kind: 'notify', title: 'Jury approved XNAUT-930', body: 'Codex 0.9; Gemini 0.9',
      context: { jury_id: 'jury-proof', revocable: 'true' }, options: [], links: [], status: 'done',
    };
    const original = window.__TAURI__.core.invoke;
    window.__juryDecisions = [];
    window.__TAURI__.core.invoke = async (cmd, args) => {
      if (cmd === 'inbox_list') return [item];
      if (cmd === 'inbox_decide') {
        window.__juryDecisions.push(args);
        item.status = 'revoked';
        return item;
      }
      return original(cmd, args);
    };
    const parent = document.createElement('div');
    parent.id = 'jury-mesh-proof';
    Object.assign(parent.style, { position: 'fixed', inset: '0', zIndex: '99999', background: '#111' });
    document.body.appendChild(parent);
    window.xnautCreateMeshPanel('proof', parent);
  });
  const panel = page.locator('#jury-mesh-proof');
  await expect(panel.locator('.mesh-row')).toContainText('Jury approved XNAUT-930');
  await panel.locator('.mesh-row').click();
  await expect(panel.getByRole('button', { name: 'Revoke jury approval' })).toBeVisible();
  await panel.getByRole('button', { name: 'Revoke jury approval' }).click();
  expect(await page.evaluate(() => window.__juryDecisions)).toEqual([{ id: 'in-jury-proof', decision: 'revoke' }]);
  await expect(panel.locator('.mesh-row')).toHaveCount(0);
});
