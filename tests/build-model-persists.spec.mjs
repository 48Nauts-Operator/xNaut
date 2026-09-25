// The Build stage model selection must survive a re-render.
//
// Reported by André while testing 1.14.0: he switched the Build model away from
// Opus, and "when I try to go back it switches back to opus right away".
//
// It was not the switch failing. `buildModelOpts` wrote `selected` onto
// claude-opus-5 unconditionally on every paint, so the choice was correct until
// something re-rendered the panel — and the periodic refresh does that on its
// own every 15 seconds. The per-stage dropdown built three lines below already
// persisted through localStorage; the Build one never did.
import { test, expect } from '@playwright/test';

const OK = {
  enabled: true, configured: true, valid: true, repo_path: '/tmp/smoke-control',
  remote_url: '', git_repository: true, project_count: 1, ticket_count: 0,
  error: '', warning: '', branch: 'main', last_commit: '', dirty: false, ahead: 0, behind: 0,
};

async function openBuildStage(page) {
  await page.evaluate((s) => {
    window.__xnautStub.pm_module_status = s;
    document.querySelectorAll('#pm-test-host').forEach((n) => n.remove());
    const host = document.createElement('div');
    host.id = 'pm-test-host';
    document.body.appendChild(host);
    window.xnautCreateProjectManagementPanel('pm-test', host, { project: 'SMOKE', section: 'nautflow' });
  }, OK);

  const pane = page.locator('#pm-test-host .pmw');
  await pane.locator('[data-flow-stage="build"]').click();
  await expect(pane.locator('.pmw-build-model')).toBeVisible();
  return pane;
}

test.beforeEach(async ({ page }) => {
  page.on('dialog', (d) => d.dismiss().catch(() => {}));
  await page.goto('/index.html?stub=1');
  await page.waitForFunction(() => typeof window.xnautCreateProjectManagementPanel === 'function');
});

test('the Build model defaults to Opus 5', async ({ page }) => {
  const pane = await openBuildStage(page);
  await expect(pane.locator('.pmw-build-model')).toHaveValue('claude-opus-5');
});

test('a chosen Build model survives a re-render', async ({ page }) => {
  let pane = await openBuildStage(page);

  await pane.locator('.pmw-build-model').selectOption('claude-fable-5');
  await expect(pane.locator('.pmw-build-model')).toHaveValue('claude-fable-5');

  // Repaint the panel from scratch, which is what the periodic refresh does.
  pane = await openBuildStage(page);

  await expect(pane.locator('.pmw-build-model'),
    'the Build model snapped back instead of keeping the choice').toHaveValue('claude-fable-5');
});

test('a stored model the catalogue no longer offers falls back rather than sticking', async ({ page }) => {
  await page.evaluate(() => localStorage.setItem('xnaut-nf-buildmodel:SMOKE', 'claude-retired-3'));
  const pane = await openBuildStage(page);

  // A select whose value is not among its options reads as empty, and the Build
  // button would then launch with no model at all.
  await expect(pane.locator('.pmw-build-model')).toHaveValue('claude-opus-5');
});
