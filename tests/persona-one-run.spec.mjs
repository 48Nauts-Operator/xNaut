// One persona run per click, even when the clicks are a heartbeat apart.
//
// XNAUT-148: André clicked "Draft the screens" three times on v1.14.0 and got no
// design at all. The one-run guard read nfStopCurrent, which nfDriveRun only sets
// AFTER two awaits and a spawn, so a second click inside that window passed the
// guard and started a second `claude -p`. The two runs then fought over the same
// .loom-goal.txt and the interrupted one wrote nothing.
//
// Driven through the Expert-mode stage button rather than the design card: it is
// the same guard, three clicks deep instead of a validation-green precondition.
import { test, expect } from '@playwright/test';

const OK = {
  enabled: true, configured: true, valid: true, repo_path: '/tmp/smoke-control',
  remote_url: '', git_repository: true, project_count: 1, ticket_count: 0,
  error: '', warning: '', branch: 'main', last_commit: '', dirty: false, ahead: 0, behind: 0,
};

test('two fast clicks start one persona run, not two', async ({ page }) => {
  page.on('dialog', (d) => d.dismiss().catch(() => {}));
  await page.goto('/index.html?stub=1');
  await page.waitForFunction(() => typeof window.xnautCreateProjectManagementPanel === 'function');

  // Real IPC takes milliseconds. The stub resolves in a microtask, which closes
  // the very window this test exists to open, so vault_init is slowed to 400ms.
  await page.evaluate(() => {
    const orig = window.__TAURI__.core.invoke;
    window.__TAURI__.core.invoke = (cmd, args) => (cmd === 'vault_init'
      ? new Promise((r) => setTimeout(() => r(orig(cmd, args)), 400))
      : orig(cmd, args));
    localStorage.setItem('xnaut-nf-mode:SMOKE', 'expert');
    const host = document.createElement('div');
    host.id = 'pm-test-host';
    document.body.appendChild(host);
    window.xnautCreateProjectManagementPanel('pm-test', host, {});
  });

  const pane = page.locator('#pm-test-host .pmw');
  await pane.locator('[data-project]:not([data-project=""])').first().click();
  await pane.locator('[data-project-section="nautflow"]').click();
  await pane.locator('[data-flow-stage="idea"]').first().click();
  const ask = pane.locator('.pmw-ask-agent');
  await expect(ask).toBeVisible();

  await ask.click();
  await ask.click({ force: true }); // inside the 400ms vault_init window

  await page.waitForTimeout(1200);
  const spawns = await page.evaluate(() => window.__xnautInvokes.filter((i) => i.cmd === 'loom_run').length);
  expect(spawns, 'the second click spawned a competing run').toBe(1);
  const script = await page.evaluate(() => window.__xnautInvokes.find((i) => i.cmd === 'loom_run').args.script);
  expect(script).toContain('--settings');
  expect(script).toContain('"disableAllHooks":true');
  expect(script).toContain('$XNAUT_VETO_URL$XNAUT_HOOK_TOKEN');
  expect(script).toContain('--strict-mcp-config');
});
