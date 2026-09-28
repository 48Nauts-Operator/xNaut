import { test, expect } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  await page.goto('/?stub=1');
  await page.waitForFunction(() => window.xnautOpenAgentSpace && window.__xnautStub);
  await page.evaluate(() => { window.__xnautStub.voice_live_ready = true; });
});

async function openAgent(page) {
  await page.getByRole('button', { name: 'More surfaces' }).click();
  await page.getByText('Agent Space', { exact: true }).first().click();
  await expect(page.getByLabel('Message @nautbot')).toBeVisible();
}

async function start(page, mode = 'STS · Speech to Speech') {
  await page.locator('.agent-space .voice-live-button').click();
  await page.getByRole('menuitem', { name: mode, exact: true }).click();
  await expect.poll(() => page.evaluate(() => window.__xnautInvokes.filter(i => i.cmd === 'voice_live_open').length)).toBe(1);
}

async function emit(page, events) {
  await page.evaluate(events => {
    const id = window.__xnautInvokes.find(i => i.cmd === 'voice_live_open').args.sessionId;
    for (const event of events) window.__xnautEmit(`voice-live://${id}`, event);
  }, events);
}

test('plus menu opens a usable regular Chat without console commands', async ({ page }) => {
  await page.getByRole('button', { name: 'New terminal', exact: true }).click();
  await page.locator('#new-tab-menu').getByText('New Chat', { exact: true }).click();
  await expect(page.locator('.chatp-pane')).toBeVisible();
  await expect(page.locator('.chatp-pane .voice-live-button')).toBeEnabled();
});

test('Agent Space uses one mic, streams STS through its selected agent and saves the full conversation', async ({ page }) => {
  await openAgent(page);
  await expect(page.locator('.agent-space [data-dictate]')).toHaveCount(1);
  await expect(page.getByRole('button', { name: 'Talk', exact: true })).toHaveCount(0);
  await start(page);
  const binding = await page.evaluate(() => window.__xnautInvokes.find(i => i.cmd === 'voice_live_open').args.binding);
  expect(binding.agent).toBe('nautbot');
  await emit(page, [
    { kind: 'ready' },
    { kind: 'caption', role: 'user', text: 'Read ticket 445.' },
    { kind: 'commit', role: 'user', text: 'Read ticket 445.', turn: 0 },
    { kind: 'caption', role: 'assistant', text: 'Checking the ticket.' },
    { kind: 'dispatch', turn: 0, epoch: 0 },
  ]);
  await expect.poll(() => page.evaluate(() => window.__xnautInvokes.filter(i => i.cmd === 'voice_live_result').length)).toBe(1);
  const call = await page.evaluate(() => window.__xnautInvokes.find(i => i.cmd === 'agent_chat_turn').args);
  expect(call.handle).toBe('nautbot');
  expect(call.messages).toEqual([{ role: 'user', content: 'Read ticket 445.' }]);
  await emit(page, [{ kind: 'commit', role: 'assistant', text: 'The release is tagged and the cask is on 1.15.0.', turn: 0 }]);
  await expect(page.locator('.as-message.user')).toHaveCount(1);
  await expect(page.locator('.as-messages')).toContainText('Checking the ticket.');
  await expect(page.locator('.as-messages')).toContainText('The release is tagged');
  const saved = await page.evaluate(() => JSON.parse(localStorage.getItem('xnaut-agent-threads:v1')).nautbot[0]);
  expect(saved.messages.filter(m => !m.voiceTranscript && m.role === 'agent')).toHaveLength(1);
  await expect(page.locator('.voice-live-docked')).toBeVisible();
  await emit(page, [{ kind: 'playback', speaking: true }]);
  await expect(page.locator('.voice-live-docked')).toHaveAttribute('data-state', 'speaking');
  await page.getByRole('button', { name: 'End voice', exact: true }).click();
  await startAgain();
  async function startAgain() {
    await page.locator('.agent-space .voice-live-button').click();
    await page.getByRole('menuitem', { name: 'STS · Speech to Speech', exact: true }).click();
    await expect.poll(() => page.evaluate(() => window.__xnautInvokes.filter(i => i.cmd === 'voice_live_open').length)).toBe(2);
    const history = await page.evaluate(() => window.__xnautInvokes.filter(i => i.cmd === 'voice_live_open').at(-1).args.history);
    expect(history.map(m => m.text)).toEqual(['Read ticket 445.', 'The release is tagged and the cask is on 1.15.0.']);
  }
});

test('Agent Space STT keeps a draft and switching agent closes its microphone', async ({ page }) => {
  await openAgent(page);
  await page.getByLabel('Message @nautbot').fill('Draft.');
  await start(page, 'STT · Speech to Text');
  await emit(page, [{ kind: 'caption', role: 'user', text: 'More words.' }]);
  await expect(page.getByLabel('Message @nautbot')).toHaveValue('Draft. More words.');
  expect(await page.evaluate(() => window.__xnautInvokes.some(i => i.cmd === 'agent_chat_turn'))).toBe(false);
  await page.locator('.asl-agent', { hasText: 'Builder' }).first().click();
  await expect(page.getByLabel('Message @builder')).toBeVisible();
  await expect.poll(() => page.evaluate(() => window.__xnautInvokes.filter(i => i.cmd === 'voice_live_close').length)).toBe(1);
});

test('spoken build requests keep the existing workspace confirmation', async ({ page }) => {
  await page.evaluate(() => { window.__xnautStub.agent_chat_turn = 'BUILD-REQUEST\nThis needs a coding session.'; });
  await openAgent(page);
  await start(page);
  await emit(page, [
    { kind: 'commit', role: 'user', text: 'Build a dashboard.', turn: 0 },
    { kind: 'dispatch', turn: 0, epoch: 0 },
  ]);
  await expect(page.locator('.as-build')).toBeVisible();
  expect(await page.evaluate(() => window.__xnautInvokes.some(i => i.cmd === 'agent_profile_launch'))).toBe(false);
  await expect.poll(() => page.evaluate(() => window.__xnautInvokes.filter(i => i.cmd === 'voice_live_result').length)).toBe(1);
});

test('pending Agent Space answer stays red during spoken acknowledgement, then becomes green', async ({ page }) => {
  await openAgent(page);
  await start(page);
  await page.evaluate(() => {
    const invoke = window.__TAURI__.core.invoke;
    window.__TAURI__.core.invoke = (name, args) => name === 'agent_chat_turn'
      ? new Promise(resolve => { window.finishAgentAnswer = resolve; }) : invoke(name, args);
  });
  await emit(page, [
    { kind: 'playback', speaking: true },
    { kind: 'commit', role: 'user', text: 'Explain the reliability sprint.', turn: 0 },
    { kind: 'dispatch', turn: 0, epoch: 0 },
    { kind: 'caption', role: 'assistant', text: "I'll get the backend's take." },
  ]);
  await expect(page.locator('.as-message-text').filter({ hasText: /^Thinking…$/ })).toBeVisible();
  await expect(page.locator('.voice-live-docked')).toHaveAttribute('data-state', 'thinking');
  await expect(page.locator('.voice-orb')).toHaveCSS('--voice-color', '#ff5c63');
  await page.evaluate(() => window.finishAgentAnswer('The sprint addresses reliability issues.'));
  await expect(page.locator('.as-message-text').filter({ hasText: /^Thinking…$/ })).toHaveCount(0);
  await expect(page.locator('.voice-live-docked')).toHaveAttribute('data-state', 'speaking');
  await expect(page.locator('.voice-orb')).toHaveCSS('--voice-color', '#2de2a8');
  await emit(page, [{ kind: 'playback', speaking: false }]);
  await expect(page.locator('.voice-live-docked')).toHaveAttribute('data-state', 'listening');
});
