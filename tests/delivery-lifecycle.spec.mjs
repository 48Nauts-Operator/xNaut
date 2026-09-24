// Delivery > Tests, redesigned (XNAUT-329).
//
// The tab's failure mode is not a crash. It is a confident page: a pass rate
// that counted runs which never ran a test, a lifecycle drawn with the stages
// nobody reached quietly missing, a log shown in place of a result. None of
// that throws, so the assertions below name values and counts rather than
// shapes, and one of them deliberately runs with the lifecycle reader broken.
import { test, expect } from '@playwright/test';

// Anchored to now: the "tickets verified" donut counts against the tickets
// that moved inside the selector's window, and absolute dates go stale on a
// calendar rather than on a code change (XNAUT-283).
const ago = (days) => new Date(Date.now() - days * 86400000).toISOString();
const at = (hours) => new Date(Date.now() - hours * 3600000).toISOString();

const REC = (over) => ({
  id: '', run_id: 'run', ticket_id: '', project: 'XNAUT', repo_path: '/tmp/x', commit_sha: '',
  provider_kind: 'gitvm', sandbox_id: '', public_url: '', status: 'passed', steps: [],
  log_dir: '/tmp/logs', video_path: '', error: '', created_at: '', updated_at: '', ...over,
});

const RECORDS = [
  // Reached the test step and passed.
  REC({ id: 'v-green', ticket_id: 'XNAUT-401', status: 'passed', created_at: at(4), updated_at: at(3.97),
    sandbox_id: 'sb-401',
    steps: [{ name: 'install', command: 'npm ci', exit_code: 0, log_tail: 'added 412 packages' },
      { name: 'build', command: 'npm run build', exit_code: 0, log_tail: 'built' },
      { name: 'test', command: 'npx playwright test', exit_code: 0, log_tail: '31 passed' }] }),
  // Reached the test step and failed: a real result, and in the pass rate.
  REC({ id: 'v-red', ticket_id: 'XNAUT-402', status: 'failed', created_at: at(5), updated_at: at(4.9),
    public_url: 'https://sb-402.nautbox.dev', commit_sha: 'abc1234def5678', error: 'step "test" exited 1',
    steps: [{ name: 'install', command: 'npm ci', exit_code: 0, log_tail: 'added 412 packages' },
      { name: 'test', command: 'npx playwright test', exit_code: 1, log_tail: 'FAIL tests/foo.spec.mjs > renders' }] }),
  // Died in install: nothing was tested, in either direction.
  REC({ id: 'v-install', ticket_id: 'XNAUT-403', status: 'failed', created_at: at(6), updated_at: at(5.99),
    error: 'step "install" exited 2',
    steps: [{ name: 'install', command: 'npm ci', exit_code: 2, log_tail: 'E404 no such package' }] }),
  // The app died under it (sandbox_verify writes "orphaned" at startup).
  REC({ id: 'v-orphan', ticket_id: 'XNAUT-404', status: 'orphaned', created_at: at(7), updated_at: at(6.9),
    steps: [{ name: 'install', command: 'npm ci', exit_code: 0, log_tail: 'added 412 packages' }] }),
  // Another project's run: it must not reach XNAUT's list or its donuts.
  REC({ id: 'v-bucky', ticket_id: 'BUCKY-9', project: 'BUCKY', status: 'passed', created_at: at(2), updated_at: at(1.9),
    steps: [{ name: 'test', command: 'pytest', exit_code: 0, log_tail: '3 passed' }] }),
];

// The shape delivery.rs returns (types Lifecycle, Stage, VerifySummary,
// SuiteTotal, StepChip, Evidence). Three stages are present and empty, which
// is what a ticket that has not shipped yet looks like.
const LIFECYCLE = {
  ticket: 'XNAUT-402', project: 'XNAUT', title: 'the tab shows what was proved',
  kind: 'feature', priority: 'high', owner: 'Claude', branch: 'feat/xnaut-402', status: 'review',
  body: 'Delivery > Tests shows raw step logs and nothing about what was tested.',
  files: ['src-tauri/src/delivery.rs', 'src/js/delivery-panel.js'],
  commits: ['9d68168aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'],
  stages: [
    { key: 'issue', title: 'Issue', at: ago(3), text: 'The tab shows logs, not outcomes.', items: [] },
    { key: 'proposed', title: 'Proposed solution', at: ago(2), text: 'Join the five sources into one reader.', items: [] },
    { key: 'final', title: 'Final solution', at: ago(1), text: 'delivery.rs joins them.', items: ['src-tauri/src/delivery.rs'] },
    { key: 'tested', title: 'Tested', at: ago(1), text: '235 passed, 1 failed.', items: [] },
    { key: 'done', title: 'Done', at: '', text: '', items: [] },
    { key: 'merged', title: 'Merged', at: '', text: '', items: [] },
    { key: 'learnings', title: 'Learnings', at: '', text: '', items: [] },
  ],
  verify: {
    record_id: 'v-red', status: 'failed',
    suites: [{ name: 'cargo test', passed: 204, failed: 0, skipped: 2 },
      { name: 'playwright', passed: 31, failed: 1, skipped: 0 }],
    failing: ['delivery lifecycle > every stage renders'],
    steps: [{ name: 'install', command: 'npm ci', exit_code: 0, duration_ms: 41200 },
      { name: 'test', command: 'npx playwright test', exit_code: 1, duration_ms: 9400 }],
    sandbox: 'https://sb-402.nautbox.dev', commit: 'abc1234def5678',
  },
  evidence: { video_path: '', screenshot_path: '', note: 'Capture lands with XNAUT-330.' },
};

const STUB = {
  git_commit_diff: [
    'diff --git a/src-tauri/src/delivery.rs b/src-tauri/src/delivery.rs',
    'index 1111111..2222222 100644',
    '--- a/src-tauri/src/delivery.rs',
    '+++ b/src-tauri/src/delivery.rs',
    '@@ -1,3 +1,4 @@',
    ' use serde::Serialize;',
    '+pub struct Lifecycle {}',
    '-fn gone() {}',
    'diff --git a/src/js/delivery-panel.js b/src/js/delivery-panel.js',
    '--- a/src/js/delivery-panel.js',
    '+++ b/src/js/delivery-panel.js',
    '@@ -10,2 +10,3 @@',
    "+  const codePane = () => 'here';",
  ].join('\n'),
  pm_project_list: [
    { key: 'XNAUT', name: 'xnaut', source_path: '/tmp/x', stage: '', flow_type: '', revision: 1 },
    { key: 'BUCKY', name: 'Bucky', source_path: '/tmp/b', stage: '', flow_type: '', revision: 1 },
  ],
  sandbox_verify_records: RECORDS,
  pm_ticket_list: [
    { id: 'XNAUT-401', project: 'XNAUT', title: 'the donuts', type: 'feature', status: 'review',
      priority: 'high', owner: 'Claude', body: 'Totals as donuts, not tiles.', release: '1.27.0',
      revision: 1, updated_at: ago(1) },
    { id: 'XNAUT-402', project: 'XNAUT', title: 'the tab shows what was proved', type: 'feature', status: 'review',
      priority: 'high', owner: 'Claude', release: '1.26.4', revision: 1, updated_at: ago(1) },
    { id: 'XNAUT-403', project: 'XNAUT', title: 'install is not a verdict', type: 'bug', status: 'ready',
      priority: 'medium', owner: 'Claude', revision: 1, updated_at: ago(2) },
    { id: 'XNAUT-404', project: 'XNAUT', title: 'orphaned runs', type: 'bug', status: 'ready',
      priority: 'low', owner: 'Claude', revision: 1, updated_at: ago(2) },
  ],
  delivery_lifecycle: LIFECYCLE,
};

async function openDelivery(page, extra) {
  await page.addInitScript(() => localStorage.setItem('xnaut-sidebar-visible', '1'));
  await page.goto('/?stub=1');
  await page.waitForSelector('#btn-help');
  await page.evaluate((stub) => { Object.assign(window.__xnautStub, stub); }, { ...STUB, ...(extra || {}) });
  await page.waitForTimeout(2500);
  await page.evaluate(() => window.xnautOpenDelivery({ project: 'XNAUT', tab: 'tests' }));
  await expect(page.locator('.dlv-pane')).toBeVisible();
}

test('the project dropdown filters the runs list', async ({ page }) => {
  await openDelivery(page);

  await expect(page.locator('.dlv-run')).toHaveCount(4);
  await expect(page.locator('[data-run="v-bucky"]')).toHaveCount(0);

  await page.locator('.dlv-proj-select').selectOption('BUCKY');

  await expect(page.locator('[data-run="v-bucky"]')).toHaveCount(1);
  await expect(page.locator('[data-run="v-green"]')).toHaveCount(0);
  await expect(page.locator('.dlv-run')).toHaveCount(1);

  expect(await page.evaluate(() => window.__xnautErrors || [])).toEqual([]);
});

test('runs are grouped by release, newest first, and General is last', async ({ page }) => {
  await openDelivery(page);

  // One group per release the runs' tickets name, plus General for the runs
  // whose ticket names none. General sits last: it is the holding pen, not
  // the newest release.
  const groups = await page.locator('.dlv-relgrp b').allTextContents();
  expect(groups).toEqual(['1.27.0', '1.26.4', 'General']);

  // Every group is open to start with: the grouping folds the list away on
  // request, it does not hide it until asked.
  await expect(page.locator('.dlv-relgrp[data-rel="General"]')).toHaveAttribute('aria-expanded', 'true');
  await expect(page.locator('.dlv-side [data-run="v-install"]')).toHaveCount(1);

  // Each header counts its runs and its distinct tickets.
  await expect(page.locator('.dlv-relgrp[data-rel="General"]')).toContainText('2 runs');
  await expect(page.locator('.dlv-relgrp[data-rel="General"]')).toContainText('2 tickets');

  // A run inside a group is still selectable.
  await page.locator('.dlv-side [data-run="v-install"]').click();
  await expect(page.locator('.dlv-issue')).toContainText('XNAUT-403');

  // Closing a group hides its runs and keeps the selection.
  await page.locator('.dlv-relgrp[data-rel="General"]').click();
  await expect(page.locator('.dlv-relgrp[data-rel="General"]')).toHaveAttribute('aria-expanded', 'false');
  await expect(page.locator('.dlv-side [data-run="v-install"]')).toHaveCount(0);
  await expect(page.locator('.dlv-issue')).toContainText('XNAUT-403');

  // And opening it again brings them back.
  await page.locator('.dlv-relgrp[data-rel="General"]').click();
  await expect(page.locator('.dlv-side [data-run="v-install"]')).toHaveCount(1);

  expect(await page.evaluate(() => window.__xnautErrors || [])).toEqual([]);
});

test('a run that never reached the test step is could not verify, and is out of the pass rate', async ({ page }) => {
  await openDelivery(page);

  await expect(page.locator('[data-run="v-install"]')).toContainText('could not verify');
  await expect(page.locator('[data-run="v-orphan"]')).toContainText('could not verify');
  await expect(page.locator('[data-run="v-red"]')).toContainText('failed');

  // One passed, one failed, two that proved nothing. Counting the two as
  // failures would read 25%; dropping them from the list would read 100%.
  await expect(page.locator('.dlv-stat', { hasText: 'pass rate' })).toContainText('50%');
  await expect(page.locator('.dlv-stat', { hasText: 'could not verify' })).toContainText('2');
  await expect(page.locator('.dlv-stat', { hasText: 'runs' })).toContainText('4');
  // One of the four tickets that moved in the window has a green run behind it.
  await expect(page.locator('.dlv-stat', { hasText: 'tickets verified' })).toContainText('1/4');

  // The totals are donuts, drawn here, not tiles and not a library.
  await expect(page.locator('.dlv-donuts svg').first()).toBeVisible();

  // A live step arrives on the event the backend already emits, and the donuts
  // follow it. This was the old tab's only moving part; it still moves.
  await page.evaluate(() => window.__xnautEmit('sandbox-verify-changed', {
    id: 'v-red', run_id: 'run', ticket_id: 'XNAUT-402', project: 'XNAUT', repo_path: '/tmp/x',
    provider_kind: 'gitvm', sandbox_id: '', public_url: '', status: 'passed', commit_sha: '',
    steps: [{ name: 'install', command: 'npm ci', exit_code: 0, log_tail: 'added 412 packages' },
      { name: 'test', command: 'npx playwright test', exit_code: 0, log_tail: '32 passed' }],
    log_dir: '/tmp/logs', video_path: '', error: '',
    created_at: new Date(Date.now() - 5 * 3600000).toISOString(), updated_at: new Date().toISOString(),
  }));
  await expect(page.locator('.dlv-stat', { hasText: 'pass rate' })).toContainText('100%');
  await expect(page.locator('.dlv-stat', { hasText: 'could not verify' })).toContainText('2');
});

test('selecting a run renders the issue and what the verify proved', async ({ page }) => {
  await openDelivery(page);
  await page.locator('[data-run="v-red"]').click();

  const issue = page.locator('.dlv-issue');
  await expect(issue).toContainText('XNAUT-402');
  await expect(issue).toContainText('the tab shows what was proved');
  await expect(issue).toContainText('feature');
  await expect(issue).toContainText('high');
  await expect(issue).toContainText('Claude');
  await expect(issue).toContainText('feat/xnaut-402');
  await expect(issue).toContainText('Delivery > Tests shows raw step logs');

  // The run's own numbers are cards at the top, beside the window's donuts,
  // so the pair the reader compares is on one line.
  const cards = page.locator('.dlv-runstats');
  await expect(cards.locator('[data-suite="cargo test"]')).toContainText('204');
  await expect(cards.locator('[data-suite="playwright"]')).toContainText('31');
  await expect(cards.locator('[data-suite="playwright"]')).toContainText('1 failed');
  await expect(cards.locator('[data-step]')).toHaveCount(2);
  await expect(cards.locator('[data-step="test"]')).toContainText('exit 1');
  await expect(cards.locator('[data-step="test"]')).toContainText('9.4s');
  await expect(page.locator('.dlv-fail-top')).toContainText('delivery lifecycle > every stage renders');
  // The sandbox and the commit are apparatus, so they sit at the foot.
  await expect(page.locator('.dlv-foot')).toContainText('sb-402.nautbox.dev');
  await expect(page.locator('.dlv-foot')).toContainText('abc1234def56');
  // The outcome is on the run row already; it is not repeated here.
  await expect(cards.locator('.dlv-pill')).toHaveCount(0);

  expect(await page.evaluate(() => window.__xnautErrors || [])).toEqual([]);
});

test('the Code tab lists the files the commits touched and shows one file at a time', async ({ page }) => {
  await openDelivery(page);
  await page.locator('[data-run="v-red"]').click();

  // The left half is two readings of the same work, chosen with tabs.
  await expect(page.locator('.dlv-issue')).toBeVisible();
  await page.locator('.dlv-ltab[data-ltab="code"]').click();
  await expect(page.locator('.dlv-issue')).toHaveCount(0);

  // The files come out of the commit's own diff, not out of a list someone
  // typed, so a file the handback forgot to name still appears.
  const files = page.locator('.dlv-codefile');
  await expect(files).toHaveCount(2);
  await expect(files.first()).toContainText('delivery.rs');

  // One file at a time on the right, painted line by line.
  await expect(page.locator('.dlv-code-diff')).toContainText('pub struct Lifecycle {}');
  await expect(page.locator('.dlv-code-diff')).not.toContainText("const codePane");
  // Added and removed lines carry a background, from the shared renderer.
  await expect(page.locator('.xcr-add').first()).toBeVisible();
  await expect(page.locator('.xcr-del').first()).toBeVisible();

  await page.locator('.dlv-codefile', { hasText: 'delivery-panel.js' }).click();
  await expect(page.locator('.dlv-code-diff')).toContainText("const codePane");
  await expect(page.locator('.dlv-code-diff')).not.toContainText('pub struct Lifecycle {}');

  // Back to the issue, and the tab is remembered per pane, not per ticket.
  await page.locator('.dlv-ltab[data-ltab="issue"]').click();
  await expect(page.locator('.dlv-issue')).toContainText('XNAUT-402');

  expect(await page.evaluate(() => window.__xnautErrors || [])).toEqual([]);
});

test('a ticket with no commit says so in Code rather than showing an empty pane', async ({ page }) => {
  await openDelivery(page);
  // Swap the answer before selecting a run whose lifecycle has not been read
  // yet; the panel caches one read per ticket, which is the point of it.
  await page.evaluate(() => {
    window.__xnautStub.delivery_lifecycle = { ...window.__xnautStub.delivery_lifecycle, commits: [], files: ['src/js/x.js'] };
  });
  await page.locator('[data-run="v-install"]').click();
  await page.locator('.dlv-ltab[data-ltab="code"]').click();

  await expect(page.locator('.dlv-left')).toContainText('No commit is recorded on this ticket');
  await expect(page.locator('.dlv-filelist')).toContainText('src/js/x.js');

  expect(await page.evaluate(() => window.__xnautErrors || [])).toEqual([]);
});

test('the raw output stays closed until its control is pressed', async ({ page }) => {
  await openDelivery(page);
  await page.locator('[data-run="v-red"]').click();
  await expect(page.locator('[data-step="test"]')).toContainText('exit 1');

  // The log is on the record and nowhere on the page.
  await expect(page.locator('.dlv-raw')).toHaveCount(0);
  await expect(page.locator('.dlv-pane pre')).toHaveCount(0);

  await page.locator('.dlv-raw-toggle').click();
  // The step logs, in full, exactly as the old tab drew them.
  await expect(page.locator('.dlv-raw pre')).toHaveCount(2);
  await expect(page.locator('.dlv-raw')).toContainText('FAIL tests/foo.spec.mjs');
  await expect(page.locator('.dlv-raw')).toContainText('npx playwright test');
  await expect(page.locator('.dlv-raw')).toContainText('step "test" exited 1');

  await page.locator('.dlv-raw-toggle').click();
  await expect(page.locator('.dlv-raw')).toHaveCount(0);
});

test('every resolution stage renders, including the ones the ticket has not reached', async ({ page }) => {
  await openDelivery(page);
  await page.locator('[data-run="v-red"]').click();

  // Six cards, not seven: the issue is the left pane, and the same paragraph
  // printed twice on one screen taught nobody anything the first copy had not.
  await expect(page.locator('.dlv-card')).toHaveCount(6);
  await expect(page.locator('[data-stage="issue"]')).toHaveCount(0);
  expect(await page.locator('.dlv-card-h b').allTextContents()).toEqual([
    'Proposed solution', 'Final solution', 'Tested', 'Done', 'Merged', 'Learnings',
  ]);

  await expect(page.locator('[data-stage="final"]')).toContainText('src-tauri/src/delivery.rs');
  await expect(page.locator('[data-stage="tested"]')).toContainText('235 passed, 1 failed.');

  // Not reached is not missing. Learnings is the last line either way, and it
  // is not a stage a ticket "reaches": it is empty when nobody wrote one.
  for (const key of ['done', 'merged']) {
    await expect(page.locator(`[data-stage="${key}"]`)).toHaveClass(/empty/);
    await expect(page.locator(`[data-stage="${key}"]`)).toContainText('not reached yet');
  }
  await expect(page.locator('[data-stage="learnings"]')).toHaveClass(/empty/);
  await expect(page.locator('[data-stage="learnings"]')).toContainText('nothing recorded yet');

  // A card with something to say opens itself; closing it keeps the header.
  await expect(page.locator('[data-stage="final"]')).toHaveClass(/open/);
  await page.locator('.dlv-stage-h[data-toggle="final"]').click();
  await expect(page.locator('[data-stage="final"]')).not.toHaveClass(/open/);
  await expect(page.locator('[data-stage="final"]')).not.toContainText('src-tauri/src/delivery.rs');
  await expect(page.locator('.dlv-card-h b').first()).toBeVisible();
});

test('a lifecycle that cannot be read says so and leaves the run on screen', async ({ page }) => {
  // What the panel actually meets today: delivery_lifecycle is registered and
  // answers "not implemented". A blank page, or seven invented stages, would
  // both be worse than saying which half is missing.
  await openDelivery(page, { delivery_lifecycle: { __reject: 'not implemented' } });

  await expect(page.locator('.dlv-life-err')).toContainText('not implemented');

  // The run's own record still fills the page.
  await expect(page.locator('.dlv-run')).toHaveCount(4);
  await expect(page.locator('.dlv-stat', { hasText: 'pass rate' })).toContainText('50%');
  await expect(page.locator('.dlv-issue')).toContainText('XNAUT-401');
  await expect(page.locator('.dlv-issue')).toContainText('the donuts');       // from pm_ticket_list
  await expect(page.locator('.dlv-runstats [data-step]')).toHaveCount(3);

  // Six cards, all of them honestly empty.
  await expect(page.locator('.dlv-card')).toHaveCount(6);
  await expect(page.locator('.dlv-card.empty')).toHaveCount(6);

  expect(await page.evaluate(() => window.__xnautErrors || [])).toEqual([]);
});

test('the evidence overlay opens and says when there is no recording', async ({ page }) => {
  await openDelivery(page);
  await page.locator('[data-run="v-red"]').click();

  await expect(page.locator('.dlv-overlay')).toHaveCount(0);
  await page.locator('.dlv-foot .dlv-thumb').click();

  const overlay = page.locator('.dlv-overlay');
  await expect(overlay).toBeVisible();
  await expect(overlay).toContainText(/no recording for this run/i);
  await expect(overlay).toContainText('Capture lands with XNAUT-330.');
  await overlay.locator('.dlv-overlay-x').click();
  await expect(page.locator('.dlv-overlay')).toHaveCount(0);

  // The thumbnail on the run row opens the same overlay, and selecting is not
  // what it does: the clicked row must not change.
  await page.locator('[data-run="v-green"] .dlv-thumb').click();
  await expect(page.locator('.dlv-overlay')).toContainText(/no recording for this run/i);
  await expect(page.locator('[data-run="v-red"]')).toHaveClass(/active/);
});
