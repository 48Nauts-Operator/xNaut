import { test, expect } from '@playwright/test';

// The agent's diagram lives beside the conversation. Ported from Cockpit's
// concept canvas: the agent sends the whole graph, the owner arranges it.
test('the agent canvas draws its boxes and arrows in the agent pane', async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.setItem('xnaut-sidebar-visible', '1');
    localStorage.setItem('xnaut-right-pane-visible', '1');
  });
  await page.goto('/?stub=1');
  await page.waitForTimeout(900);
  await page.getByText('Agent Space', { exact: true }).first().click();
  await page.locator('.asl-agent').first().click();

  const canvas = page.locator('.aqp-canvas .cvs');
  await expect(canvas).toBeVisible();
  await expect(canvas.locator('.cvs-title')).toHaveText('How an Agentic Loop Works');
  await expect(canvas.locator('[data-node="observe"]')).toBeVisible();
  await expect(canvas.locator('[data-node="reason"]')).toBeVisible();
  // An arrow between them, and its label.
  await expect(canvas.locator('.cvs-edge')).toHaveCount(1);
  await expect(canvas.locator('.cvs-edge-label')).toHaveText('context');
});
