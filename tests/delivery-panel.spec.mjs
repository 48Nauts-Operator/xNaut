// The Delivery panel is a reader over data four other systems already write.
// Every way it can be wrong is silent: a renamed field renders an empty column,
// a missed event leaves a run stuck on "running", a broken download writes a
// file nobody opens until later. So the assertions name values, not shapes.
import { test, expect } from '@playwright/test';

const RECORDS = [
  { id: 'r1', run_id: 'run-1', ticket_id: 'XNAUT-208', project: 'XNAUT', repo_path: '/tmp/x',
    provider_kind: 'gitvm', sandbox_id: 'sb-1', public_url: 'https://sb-1.nautbox.dev', status: 'passed',
    steps: [{ name: 'build', command: 'cargo build', exit_code: 0, log_tail: 'Finished dev' },
            { name: 'test', command: 'cargo test', exit_code: 0, log_tail: '8 passed' }],
    log_dir: '/tmp/logs/r1', video_path: '', error: '',
    created_at: '2026-08-19T10:00:00Z', updated_at: '2026-08-19T10:02:30Z' },
  { id: 'r2', run_id: 'run-2', ticket_id: 'XNAUT-207', project: 'XNAUT', repo_path: '/tmp/x',
    provider_kind: 'gitvm', sandbox_id: 'sb-2', public_url: '', status: 'failed',
    steps: [{ name: 'test', command: 'cargo test', exit_code: 101, log_tail: 'assertion failed: left != right' }],
    log_dir: '/tmp/logs/r2', video_path: '', error: 'step "test" exited 101',
    created_at: '2026-08-19T09:00:00Z', updated_at: '2026-08-19T09:01:00Z' },
  // Another project's run: it must not be counted in XNAUT's stats.
  { id: 'r3', run_id: 'run-3', ticket_id: 'BUCKY-1', project: 'BUCKY', repo_path: '/tmp/b',
    provider_kind: 'gitvm', sandbox_id: 'sb-3', public_url: '', status: 'failed', steps: [],
    log_dir: '', video_path: '', error: '', created_at: '2026-08-18T10:00:00Z', updated_at: '2026-08-18T10:00:10Z' },
];

const STUB = {
  pm_project_list: [
    { key: 'XNAUT', name: 'xnaut', source_path: '/tmp/x', stage: '', flow_type: '', revision: 1 },
    { key: 'BUCKY', name: 'Bucky', source_path: '/tmp/b', stage: '', flow_type: '', revision: 1 },
  ],
  sandbox_verify_records: RECORDS,
  git_release_history: [
    { tag: 'v1.18.1', date: '2026-08-19', subject: 'xNAUT 1.18.1', commits: 3, tickets: ['XNAUT-201'] },
    { tag: 'v1.18.0', date: '2026-08-19', subject: 'agents that tell the truth', commits: 12, tickets: ['XNAUT-194', 'XNAUT-196'] },
  ],
  git_commit_log: [
    { sha: 'a'.repeat(40), short_sha: 'aaaaaaa', subject: 'feat: XNAUT-208 delivery panel', author: 'Cand0rian',
      date: '2026-08-19', added: 500, deleted: 20, files: ['src/js/delivery-panel.js'], ticket: 'XNAUT-208', tag: null },
    { sha: 'b'.repeat(40), short_sha: 'bbbbbbb', subject: 'fix: XNAUT-207 receipts', author: 'Cand0rian',
      date: '2026-08-18', added: 40, deleted: 8, files: ['src-tauri/src/chat.rs', 'src-tauri/tests/chat.rs'],
      ticket: 'XNAUT-207', tag: 'v1.18.1' },
    { sha: 'c'.repeat(40), short_sha: 'ccccccc', subject: 'chore(release): 1.18.1', author: 'Cand0rian',
      date: '2026-08-18', added: 2, deleted: 2, files: ['src-tauri/Cargo.toml'], ticket: null, tag: 'v1.18.1' },
  ],
  pm_ticket_list: [
    { id: 'XNAUT-208', project: 'XNAUT', title: 'delivery loop', type: 'feature', status: 'in_progress',
      priority: 'high', owner: 'Claude', revision: 1, updated_at: '2026-08-19T22:00:00Z' },
    { id: 'XNAUT-207', project: 'XNAUT', title: 'proof of work', type: 'feature', status: 'review',
      priority: 'high', owner: 'Claude', revision: 1, updated_at: '2026-08-19T20:00:00Z' },
    // In review, no commit in the window: the report has to say so out loud.
    { id: 'XNAUT-138', project: 'XNAUT', title: 'unproven work', type: 'bug', status: 'review',
      priority: 'high', owner: 'Claude', revision: 1, updated_at: '2026-08-19T18:00:00Z' },
  ],
  get_home_directory: '/tmp/home',
  write_file: null,
};

async function openDelivery(page) {
  await page.addInitScript(() => localStorage.setItem('xnaut-sidebar-visible', '1'));
  await page.goto('/?stub=1');
  await page.waitForSelector('#btn-help');
  await page.evaluate((stub) => { Object.assign(window.__xnautStub, stub); }, STUB);
  await page.waitForTimeout(2500);
  await page.evaluate(() => window.xnautOpenDelivery({ project: 'XNAUT', tab: 'tests' }));
  await expect(page.locator('.dlv-pane')).toBeVisible();
}

test('Tests tab reports the selected project only, and follows a live run', async ({ page }) => {
  await openDelivery(page);

  // Two XNAUT runs, one passed: BUCKY's failure must not appear in the rate.
  await expect(page.locator('.dlv-stat', { hasText: 'runs' })).toContainText('2');
  await expect(page.locator('.dlv-stat', { hasText: 'pass rate' })).toContainText('50%');
  await expect(page.locator('[data-run="r3"]')).toHaveCount(0);

  // Expand the failed run: the step, its exit code and its output.
  await page.locator('[data-run="r2"] .dlv-row-h').click();
  await expect(page.locator('[data-run="r2"]')).toContainText('exit 101');
  await expect(page.locator('[data-run="r2"] pre')).toContainText('assertion failed');
  await expect(page.locator('[data-run="r2"]')).toContainText('step "test" exited 101');

  // A live step arrives on the event the backend already emits.
  await page.evaluate(() => window.__xnautEmit('sandbox-verify-changed', {
    id: 'r2', run_id: 'run-2', ticket_id: 'XNAUT-207', project: 'XNAUT', repo_path: '/tmp/x',
    provider_kind: 'gitvm', sandbox_id: 'sb-2', public_url: '', status: 'passed',
    steps: [{ name: 'test', command: 'cargo test', exit_code: 0, log_tail: '9 passed' }],
    log_dir: '/tmp/logs/r2', video_path: '', error: '',
    created_at: '2026-08-19T09:00:00Z', updated_at: '2026-08-19T09:05:00Z',
  }));
  await expect(page.locator('.dlv-stat', { hasText: 'pass rate' })).toContainText('100%');

  expect(await page.evaluate(() => window.__xnautErrors || [])).toEqual([]);
});

test('Releases tab lists tags newest first with their tickets', async ({ page }) => {
  await openDelivery(page);
  await page.locator('.dlv-tabs button[data-tab="releases"]').click();

  await expect(page.locator('.dlv-stat', { hasText: 'latest' })).toContainText('v1.18.1');
  const tags = await page.locator('.dlv-row-h b').allTextContents();
  expect(tags.slice(0, 2)).toEqual(['v1.18.1', 'v1.18.0']);
  await expect(page.locator('.dlv-body')).toContainText('XNAUT-196');
});

test('Report tab totals the window and names work with no commit behind it', async ({ page }) => {
  await openDelivery(page);
  await page.locator('.dlv-tabs button[data-tab="report"]').click();

  // Anchored: "test commits" also contains "commits".
  await expect(page.locator('.dlv-stat').filter({ hasText: /^\d+commits$/ })).toContainText('3');
  await expect(page.locator('.dlv-stat', { hasText: 'lines added' })).toContainText('542');
  await expect(page.locator('.dlv-stat', { hasText: 'lines removed' })).toContainText('30');
  await expect(page.locator('.dlv-stat', { hasText: 'ticket-linked' })).toContainText('2/3');
  // One commit touched src-tauri/tests/chat.rs.
  await expect(page.locator('.dlv-stat', { hasText: 'test commits' })).toContainText('1');
  // In review with nothing behind it.
  await expect(page.locator('.dlv-err')).toContainText('XNAUT-138');

  // Download writes one self-contained file where it says it does.
  await page.locator('.dlv-download').click();
  await expect(page.locator('.dlv-download')).toHaveText('Saved to Downloads');
  const write = await page.evaluate(() => window.__xnautInvokes.filter((i) => i.cmd === 'write_file').pop());
  expect(write.args.path).toMatch(/^\/tmp\/home\/Downloads\/XNAUT-work-report-\d{4}-\d{2}-\d{2}\.html$/);
  expect(write.args.content).toContain('<!doctype html>');
  expect(write.args.content).toContain('XNAUT-138');
  expect(write.args.content).not.toMatch(/src=|href=/); // no external assets
});
