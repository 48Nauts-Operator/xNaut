// XNAUT-355: a NautFlow persona is an agent profile, matched by role.
//
// The roster this replaced was a localStorage table read by one consumer, and
// it drifted from the profiles that drive everything else. Two ways the new
// wiring can be quietly wrong, both asserted on values rather than pixels:
//
//   1. the stage launches with something other than the matching profile's
//      runtime and model. The launch goes through `agent_headless_command`, so
//      the handle it is asked for IS the runtime decision, and `loom_run`
//      carries the model the record shows;
//   2. a role nobody holds picks a substitute silently. It has to run as
//      NautBot AND say so on the card, or the Architect gets worse with nobody
//      noticing.
import { test, expect } from '@playwright/test';

const NAUTBOT = {
  handle: 'nautbot', display_name: 'NautBot', tagline: '', purpose: '', runtime_id: 'codex',
  provider: 'nautgate', model: 'gpt-5.6-sol', chat_model: '', reasoning_effort: 'high', execution: 'local',
  role: 'core-orchestrator', capabilities: [], notifications: true, accent_color: '#f5b840',
  default_project: null, created_at: '2026-09-13T00:00:00Z', updated_at: '2026-09-13T00:00:00Z',
};
// The Reviewer on a provider NautBot does not use, as the seed ships it.
const REVIEWER = {
  ...NAUTBOT, handle: 'reviewer', display_name: 'Reviewer', runtime_id: 'claude',
  provider: 'anthropic', model: 'claude-sonnet-5', reasoning_effort: '', role: 'reviewer', accent_color: '#6aa9ff',
};

const STUB = {
  // A Reviewer and NautBot; nobody holds the analyst or designer role.
  agent_profile_list: [NAUTBOT, REVIEWER],
  // No stage document anywhere, so the Build stage shows the Design intro card
  // rather than a drafted design.
  vault_note_read: null,
  loom_run: { pid: 4242, log: '/tmp/persona.log' },
};

const errors = (page) => page.evaluate(() => window.__xnautErrors || []);
const invokes = (page, cmd) => page.evaluate((c) => window.__xnautInvokes.filter((i) => i.cmd === c).map((i) => i.args), cmd);

async function openNautFlow(page, mode) {
  page.on('dialog', (d) => d.dismiss().catch(() => {}));
  await page.goto('/index.html?stub=1');
  await page.waitForFunction(() => typeof window.xnautCreateProjectManagementPanel === 'function');
  await page.evaluate(([stub, nfMode]) => {
    Object.assign(window.__xnautStub, stub);
    localStorage.setItem('xnaut-nf-mode:SMOKE', nfMode);
    // Validation is green by override, so the Build stage reaches the Design card.
    localStorage.setItem('xnaut-nf-valoverride:SMOKE', '1');
    const host = document.createElement('div');
    host.id = 'pm-test-host';
    document.body.appendChild(host);
    window.xnautCreateProjectManagementPanel('pm-test', host, {});
  }, [STUB, mode]);
  const pane = page.locator('#pm-test-host .pmw');
  await pane.locator('[data-project]:not([data-project=""])').first().click();
  await pane.locator('[data-project-section="nautflow"]').click();
  return pane;
}

test('a stage launches with the profile whose role matches, and the card names it', async ({ page }) => {
  const pane = await openNautFlow(page, 'expert');

  // Idea is the Analyst's stage. Nobody holds that role: the card says who
  // actually runs, before anything is clicked.
  await pane.locator('[data-flow-stage="idea"]').first().click();
  await expect(pane.locator('.pmw-stage-agent')).toHaveText('Analyst · running as NautBot: no analyst profile');

  // Test and review is the Reviewer's stage. The card names the profile, its
  // provider and its model: what the launch will actually use.
  await pane.locator('[data-flow-stage="test_review"]').first().click();
  const badge = pane.locator('.pmw-stage-agent');
  await expect(badge).toHaveText('Reviewer · @reviewer · anthropic · claude-sonnet-5');

  await pane.locator('.pmw-ask-agent').click();
  await page.waitForTimeout(600);
  const asked = await invokes(page, 'agent_headless_command');
  expect(asked.length, 'the launch did not go through agent_headless_command').toBe(1);
  expect(asked[0].handle, 'the launcher was not asked for the Reviewer profile').toBe('reviewer');
  expect(asked[0].isolateMcp).toBe(true);
  const runs = await invokes(page, 'loom_run');
  expect(runs.length).toBe(1);
  expect(runs[0].model, 'the run record carries a model the profile does not').toBe('claude-sonnet-5');
  expect(runs[0].runId).toMatch(/^persona-reviewer-/);
  // Nothing is written back: the profiles are the store now.
  const legacy = await page.evaluate(() => Object.keys(localStorage).filter((k) => k.startsWith('xnaut-nf-model:') || k.startsWith('xnaut-agent-roster')));
  expect(legacy).toEqual([]);

  // Changing the profile changes the next resolution; nothing is cached from
  // the last launch.
  await page.evaluate(() => { window.__xnautStub.agent_profile_list[1].model = 'claude-opus-5'; });
  await pane.locator('[data-flow-stage="idea"]').first().click();
  await pane.locator('[data-flow-stage="test_review"]').first().click();
  await expect(pane.locator('.pmw-stage-agent')).toHaveText('Reviewer · @reviewer · anthropic · claude-opus-5');

  expect(await errors(page)).toEqual([]);
});

test('the Designer with no profile runs as NautBot and the card says so', async ({ page }) => {
  const pane = await openNautFlow(page, 'guided');

  await pane.locator('[data-flow-stage="build"]').first().click();
  const card = pane.locator('.pmw-build-term .pmw-stage-agent');
  await expect(card).toHaveText('Designer · running as NautBot: no designer profile');
  await expect(pane.locator('.pmw-dsg-draft')).toHaveText('🎨 Draft the screens');

  await pane.locator('.pmw-dsg-draft').click();
  await page.waitForTimeout(600);
  const asked = await invokes(page, 'agent_headless_command');
  expect(asked.length).toBe(1);
  expect(asked[0].handle, 'the fallback did not launch as NautBot').toBe('nautbot');
  const runs = await invokes(page, 'loom_run');
  expect(runs.length).toBe(1);
  expect(runs[0].model).toBe('gpt-5.6-sol');
  expect(runs[0].runId).toMatch(/^persona-designer-/);

  expect(await errors(page)).toEqual([]);
});
