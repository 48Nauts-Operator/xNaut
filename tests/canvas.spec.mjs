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

  // The canvas is a split of the MAIN stage, beside the conversation — not in
  // the narrow right rail.
  await expect(page.locator('.as-stage.split')).toBeVisible();
  const canvas = page.locator('.as-split .cvs');
  await expect(canvas).toBeVisible();
  await expect(canvas.locator('.cvs-title')).toHaveText('How an Agentic Loop Works');
  await expect(canvas.locator('[data-node="observe"]')).toBeVisible();
  await expect(canvas.locator('[data-node="reason"]')).toBeVisible();
  // An arrow between them, and its label.
  await expect(canvas.locator('.cvs-edge')).toHaveCount(1);
  await expect(canvas.locator('.cvs-edge-label')).toHaveText('context');

  // Full screen hands the whole stage to the canvas, and comes back.
  await canvas.getByRole('button', { name:'Full screen' }).click();
  await expect(page.locator('.as-stage.split-full')).toBeVisible();
  await canvas.getByRole('button', { name:'Full screen' }).click();
  await expect(page.locator('.as-stage.split-full')).toHaveCount(0);

  // Closing it gives the conversation the whole stage back.
  await canvas.getByRole('button', { name:'Close canvas' }).click();
  await expect(page.locator('.as-stage.split')).toHaveCount(0);
});
