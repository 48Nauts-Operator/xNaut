// XNAUT-185: thirty concurrent agents have to be reachable and visible.
//
// Two ceilings made the scale test unpassable by arithmetic, before a single
// agent was spawned:
//
//   the swarm cap        Math.min(20, maxParallel)  -- 30 requested, 20 dispatched
//   observatory-panel.js  loom_runs_list limit 30   -- zero headroom at 30
//
// The second one is the nasty one, and it is what this file guards. The run
// store collapses by id, sorts newest-first and truncates to the limit BEFORE
// the panel filters to status === 'started', so any finished record newer than
// a live one silently pushes a running agent off the deck. At exactly 30 live
// runs and a window of 30, one stale record is enough. The deck under-reports
// and the app looks fine, which is the worst shape a monitoring bug can take.
//
// So the fixture is deliberately hostile: 30 live runs with OLDER timestamps
// than 40 finished ones. Under the old window the panel saw 30 records, all
// finished, and rendered nothing.
import { test, expect } from '@playwright/test';

const LIVE = 30;
const FINISHED = 40;

function runs() {
  const now = Date.now();
  const out = [];
  // Finished runs are the NEWEST, so a too-small window keeps these and drops
  // every live agent. Ordering is the whole point of the fixture.
  for (let i = 0; i < FINISHED; i++) {
    out.push({ id: 'done-' + i, status: 'done', provider: 'local', weave: 'finished ' + i,
      pid: 0, cwd: '/tmp/x', model: 'sonnet', started_ms: now - i * 1000 });
  }
  for (let i = 0; i < LIVE; i++) {
    out.push({ id: 'live-' + i, status: 'started', provider: 'local', weave: 'agent ' + i,
      goal: 'trivial task ' + i, pid: 1000 + i, cwd: '/tmp/wt/agent-' + i,
      model: 'sonnet', started_ms: now - 100000 - i * 1000 });
  }
  return out;
}

test('the Observatory lists all 30 live agents, with finished runs crowding the window', async ({ page }) => {
  page.on('dialog', (d) => d.dismiss().catch(() => {}));
  await page.goto('/?stub=1');
  await page.waitForFunction(() => !!window.xnautCreateObservatoryPanel);

  await page.evaluate((fixture) => {
    window.__xnautStub.loom_runs_list = fixture;
    window.__xnautStub.loom_run_alive = true;   // every live pid answers alive
    window.__xnautStub.agent_sessions_list = [];
    window.__xnautStub.zellij_live_sessions = [];
    window.__xnautStub.zellij_sessions_info = [];
    const host = document.createElement('div');
    host.id = 'scale-host';
    document.body.appendChild(host);
    return window.xnautCreateObservatoryPanel('scale', host, {});
  }, runs());

  const rows = page.locator('#scale-host .obs-row');
  await expect(rows).toHaveCount(LIVE);
  await expect(page.locator('#scale-host [data-count]')).toHaveText(LIVE + ' active');
});

test('the swarm can be asked for 30 parallel runs', async ({ page }) => {
  // The ceiling used to be a number box on the Multi-Agent Manager's toolbar.
  // That pane is gone (XNAUT-354) and the cap lives on the orchestrator's
  // profile, so this reads it off the field that now sets it. Still off the
  // RENDERED control rather than the source, so a ceiling left behind in the
  // form still fails this — and the clamp behind it is pinned in Rust, by
  // swarm_plan::tests::the_cap_bounds_the_plan_and_names_what_it_dropped.
  await page.goto('/?stub=1');
  await page.waitForTimeout(900);
  await page.getByText('Agent Space', { exact: true }).first().click();
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  const field = page.locator('input[name="max_parallel"]');
  await expect(field).toBeVisible();
  expect(Number(await field.getAttribute('max'))).toBeGreaterThanOrEqual(30);
});
