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
  await page.getByRole('button', { name: 'More surfaces' }).click();
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

  // And it STAYS closed: the agent redrawing must not reopen a pane he shut.
  await page.evaluate(() => window.__xnautEmit('canvas-changed', { key: 'builder' }));
  await page.waitForTimeout(300);
  await expect(page.locator('.as-stage.split')).toHaveCount(0);

  // The card in the thread is what brings it back.
  await page.evaluate(() => window.__xnautOpenCanvasSplit());
  await expect(page.locator('.as-stage.split')).toBeVisible();
});

test('the agent writes a document into the same split, with Preview and Code', async ({ page }) => {
  // The other half of Cockpit's artifact pane. A long answer belongs in a
  // document that can be read and saved, not scrolling past in the chat.
  await page.addInitScript(() => localStorage.setItem('xnaut-sidebar-visible', '1'));
  await page.goto('/?stub=1');
  await page.waitForTimeout(900);
  await page.getByRole('button', { name: 'More surfaces' }).click();
  await page.getByText('Agent Space', { exact: true }).first().click();
  await page.locator('.asl-agent').first().click();

  await page.evaluate(() => window.__xnautOpenDocumentSplit && window.__xnautOpenDocumentSplit());
  const doc = page.locator('.as-split .doc');
  await expect(doc).toBeVisible();
  await expect(doc.locator('.doc-title')).toHaveText('Release notes');
  // Preview renders the markdown, not the source.
  await expect(doc.locator('.doc-view h2')).toHaveText('What shipped');

  // Code shows the source and is editable.
  await doc.getByRole('button', { name: 'Code', exact: true }).click();
  await expect(doc.locator('.doc-code')).toHaveValue(/## What shipped/);

  // And it can leave the app.
  await doc.getByRole('button', { name: 'Save to vault' }).click();
  const saved = await page.evaluate(() => window.__xnautInvokes
    .filter((item) => item.cmd === 'document_save_to_vault').at(-1));
  expect(saved.args.key).toBeTruthy();
  await expect(doc.locator('.doc-saved')).toBeVisible();
});
