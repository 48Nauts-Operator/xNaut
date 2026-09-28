// XNAUT-153. Dispatch is only worth a button if it reaches the backend with the
// ticket it was pressed on and reports back which branch the agent went to.
import { test, expect } from '@playwright/test';

const OK = {
  enabled: true, configured: true, valid: true, repo_path: '/tmp/smoke-control',
  remote_url: '', git_repository: true, project_count: 1, ticket_count: 1,
  error: '', warning: '', branch: 'main', last_commit: '', dirty: false, ahead: 0, behind: 0,
};

test('dispatching a ticket launches its owner and names the branch', async ({ page }) => {
  page.on('dialog', (d) => d.dismiss().catch(() => {}));
  await page.goto('/index.html?stub=1');
  await page.waitForFunction(() => typeof window.xnautCreateProjectManagementPanel === 'function');
  await page.evaluate((s) => {
    window.__xnautStub.pm_module_status = s;
    const host = document.createElement('div');
    host.id = 'pm-test-host';
    document.body.appendChild(host);
    window.xnautCreateProjectManagementPanel('pm-test', host, {});
  }, OK);

  const pane = page.locator('#pm-test-host .pmw');
  await pane.locator('[data-project]:not([data-project=""])').first().click();
  await pane.locator('[data-project-section="work"]').first().click();
  await pane.locator('[data-id="SMOKE-1"]').first().click();

  const dispatch = pane.locator('.pmw-dispatch');
  await expect(dispatch, 'no Dispatch button on the ticket').toBeVisible();
  await dispatch.click();

  // The branch the backend reported, echoed where Verify puts its own state.
  await expect(pane.locator('.pmw-verify-state')).toContainText('agent/builder/smoke-1');

  const call = await page.evaluate(() => window.__xnautInvokes.filter((i) => i.cmd === 'pm_ticket_dispatch').pop());
  expect(call, 'Dispatch never reached the backend').not.toBeNull();
  expect(call.args.ticketId).toBe('SMOKE-1');
  expect(call.args.project).toBe('SMOKE');
});
