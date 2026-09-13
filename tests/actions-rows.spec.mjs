// The Actions list in the agent quick pane used to be five spans of dead text.
// Every assertion here is about a row LEADING somewhere, or about thirty-four
// copies of one fact being one row, because both failures are silent: a span
// that is not a button looks identical to a button nobody has clicked, and a
// list that repeats itself looks like a busy fleet.
import { test, expect } from '@playwright/test';

const at = (secondsAgo) => new Date(Date.now() - secondsAgo * 1000).toISOString();

// Thirty consecutive replays of one launch, a second apart: the shape André saw
// on 2026-09-11, when one fact filled the list thirty-four times in a second.
const REPLAYS = Array.from({ length: 30 }, (_, index) => ({
  run_id: 'run-replay', at: at(200 - index), kind: 'launch_not_durable', agent: 'claude',
  ticket: 'SMOKE-2', detail: 'relaunched: the process did not survive', session: '',
}));

const LEDGER = [
  { run_id: 'run-9', at: at(400), kind: 'registry_failed', agent: 'claude', ticket: 'SMOKE-1',
    detail: 'admission failed: read_only kill-switch engaged', session: 'sess-live' },
  // A session the harness no longer has: the row must not offer to attach it.
  { run_id: 'run-9', at: at(390), kind: 'adopted', agent: 'claude', ticket: 'SMOKE-1',
    detail: 'adopted a running writer', session: 'sess-gone' },
  { run_id: 'run-9', at: at(380), kind: 'sweep_refused', agent: 'claude', ticket: 'SMOKE-1',
    detail: 'nothing to sweep', session: '' },
  { run_id: '', at: at(300), kind: 'dispatched', agent: 'nautbot', ticket: 'SMOKE-3',
    detail: 'Start with SMOKE-3', session: '' },
  ...REPLAYS,
];

// The real RunDetail shape (src-tauri/src/run_control.rs: RunManifest, RunState
// and RunKind serialize snake_case; ticket, model and waiting_on are Options).
const RUN_DETAIL = {
  run: {
    schema_version: 1, run_id: 'run-9', kind: 'agent', ticket: 'SMOKE-1', project: 'SMOKE',
    machine: 'cand0riacstudio', agent_handle: 'claude', runtime_id: 'claude', model: 'claude-opus-5',
    worktree_path: '/tmp/smoke/.worktrees/smoke-1', branch: 'agent/claude/smoke-1',
    pty_session: 'sess-live', zellij_session: null, output_path: '/tmp/logs/run-9.log', remote_env: null,
    owner_pid: 4242, pid: 4243, process_birth: null, state: 'blocked', previous_run_id: null,
    next_run_id: null, retirement: null, undead_notified: false, admission_refused: true,
    started_at: 1, last_seen_at: 2, last_progress_at: 3, last_hook_at: null,
    waiting_on: 'the read_only kill-switch', capture_bytes: 2048,
    last_commit: 'abc1234', last_signal: 'admission_refused', ticket_returned: false, revision: 7,
  },
  capture_tail: 'admission failed: read_only kill-switch engaged\nnothing further was attempted',
  capture_note: '',
};

const STUB = {
  ledger_recent: LEDGER,
  agent_sessions_list: [{ session_id: 'sess-live', agent_id: 'builder', status: 'running', started_at_ms: 1 }],
  sandbox_verify_records: [],
  run_detail: RUN_DETAIL,
};

// Same route into the pane as exe-computer.spec.mjs: the sidebar, the agent
// list, a real agent. Nothing here reaches in and calls the view directly.
async function openActions(page) {
  await page.addInitScript(() => {
    localStorage.setItem('xnaut-sidebar-visible', '1');
    localStorage.setItem('xnaut-right-pane-visible', '1');
  });
  await page.goto('/?stub=1');
  await page.waitForTimeout(900);
  await page.evaluate((stub) => { Object.assign(window.__xnautStub, stub); }, STUB);
  await page.getByRole('button', { name: 'More surfaces' }).click();
  await page.getByText('Agent Space', { exact: true }).first().click();
  await page.locator('.asl-agent', { hasText: 'Builder' }).first().click();
  // The pane refreshes its ledger on this event, so the stub above lands even
  // if the view mounted before it was set.
  await page.evaluate(() => window.__xnautEmit('sandbox-verify-changed', {}));
  await expect(page.locator('[data-act-kind="registry_failed"]')).toBeVisible();
}

test('a row with a ticket navigates to it', async ({ page }) => {
  await openActions(page);

  // Call through to the real navigation, and record what it was handed: the
  // ticket has to survive the hop, not just cause a pane to appear.
  await page.evaluate(() => {
    window.__opened = [];
    const real = window.xnautOpenDelivery;
    window.xnautOpenDelivery = (opts) => { window.__opened.push(opts); return real(opts); };
  });

  await page.locator('[data-act-ticket="SMOKE-1"]').first().click();

  expect(await page.evaluate(() => window.__opened)).toEqual([
    { project: 'SMOKE', ticket: 'SMOKE-1', tab: 'tests' },
  ]);
  await expect(page.locator('.dlv-pane')).toBeVisible();
});

test('a row with a run opens the run detail', async ({ page }) => {
  await openActions(page);

  await page.locator('[data-act-run]').first().click();

  // The registry's own answer, asked for by id.
  const asked = await page.evaluate(() => (window.__xnautInvokes || []).filter((item) => item.cmd === 'run_detail'));
  expect(asked).toEqual([{ cmd: 'run_detail', args: { runId: 'run-9' } }]);

  const field = (name) => page.locator(`[data-run-field="${name}"]`);
  await expect(field('state')).toHaveText('blocked');
  await expect(field('kind')).toHaveText('agent');
  await expect(field('ticket')).toHaveText('SMOKE-1');
  await expect(field('agent')).toHaveText('@claude');
  await expect(field('runtime')).toHaveText('claude · claude-opus-5');
  await expect(field('branch')).toHaveText('agent/claude/smoke-1');
  await expect(field('machine')).toHaveText('cand0riacstudio');
  await expect(field('waiting_on')).toHaveText('the read_only kill-switch');
  await expect(field('signal')).toHaveText('admission_refused');
  await expect(page.locator('.aqp-run')).toContainText('run-9');
  await expect(page.locator('[data-run-capture]')).toContainText('read_only kill-switch engaged');
  await expect(page.locator('[data-run-capture]')).toContainText('nothing further was attempted');

  // The trail below is the second half of the same view: run-9's rows only.
  await expect(page.locator('[data-act-kind="registry_failed"]')).toBeVisible();
  await expect(page.locator('[data-act-kind="sweep_refused"]')).toBeVisible();
  await expect(page.locator('[data-act-kind="dispatched"]')).toHaveCount(0);
  await expect(page.locator('[data-act-kind="launch_not_durable"]')).toHaveCount(0);

  await page.locator('[data-act-close-run]').click();
  await expect(page.locator('.aqp-run')).toHaveCount(0);
  await expect(page.locator('[data-act-kind="dispatched"]')).toBeVisible();
});

test('a run whose manifest is gone says so and leaves the trail on screen', async ({ page }) => {
  await openActions(page);
  // A ledger row outlives the run it describes, so a reaped manifest is the
  // ordinary case, not an exception.
  await page.evaluate(() => { window.__xnautStub.run_detail = { __reject: 'no manifest for run-9' }; });

  await page.locator('[data-act-run]').first().click();

  await expect(page.locator('.aqp-run-error')).toHaveText('no manifest for run-9');
  await expect(page.locator('[data-run-field="state"]')).toHaveCount(0);
  // Never a blank pane: the run's trail is still readable underneath.
  await expect(page.locator('[data-act-kind="registry_failed"]')).toBeVisible();
  await expect(page.locator('[data-act-kind="sweep_refused"]')).toBeVisible();

  await page.locator('[data-act-close-run]').click();
  await expect(page.locator('.aqp-run')).toHaveCount(0);
  await expect(page.locator('[data-act-kind="dispatched"]')).toBeVisible();
});

test('a row with a live session attaches that session, and an unknown one offers nothing', async ({ page }) => {
  await openActions(page);

  // The ledger's `session` is the evidence-chain id, not a PTY id. Only the one
  // agent_sessions_list knows gets a button; the rest would open a dead tab.
  await expect(page.locator('[data-act-session]')).toHaveCount(1);
  await expect(page.locator('[data-act-session="sess-live"]')).toBeVisible();

  await page.locator('[data-act-session="sess-live"]').click();
  const attached = await page.evaluate(() => (window.xnaut.tabs || []).map((tab) => tab.agentSessionId));
  expect(attached).toContain('sess-live');
});

test('thirty identical rows render as one with x30', async ({ page }) => {
  await openActions(page);

  await expect(page.locator('[data-act-kind="launch_not_durable"]')).toHaveCount(1);
  await expect(page.locator('[data-act-repeats]')).toHaveText('x30');
  await expect(page.locator('[data-act-repeats]')).toHaveAttribute('aria-expanded', 'false');
  // The two run-9 rows differ in kind, so neither is swallowed by the collapse.
  await expect(page.locator('[data-act-kind="registry_failed"]')).toHaveCount(1);
  await expect(page.locator('[data-act-kind="sweep_refused"]')).toHaveCount(1);
});

test('the count expands on click', async ({ page }) => {
  await openActions(page);

  await expect(page.locator('.aqp-repeats')).toHaveCount(0);
  await page.locator('[data-act-repeats]').click();

  await expect(page.locator('.aqp-repeats .aqp-event')).toHaveCount(30);
  await expect(page.locator('[data-act-repeats]')).toHaveAttribute('aria-expanded', 'true');

  await page.locator('[data-act-repeats]').click();
  await expect(page.locator('.aqp-repeats')).toHaveCount(0);
});

test('the kind chip filters the list', async ({ page }) => {
  await openActions(page);

  await page.locator('[data-act-kind="dispatched"]').click();

  await expect(page.locator('.aqp-filter')).toContainText('dispatched');
  await expect(page.locator('[data-act-kind="dispatched"]')).toHaveCount(1);
  await expect(page.locator('[data-act-kind="registry_failed"]')).toHaveCount(0);
  await expect(page.locator('[data-act-repeats]')).toHaveCount(0);

  // The chip is a toggle: clicking the same kind again puts everything back.
  await page.locator('[data-act-kind="dispatched"]').click();
  await expect(page.locator('[data-act-kind="registry_failed"]')).toHaveCount(1);
});

test('a row is reachable and activatable from the keyboard', async ({ page }) => {
  await openActions(page);

  const chip = page.locator('[data-act-kind="dispatched"]');
  expect(await chip.evaluate((el) => el.tagName)).toBe('BUTTON');

  // Enter on the focused control filters, exactly as the click does, and the
  // chip keeps focus across the repaint instead of dumping a keyboard user at
  // the top of the document.
  await chip.press('Enter');
  await expect(page.locator('.aqp-filter')).toContainText('dispatched');
  const focus = await page.evaluate(() => {
    const el = document.activeElement;
    if (!el || !el.matches('[data-act-kind]')) return { on: '' };
    return { on: el.dataset.actKind, ring: el.matches(':focus-visible'), outline: getComputedStyle(el).outlineWidth };
  });
  expect(focus.on, 'the kind chip keeps focus across the repaint').toBe('dispatched');
  expect(focus.ring, 'keyboard focus is visible').toBe(true);
  expect(focus.outline, 'the focus ring is actually painted').toBe('2px');

  // Space activates the clear control, the other half of what a button owes.
  await page.locator('[data-act-clear-kind]').press(' ');
  await expect(page.locator('.aqp-filter')).toHaveCount(0);
  await expect(page.locator('[data-act-kind="registry_failed"]')).toHaveCount(1);

  expect(await page.evaluate(() => window.__xnautErrors || [])).toEqual([]);
});
