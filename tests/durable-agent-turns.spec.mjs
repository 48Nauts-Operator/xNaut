import { test, expect } from '@playwright/test';

async function seed(page, withOutbox = false) {
  await page.addInitScript(({ withOutbox }) => {
    if (localStorage.getItem('__fixture_conversations')) return;
    const request = { handle: 'builder', requestId: 'durable-original', threadId: 'original',
      messages: [{ role: 'user', content: 'Inspect the original project' }],
      repositoryContext: ['/tmp/original'], projectScope: 'ORIGINAL' };
    const value = JSON.stringify({
      builder: [{ id: 'original', title: 'Original project', created_at: '2026-10-08', updated_at: '2026-10-08',
        messages: [{ id: 'u', role: 'user', text: 'Inspect the original project' },
          { id: 'a', role: 'agent', text: 'Thinking…', executionRequestId: request.requestId,
            executionStatus: 'pending', journalProject: 'ORIGINAL', ...(withOutbox ? { executionRequest: request } : {}) }] }],
      nautbot: [{ id: 'other', title: 'Other project', created_at: '2026-10-08', updated_at: '2026-10-08',
        messages: [{ id: 'other-message', role: 'user', text: 'Keep this unrelated conversation' }] }],
    });
    localStorage.setItem('__fixture_conversations', JSON.stringify({ 'xnaut-agent-threads:v1': { value, revision: 1 } }));
  }, { withOutbox });
  await page.goto('/?stub=1');
  await page.waitForSelector('#btn-help');
  await page.evaluate(() => window.xnautConversationStorage.ready());
}
const saved = page => page.evaluate(() => JSON.parse(JSON.parse(localStorage.getItem('__fixture_conversations'))['xnaut-agent-threads:v1'].value));
const result = { request_id: 'durable-original', handle: 'builder', thread_id: 'original', status: 'completed',
  result: 'Recovered the original project result.', partial: '', error: null,
  execution_receipts: [{ ok: true, worktree_path: '/tmp/original-worker', launch: { session_id: 'original-worker', run_id: 'original-run' } }],
  outcome: { swarm_plan: { id: 'recovered-plan', project: 'ORIGINAL', max_parallel: 1, runs: [{ ticket: 'ORIGINAL-1', handle: 'builder', model: 'fixture', title: 'Original work' }] } } };

test('recovered results stay in their original thread and plan cards never auto-dispatch', async ({ page }) => {
  await seed(page);
  await page.evaluate(() => window.xnautOpenAgentSpace('nautbot', 'other'));
  await page.evaluate(async result => {
    window.__xnautStub.durable_agent_turns = [result];
    await window.xnautDurableAgentTurns.reconcile();
  }, result);
  await expect.poll(async () => (await saved(page)).builder[0].messages[1].text).toBe(result.result);
  const records = await saved(page);
  expect(records.builder[0].messages[1].journalProject).toBe('ORIGINAL');
  expect(records.nautbot[0].messages).toHaveLength(1);
  expect(records.builder[0].session_id).toBe('original-worker');
  await expect(page.getByText('Keep this unrelated conversation', { exact: true })).toBeVisible();
  await expect(page.getByText(result.result, { exact: true })).toHaveCount(0);
  await page.evaluate(() => window.xnautOpenAgentSpace('builder', 'original'));
  await expect(page.getByText(result.result, { exact: true })).toBeVisible();
  await expect(page.locator('[data-swarm-go="recovered-plan"]')).toBeVisible();
  await page.evaluate(() => window.xnautDurableAgentTurns.reconcile());
  expect((await saved(page)).builder[0].messages.filter(m => m.kind === 'swarm')).toHaveLength(1);
  expect((await saved(page)).builder[0].messages.filter(m => m.executionReceipt)).toHaveLength(1);
  const commands = await page.evaluate(() => window.__xnautInvokes.map(call => call.cmd));
  expect(commands).toContain('durable_agent_turn_ack');
  expect(commands).not.toContain('swarm_plan_dispatch');
  await page.reload(); await page.waitForSelector('#btn-help');
  await page.evaluate(() => window.xnautOpenAgentSpace('builder', 'original'));
  await expect(page.getByText(result.result, { exact: true })).toBeVisible();
});

test('failed conversation persistence keeps the execution result unacknowledged', async ({ page }) => {
  await seed(page);
  await page.evaluate(async result => {
    window.__xnautStub.durable_agent_turns = [result];
    window.__xnautStub.conversation_store_put = { __reject: 'Disk full' };
    await window.xnautDurableAgentTurns.reconcile();
  }, result);
  await expect(page.locator('#durable-turn-error')).toContainText('Saved execution results are retained');
  const ack = await page.evaluate(() => window.__xnautInvokes.filter(call => call.cmd === 'durable_agent_turn_ack'));
  expect(ack).toHaveLength(0);
  expect((await saved(page)).builder[0].messages[1].text).toBe('Thinking…');
});

test('a late plan event cannot attach to a different thread of the same agent', async ({ page }) => {
  await seed(page);
  await page.evaluate(async () => {
    const store = window.xnautConversationStorage;
    const all = JSON.parse(store.getItem('xnaut-agent-threads:v1'));
    all.builder.push({ id: 'newer', title: 'Different project', messages: [], created_at: '2026-10-09', updated_at: '2026-10-09' });
    store.setItem('xnaut-agent-threads:v1', JSON.stringify(all));
    await store.confirmSaved('xnaut-agent-threads:v1');
    await window.xnautOpenAgentSpace('builder', 'newer');
  });
  await expect(page.getByLabel('Message @builder')).toBeVisible();
  await page.evaluate(result => {
    window.__xnautEmit('swarm-plan-proposed', { agent_id: 'builder', thread_id: 'original', plan: result.outcome.swarm_plan });
  }, result);
  await page.evaluate(() => window.xnautConversationStorage.flush());
  expect((await saved(page)).builder.find(t => t.id === 'newer').messages).toHaveLength(0);
  await expect(page.locator('[data-swarm-go="recovered-plan"]')).toHaveCount(0);
});

test('native submission uses the request identity already saved with its reply placeholder', async ({ page }) => {
  await seed(page);
  await page.evaluate(() => window.xnautOpenAgentSpace('builder', 'original'));
  await page.evaluate(() => {
    const invoke = window.__TAURI__.core.invoke;
    window.__TAURI__.core.invoke = async (cmd, args) => {
      if (cmd === 'agent_chat_turn') {
        const saved = JSON.parse(JSON.parse(localStorage.getItem('__fixture_conversations'))['xnaut-agent-threads:v1'].value);
        const message = saved.builder[0].messages.find(m => m.executionRequestId === args.requestId);
        window.__durableSubmission = { args, message };
      }
      return invoke(cmd, args);
    };
  });
  await page.getByLabel('Message @builder').fill('Read the project documentation');
  await page.getByLabel('Message @builder').press('Enter');
  await expect.poll(() => page.evaluate(() => !!window.__durableSubmission)).toBe(true);
  const submission = await page.evaluate(() => window.__durableSubmission);
  expect(submission.message.executionRequest).toEqual(submission.args);
  expect(submission.args.threadId).toBe('original');
  expect(submission.args.messages.some(m => m.content === 'Thinking…')).toBe(false);
});

test('the saved submission outbox retries the same ID after reload', async ({ page }) => {
  await seed(page, true);
  await expect.poll(() => page.evaluate(() => window.__xnautInvokes.filter(call => call.cmd === 'agent_chat_turn').length)).toBeGreaterThan(0);
  const calls = await page.evaluate(() => window.__xnautInvokes.filter(call => call.cmd === 'agent_chat_turn'));
  expect(calls.every(call => call.args.requestId === 'durable-original' && call.args.projectScope === 'ORIGINAL')).toBe(true);
  expect(calls[0].args.repositoryContext).toEqual(['/tmp/original']);
  await page.evaluate(async result => {
    window.__xnautStub.durable_agent_turns = [result];
    await window.xnautDurableAgentTurns.reconcile();
  }, result);
  await expect.poll(async () => (await saved(page)).builder[0].messages[1].executionRequest).toBeUndefined();
});
