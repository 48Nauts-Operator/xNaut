// XNAUT-354. NautBot offers the swarm; the Multi-Agent Manager pane is gone.
//
// Three of the four acceptance criteria are frontend properties and are here:
//   - a multi-ticket request shows a plan CARD and dispatches nothing until it
//     is confirmed;
//   - every run a confirmed swarm starts is in the registry, and the
//     Observatory reads it from there, under its project;
//   - nothing in the nav or the rail reaches the deleted pane.
//
// The fourth — a single-ticket request gets no swarm question — is decided in
// Rust, before anything could reach a card, and is pinned there by
// swarm_plan::tests::one_runnable_ticket_is_a_dispatch_and_never_a_swarm_question.
// Asserting the ABSENCE of a card here would pass for any reason at all,
// including a broken listener.
import { test, expect } from '@playwright/test';

// The shape swarm_plan returns and agent_profiles emits. Two runs, so it is a
// swarm, and one refusal, because the reasons are half of what the card is for.
const PLAN = {
  id: 'swarm-abc12345',
  project: 'SMOKE',
  max_parallel: 3,
  created_at: 1757793600000,
  runs: [
    { ticket: 'SMOKE-1', title: 'First project ticket', owner: 'builder', model: 'gpt-5.6-codex', branch: 'agent/builder/smoke-1', environment: 'exe-dev' },
    { ticket: 'SMOKE-2', title: 'Second project ticket', owner: 'codex', model: 'gpt-5.6-codex', branch: 'agent/codex/smoke-2', environment: 'gitvm' },
  ],
  skipped: [{ ticket: 'SMOKE-9', reason: 'not a ticket the PM has' }],
};

// Through the app's own entry point rather than the sidebar. "Agent Space" is
// behind the More menu since the rail fold (XNAUT-337/342), which is why
// agent-space.spec.mjs's own `getByText('Agent Space')` currently times out —
// a separate, pre-existing break that this ticket does not own and must not
// hide behind. xnautOpenAgentSpace is what every caller in the app uses.
async function openNautbot(page) {
  await page.goto('/?stub=1');
  await page.waitForTimeout(900);
  await page.evaluate(() => window.xnautOpenAgentSpace('nautbot'));
  await expect(page.locator('.as-title h1')).toHaveText('NautBot');
}

test('a swarm plan arrives as a card and dispatches nothing until it is confirmed', async ({ page }) => {
  await openNautbot(page);
  await page.evaluate(() => { window.__xnautInvokes.length = 0; });
  await page.evaluate((plan) => window.__xnautEmit('swarm-plan-proposed', { agent_id: 'nautbot', plan }), PLAN);

  const card = page.locator('.as-swarm');
  await expect(card).toBeVisible();
  await expect(card.locator('.as-swarm-head b')).toHaveText('Swarm plan · SMOKE');
  await expect(card.locator('.as-swarm-run')).toHaveCount(2);
  await expect(card.locator('.as-swarm-run .id').first()).toHaveText('SMOKE-1');
  await expect(card.locator('.as-swarm-run .who').first()).toContainText('@builder');
  await expect(card.locator('.as-swarm-run .who').first()).toContainText('exe-dev');
  await expect(card.locator('.as-swarm-run .who').last()).toContainText('gitvm');
  // Only tickets the PM has. The one that was asked for and is not here says
  // why, rather than vanishing.
  await expect(card.locator('.as-swarm-skipped')).toContainText('SMOKE-9 — not a ticket the PM has');

  // Nothing has started. This is the criterion, and it is the whole reason the
  // card exists rather than a sentence.
  expect(await page.evaluate(() => window.__xnautInvokes.map((i) => i.cmd)))
    .not.toContain('swarm_plan_dispatch');
  expect(await page.evaluate(() => window.__xnautInvokes.map((i) => i.cmd)))
    .not.toContain('pm_ticket_dispatch');

  await card.getByRole('button', { name: 'Dispatch 2 agents' }).click();
  // The plan ID is what starts the batch — the backend persists its approval and members, so
  // repeated confirmation can recover the same group without another swarm.
  const sent = await page.evaluate(() => window.__xnautInvokes.find((i) => i.cmd === 'swarm_plan_dispatch'));
  expect(sent.args).toMatchObject({ planId: 'swarm-abc12345' });
  // The outcome lands on the card, which stays in the thread, not in a toast
  // that is gone before anyone reads it.
  await expect(card).toContainText('Started 2; queued 0; blocked 0');
  await expect(card.getByRole('button', { name: /Dispatch/ })).toHaveCount(0);
  expect(await page.evaluate(() => window.__xnautErrors)).toEqual([]);
});

test('a local plan explains why this workstation cannot execute it', async ({ page }) => {
  await openNautbot(page);
  await page.evaluate(() => { window.__xnautInvokes.length = 0; });
  await page.evaluate((plan) => window.__xnautEmit('swarm-plan-proposed', { agent_id: 'nautbot', plan }),
    { ...PLAN, dispatch_hold: 'This plan requires local execution. Choose exe.dev or GitVM to run approved work remotely from this workstation.' });
  await expect(page.locator('.as-swarm')).toContainText('Choose exe.dev or GitVM');
  expect(await page.evaluate(() => window.__xnautInvokes.some((i) => i.cmd === 'swarm_plan_dispatch'))).toBe(false);
});

for (const startedCount of [0, 3]) {
  test(`five-ticket swarm keeps ${5 - startedCount} queued tickets visible after confirmation and reload`, async ({ page }) => {
    await openNautbot(page);
    const plan = { ...PLAN, id: 'swarm-five', skipped: [], runs: Array.from({ length: 5 }, (_, i) =>
      ({ ...PLAN.runs[0], ticket: `SMOKE-${i + 1}`, environment: 'exe-dev' })) };
    await page.evaluate(({ plan, startedCount }) => {
      window.__xnautInvokes.length = 0;
      window.__xnautStub.swarm_plan_dispatch = {
        plan_id: plan.id, project: plan.project, total: 5,
        started: plan.runs.slice(0, startedCount),
        queued: plan.runs.slice(startedCount).map(r => r.ticket), failed: [],
      };
      window.__xnautEmit('swarm-plan-proposed', { agent_id: 'nautbot', plan });
    }, { plan, startedCount });
    const card = page.locator('.as-swarm');
    await card.getByRole('button', { name: 'Dispatch 5 agents' }).click();
    await expect(card).toContainText(`Started ${startedCount}; queued ${5 - startedCount}; blocked 0`);
    await expect(card.locator('.as-swarm-run')).toHaveCount(5);
    expect(await page.evaluate(() => window.__xnautInvokes.filter(i => i.cmd === 'swarm_plan_dispatch').length)).toBe(1);
    await openNautbot(page);
    await expect(page.locator('[data-swarm]').last()).toContainText(`Started ${startedCount}; queued ${5 - startedCount}; blocked 0`);
    expect(await page.evaluate(() => window.__xnautInvokes.some(i => i.cmd === 'swarm_plan_dispatch'))).toBe(false);
  });
}

test('dispatch refusal is shown on the plan without claiming that workers started', async ({ page }) => {
  await openNautbot(page);
  await page.evaluate(plan => {
    window.__xnautStub.swarm_plan_dispatch = { __reject: 'Repository changed since approval; review the updated plan.' };
    window.__xnautEmit('swarm-plan-proposed', { agent_id: 'nautbot', plan });
  }, PLAN);
  const card = page.locator('.as-swarm');
  await card.getByRole('button', { name: 'Dispatch 2 agents' }).click();
  await expect(card).toContainText('Not dispatched: Repository changed since approval');
  await expect(card).not.toContainText('Watch them in the Observatory');
  await expect(card.locator('.as-swarm-run')).toHaveCount(2);
});

test('the Observatory reads dispatched runs from the registry, grouped by project', async ({ page }) => {
  await page.goto('/?stub=1');
  await page.waitForFunction(() => !!window.xnautCreateObservatoryPanel);
  await page.evaluate(() => {
    const host = document.createElement('div');
    host.id = 'swarm-host';
    document.body.appendChild(host);
    return window.xnautCreateObservatoryPanel('swarm', host, {});
  });

  const band = page.locator('#swarm-host [data-swarm-pills]');
  await expect(band.locator('.obs-pill-group > b')).toHaveText(['Smoke Test']);
  await expect(band.locator('.obs-pill')).toHaveCount(2);
  await expect(band).toContainText('SMOKE-1');
  await expect(band).toContainText('SMOKE-2');
  // The registry row with no ticket is a session somebody opened. The band is
  // for dispatched work, so it must not be claimed here.
  await expect(band.locator('.obs-pill.running')).toHaveCount(1);
  await expect(band.locator('.obs-pill.done')).toHaveCount(1);
  await expect(page.locator('#swarm-host [data-swarm-counts]')).toContainText('1 running');

  // Read from the registry, not from a pane's memory: there is no swarm global
  // left to publish one, and this band has to survive a reload without one.
  expect(await page.evaluate(() => window.__xnautInvokes.some((i) => i.cmd === 'run_registry_list'))).toBe(true);
  expect(await page.evaluate(() => typeof window.xnautSwarm)).toBe('undefined');
});

test('nothing reaches the retired Multi-Agent pane', async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.setItem('xnaut-right-pane-visible', '1');
  });
  await page.goto('/?stub=1');
  await page.waitForTimeout(900);
  // No rail entry, no registered view, no button anywhere that opens it.
  await expect(page.locator('[data-rpane-view="multiagent"]')).toHaveCount(0);
  await expect(page.locator('[data-multiagent]')).toHaveCount(0);
  await expect(page.getByText('Initialize Multi-Agent')).toHaveCount(0);
  // And asking for the view by name does not resurrect it.
  const shown = await page.evaluate(() => {
    try { window.xnautRightPaneShow('multiagent'); } catch (_) { /* refusing is fine */ }
    return !!document.querySelector('.mag');
  });
  expect(shown).toBe(false);
});
