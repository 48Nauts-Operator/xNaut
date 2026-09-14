// The Delivery panel is a reader over data four other systems already write.
// Every way it can be wrong is silent: a renamed field renders an empty column,
// a missed event leaves a run stuck on "running", a broken download writes a
// file nobody opens until later. So the assertions name values, not shapes.
import { test, expect } from '@playwright/test';

// The Report tab filters tickets to a rolling window (delivery-panel.js:308,
// default 7 days). Absolute dates here go stale on a calendar, not on a code
// change: these fixtures were written 2026-08-19 and quietly stopped landing
// inside the window on 2026-08-26 (XNAUT-283). Anchor them to now instead.
const ago = (days) => new Date(Date.now() - days * 86400000).toISOString();

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
  git_release_notes: {
    tag: 'v1.18.1', previous: 'v1.18.0', notes_source: 'changelog',
    notes: 'The release where agents told the truth.\n\n### Added\n- A gate that refuses on drift.',
    commits: [
      { sha: 'c'.repeat(40), short_sha: 'ccccccc', subject: 'feat: XNAUT-201 the gate', author: 'Cand0rian',
        date: '2026-08-19', added: 120, deleted: 4, files: ['src-tauri/src/jury.rs'], ticket: 'XNAUT-201', tag: 'v1.18.1' },
    ],
  },
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
      priority: 'high', owner: 'Claude', revision: 1, updated_at: ago(1) },
    { id: 'XNAUT-207', project: 'XNAUT', title: 'proof of work', type: 'feature', status: 'review',
      priority: 'high', owner: 'Claude', revision: 1, updated_at: ago(2) },
    // In review, no commit in the window: the report has to say so out loud.
    { id: 'XNAUT-138', project: 'XNAUT', title: 'unproven work', type: 'bug', status: 'review',
      priority: 'high', owner: 'Claude', revision: 1, updated_at: ago(3) },
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

  // Select the failed run. The step and its exit code are on the page as
  // chips; the log itself is behind the raw control now (XNAUT-329), which
  // is the whole point of the redesign.
  await page.locator('[data-run="r2"] .dlv-row-h').click();
  await expect(page.locator('[data-step="test"]')).toContainText('exit 101');
  await expect(page.locator('.dlv-raw')).toHaveCount(0);
  await page.locator('.dlv-raw-toggle').click();
  await expect(page.locator('.dlv-raw pre')).toContainText('assertion failed');

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

  // The tags are the left column now, newest first; the centre is the one
  // that is selected, which is the newest until the reader picks another.
  const tags = await page.locator('.dlv-side .dlv-rel b').allTextContents();
  expect(tags.slice(0, 2)).toEqual(['v1.18.1', 'v1.18.0']);
  await expect(page.locator('.dlv-stat').first()).toContainText('v1.18.1');
  await expect(page.locator('.dlv-body')).toContainText('XNAUT-201');

  // Picking an older one moves the centre to it, tickets and all.
  await page.locator('.dlv-side .dlv-rel[data-key="v1.18.0"]').click();
  await expect(page.locator('.dlv-side .dlv-rel[data-key="v1.18.0"]')).toHaveClass(/active/);
  await expect(page.locator('.dlv-stat').first()).toContainText('v1.18.0');
  await expect(page.locator('.dlv-body')).toContainText('XNAUT-196');
});

test('a release shows its text, its commits, and tickets that open in Tests', async ({ page }) => {
  await openDelivery(page);
  await page.locator('.dlv-tabs button[data-tab="releases"]').click();

  // The release text, which is the thing a tag alone cannot tell anyone.
  await expect(page.locator('.dlv-relnotes')).toContainText('The release where agents told the truth.');
  // Rendered as markdown, not shown raw (Andre, 2026-09-14): the heading is
  // a heading, the bullet a list item, and the ### is gone.
  await expect(page.locator('.dlv-relnotes h3')).toHaveText('Added');
  await expect(page.locator('.dlv-relnotes li')).toHaveText('A gate that refuses on drift.');
  await expect(page.locator('.dlv-relnotes')).not.toContainText('###');
  // No inner scrollbar: the block grows with its text.
  expect(await page.locator('.dlv-relnotes').evaluate((el) => getComputedStyle(el).maxHeight)).toBe('none');
  await expect(page.locator('.dlv-body')).toContainText('CHANGELOG.md');
  await expect(page.locator('.dlv-body')).toContainText('v1.18.0..v1.18.1');

  // The work behind it, counted from the commits rather than asserted.
  await expect(page.locator('.dlv-stat', { hasText: 'lines added' })).toContainText('+120');
  await expect(page.locator('.dlv-body')).toContainText('feat: XNAUT-201 the gate');

  // And the way back in: a ticket opens Tests focused on it.
  await page.locator('.dlv-ticketlink', { hasText: 'XNAUT-201' }).click();
  await expect(page.locator('.dlv-tabs button[data-tab="tests"]')).toHaveClass(/active/);
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
