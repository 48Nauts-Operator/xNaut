// Evidence, after XNAUT-329's clean-up.
//
// The tab printed every record twice, once as a command line and once as the
// same command as raw JSON, with a full worktree path and a hash on a third
// line. At 2116 records that is not a chain anyone can read. It is one line
// per record now, and the detail belongs to the record the reader opened.
import { test, expect } from '@playwright/test';

const ago = (days) => new Date(Date.now() - days * 86400000).toISOString();

const FORCED = ['git', 'push', '--force', 'origin', 'main'].join(' ');

const SESSIONS = [
  { session_id: 'bb139ea5-cc87-440d-a33a-67043953ff4d', records: 3, refused: 1, agents: ['claude'],
    sealed: false, shredded: false, kek: 'xnaut-kek-v1', last_at: ago(0) },
  { session_id: '8c0a8ae2-1111-2222-3333-444455556666', records: 1, refused: 0, agents: [],
    sealed: true, shredded: false, kek: 'xnaut-kek-v1', last_at: ago(3) },
];

const RECORDS = [
  { seq: 0, at: ago(0), kind: 'tool_call', tool: 'Bash', decision: 'allow', agent: 'claude',
    summary: 'git log --oneline -15', args: '{"command":"git log --oneline -15","description":"Check branch state"}',
    cwd: '/Users/x/DevHub_Studio/factory/02-Development/xnaut/.worktrees/agent-claude-xnaut-87',
    args_hash: 'sha256:61b6a6290c6bdead' },
  { seq: 1, at: ago(0), kind: 'model_call', model: 'claude-opus-5', agent: 'claude',
    summary: '', args: '', cwd: '', args_hash: '' },
  { seq: 2, at: ago(0), kind: 'tool_refused', tool: 'Bash', decision: 'deny', agent: 'claude',
    rule: 'destructive git is blocked', summary: FORCED,
    args: `{"command":"${FORCED}"}`, cwd: '', args_hash: '' },
];

const STUB = {
  pm_project_list: [{ key: 'XNAUT', name: 'xnaut', source_path: '/tmp/x', stage: '', flow_type: '', revision: 1 }],
  pm_ticket_list: [],
  sandbox_verify_records: [],
  evidence_sessions: SESSIONS,
  evidence_kek_label: 'xnaut-kek-v1',
  evidence_verify: { ok: true, records: 4, sessions: 2, path: '/tmp/evidence/execution.jsonl',
    checked: ['4 records re-hashed', '4 prev_hash links followed'] },
  evidence_records: RECORDS,
  git_release_history: [],
  git_commit_log: [],
};

async function openEvidence(page) {
  await page.addInitScript(() => localStorage.setItem('xnaut-sidebar-visible', '1'));
  await page.goto('/?stub=1');
  await page.waitForSelector('#btn-help');
  await page.evaluate((stub) => { Object.assign(window.__xnautStub, stub); }, STUB);
  await page.waitForTimeout(2500);
  await page.evaluate(() => window.xnautOpenDelivery({ project: 'XNAUT', tab: 'tests' }));
  await expect(page.locator('.dlv-pane')).toBeVisible();
  await page.locator('.dlv-tabs button[data-tab="evidence"]').click();
}

test('the sessions are the left column and the open one fills the centre', async ({ page }) => {
  await openEvidence(page);

  await expect(page.locator('.dlv-side .dlv-sess')).toHaveCount(2);
  await expect(page.locator('.dlv-side')).toContainText('claude');
  await expect(page.locator('.dlv-side')).toContainText('3 records');

  // The newest is open to begin with, and its own header carries the counts.
  await expect(page.locator('.dlv-sess-head')).toContainText('claude');
  await expect(page.locator('.dlv-sess-head')).toContainText('3 records');
  await expect(page.locator('.dlv-sess-head')).toContainText('1 refused');

  expect(await page.evaluate(() => window.__xnautErrors || [])).toEqual([]);
});

test('one line per record, and the detail belongs to the record you opened', async ({ page }) => {
  await openEvidence(page);

  await expect(page.locator('.dlv-rec')).toHaveCount(3);

  // The command is on the line. The JSON that repeats it is not, until asked.
  const first = page.locator('.dlv-rec').first();
  await expect(first).toContainText('git log --oneline -15');
  await expect(page.locator('.dlv-rec-args')).toHaveCount(0);
  await expect(page.locator('.dlv-body')).not.toContainText('"description":"Check branch state"');

  await first.locator('.dlv-rec-h').click();
  await expect(first.locator('.dlv-rec-args')).toContainText('"description":"Check branch state"');
  // The worktree path is shortened to the part that identifies it.
  await expect(first).toContainText('…/xnaut/.worktrees/agent-claude-xnaut-87');
  await expect(first).not.toContainText('/Users/x/DevHub_Studio');

  await first.locator('.dlv-rec-h').click();
  await expect(page.locator('.dlv-rec-args')).toHaveCount(0);

  expect(await page.evaluate(() => window.__xnautErrors || [])).toEqual([]);
});

test('a refused call reads as refused, with the rule behind it', async ({ page }) => {
  await openEvidence(page);

  const denied = page.locator('.dlv-rec.denied');
  await expect(denied).toHaveCount(1);
  await expect(denied).toContainText('refused');
  await expect(denied).toContainText(FORCED);

  await denied.locator('.dlv-rec-h').click();
  await expect(denied).toContainText('refused because: destructive git is blocked');

  expect(await page.evaluate(() => window.__xnautErrors || [])).toEqual([]);
});

test('Shred is on the open session only, never on every row of the list', async ({ page }) => {
  await openEvidence(page);

  await expect(page.locator('.dlv-side .dlv-shred')).toHaveCount(0);
  await expect(page.locator('.dlv-sess-head .dlv-shred')).toHaveCount(1);

  expect(await page.evaluate(() => window.__xnautErrors || [])).toEqual([]);
});
