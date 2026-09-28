// XNAUT-416 V1: the continuous public voice conversation, through the real
// Chat pane with a scripted backend session.
//
// The Rust suite proves the session's ordering and identity rules. This proves
// the other half: that the pane starts once, holds several turns with no Send
// or Speak-again click, runs each turn through the existing chat_send_tools
// path with this conversation's agent/model/permissions, persists one message
// pair per turn, and replays a saved conversation on reopen.
import { test, expect } from '@playwright/test';
import { fileURLToPath } from 'node:url';

const SCRIPTS = ['voice-session', 'voice-overlay', 'voice-dictate', 'voice-conversation', 'voice-live', 'chat-panel'];

async function boot(page, { ready = true, history = null } = {}) {
  await page.route('**/voice-live.html', route =>
    route.fulfill({ contentType: 'text/html', body: '<main id="chat"></main>' }));
  await page.goto('/voice-live.html');
  await page.evaluate(([ready, history]) => {
    window.calls = [];
    window.listeners = {};
    window.answer = 'Build 412 passed.';
    window.liveSessionId = null;
    if (history) localStorage.setItem('xnaut-chat-history:voice-live', JSON.stringify(history));
    window.__TAURI__ = {
      event: {
        listen: async (name, callback) => {
          window.listeners[name] = callback;
          return () => { delete window.listeners[name]; };
        },
      },
      core: {
        invoke: async (name, args) => {
          window.calls.push({ name, args });
          if (name === 'settings_get') return { llm: {}, categories: [], forges: [] };
          if (name === 'agent_list') return [];
          if (name === 'voice_live_ready') return ready;
          if (name === 'voice_live_open') { window.liveSessionId = args.sessionId; return null; }
          if (name === 'chat_send_tools') {
            if (window.answerError) throw new Error(window.answerError);
            return window.answer;
          }
          return null;
        },
      },
    };
    // Pushes one backend event into the open session, the way the Rust side
    // emits it. Returns false when nothing is listening, so a test that races
    // the subscription fails loudly instead of passing on a dropped event.
    window.emitLive = (event) => {
      const listener = window.listeners[`voice-live://${window.liveSessionId}`];
      if (!listener) return false;
      listener({ payload: event });
      return true;
    };
  }, [ready, history]);
  for (const file of SCRIPTS) {
    await page.addScriptTag({ path: fileURLToPath(new URL(`../src/js/${file}.js`, import.meta.url)) });
  }
  await page.evaluate(async () => {
    window.chatEntry = await window.xnautCreateChatPane('test', document.querySelector('#chat'), { chatKey: 'voice-live' });
  });
}

async function start(page) {
  await page.getByRole('button', { name: 'Voice conversation with Chat composer' }).click();
  await expect.poll(() => page.evaluate(() => window.liveSessionId)).not.toBeNull();
  await page.evaluate(() => window.emitLive({ kind: 'ready', restored: false }));
}

/// One complete turn, as the backend delivers it: the user's committed words,
/// then the dispatch that asks the agent to do the work.
async function turn(page, { said, turn: index, delegationId }) {
  await page.evaluate(([said, index]) => {
    window.emitLive({ kind: 'commit', turn: index, role: 'user', text: said });
    window.emitLive({
      kind: 'dispatch',
      turn: index,
      epoch: 0,
      delegationId: `d${index}`,
      context: `user: ${said}`,
      binding: {},
    });
  }, [said, index, delegationId]);
  // The pane answers through chat_send_tools and returns the result.
  await expect.poll(() => page.evaluate(() =>
    window.calls.filter(c => c.name === 'voice_live_result').length)).toBeGreaterThan(index);
  const answer = await page.evaluate(() => window.answer);
  await page.evaluate(([index, answer]) => {
    window.emitLive({ kind: 'commit', turn: index, role: 'assistant', text: answer });
  }, [index, answer]);
}

test('one start carries three spoken turns with no send or speak-again click', async ({ page }) => {
  await boot(page);
  await start(page);
  for (const index of [0, 1, 2]) {
    await page.evaluate((n) => { window.answer = `Answer ${n}.`; }, index);
    await turn(page, { said: `question ${index}`, turn: index });
  }

  const result = await page.evaluate(() => ({
    history: window.chatEntry.history,
    sends: window.calls.filter(c => c.name === 'chat_send_tools').length,
    opens: window.calls.filter(c => c.name === 'voice_live_open').length,
  }));
  expect(result.opens).toBe(1);
  expect(result.sends).toBe(3);
  // Exactly one user and one assistant message per turn, in order.
  expect(result.history.map(m => m.content)).toEqual([
    'question 0', 'Answer 0.',
    'question 1', 'Answer 1.',
    'question 2', 'Answer 2.',
  ]);
  // The overlay is still up: the conversation never ended between turns.
  await expect(page.locator('.voice-live-overlay')).toHaveCount(1);
  // And there is no per-turn control to press.
  await expect(page.getByRole('button', { name: 'Finish & send' })).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Speak again' })).toHaveCount(0);
});

test('each turn runs on this conversation selected agent, model and permissions', async ({ page }) => {
  await boot(page);
  await page.evaluate(() => {
    window.chatEntry.providerOverride = 'nautgate';
    window.chatEntry.modelOverride = 'claude-opus-5';
  });
  await start(page);
  const binding = await page.evaluate(() =>
    window.calls.find(c => c.name === 'voice_live_open').args.binding);
  expect(binding.conversationId).toBe('voice-live');
  expect(binding.provider).toBe('nautgate');
  expect(binding.model).toBe('claude-opus-5');
  expect(binding.permission).toBe('chat');

  await turn(page, { said: 'what is the version?', turn: 0 });
  // The turn went through the ordinary chat path with the same selection.
  const send = await page.evaluate(() => window.calls.find(c => c.name === 'chat_send_tools').args);
  expect(send.provider).toBe('nautgate');
  expect(send.model).toBe('claude-opus-5');
  expect(send.chatKey).toBe('voice-live');
});

test('reopening a saved conversation replays it as context and does not redo the work', async ({ page }) => {
  await boot(page, {
    history: [
      { role: 'user', content: 'my deploy key is named tron-ci' },
      { role: 'assistant', content: 'Noted: tron-ci.' },
    ],
  });
  await start(page);
  const opened = await page.evaluate(() =>
    window.calls.find(c => c.name === 'voice_live_open').args.history);
  expect(opened).toEqual([
    { role: 'user', text: 'my deploy key is named tron-ci' },
    { role: 'assistant', text: 'Noted: tron-ci.' },
  ]);
  // Restoring context runs no agent turns.
  expect(await page.evaluate(() =>
    window.calls.filter(c => c.name === 'chat_send_tools').length)).toBe(0);
  expect(await page.evaluate(() => window.chatEntry.history.length)).toBe(2);
});

test('talking over a reply clears the caption and says so', async ({ page }) => {
  await boot(page);
  await start(page);
  await page.evaluate(() => window.emitLive({ kind: 'caption', turn: 0, role: 'assistant', text: 'Let me check that for you' }));
  await expect(page.locator('[data-live-caption]')).toContainText('Let me check that');
  await page.evaluate(() => window.emitLive({ kind: 'interrupted', generation: 1 }));
  await expect(page.getByRole('status')).toContainText('Stopped');
  await expect(page.locator('[data-live-caption]')).toBeHidden();
});

test('a failed agent turn is reported as a failure, not spoken as an answer', async ({ page }) => {
  await boot(page);
  await start(page);
  await page.evaluate(() => { window.answerError = 'the sandbox is offline'; });
  await page.evaluate(() => {
    window.emitLive({ kind: 'dispatch', turn: 0, epoch: 0, delegationId: 'd0', context: 'user: deploy', binding: {} });
  });
  await expect.poll(() => page.evaluate(() =>
    window.calls.find(c => c.name === 'voice_live_result')?.args.outcome)).toMatchObject({
    turn: 0, epoch: 0, failed: true,
  });
  const outcome = await page.evaluate(() => window.calls.find(c => c.name === 'voice_live_result').args.outcome);
  expect(outcome.answer).toContain('the sandbox is offline');
});

test('a tool action is shown in the conversation rather than read out as JSON', async ({ page }) => {
  await boot(page);
  await start(page);
  await page.evaluate(() => { window.answer = '{"action":"vault_write","content":"tool arguments"}'; });
  await page.evaluate(() => {
    window.emitLive({ kind: 'dispatch', turn: 0, epoch: 0, delegationId: 'd0', context: 'user: save it', binding: {} });
  });
  await expect.poll(() => page.evaluate(() =>
    window.calls.find(c => c.name === 'voice_live_result')?.args.outcome.failed)).toBe(true);
  const outcome = await page.evaluate(() => window.calls.find(c => c.name === 'voice_live_result').args.outcome);
  expect(outcome.answer).not.toContain('tool arguments');
  expect(outcome.answer).toContain('shown in the conversation');
});

test('typing during a voice conversation joins it rather than starting a second one', async ({ page }) => {
  await boot(page);
  await start(page);
  await page.locator('.chatp-input').fill('check the logs');
  await page.locator('.chatp-input').press('Enter');
  await expect.poll(() => page.evaluate(() =>
    window.calls.find(c => c.name === 'voice_live_text')?.args.text)).toBe('check the logs');
  // The composer clears and no parallel chat turn is started; the backend
  // commits the words and dispatches them like any other turn.
  await expect(page.locator('.chatp-input')).toHaveValue('');
  expect(await page.evaluate(() =>
    window.calls.filter(c => c.name === 'chat_send_tools').length)).toBe(0);
  expect(await page.evaluate(() => window.chatEntry.history.length)).toBe(0);
});

test('typed input that the session refuses stays in the composer', async ({ page }) => {
  await boot(page);
  await start(page);
  await page.evaluate(() => {
    const invoke = window.__TAURI__.core.invoke;
    window.__TAURI__.core.invoke = (name, args) => name === 'voice_live_text'
      ? Promise.reject(new Error('session ended'))
      : invoke(name, args);
  });
  await page.locator('.chatp-input').fill('do not lose this');
  await page.locator('.chatp-input').press('Enter');
  await expect(page.locator('.chatp-input')).toHaveValue('do not lose this');
});

test('ending the conversation releases the session and takes the overlay down', async ({ page }) => {
  await boot(page);
  await start(page);
  await page.getByRole('button', { name: 'End voice' }).click();
  await expect(page.locator('.voice-live-overlay')).toHaveCount(0);
  await expect.poll(() => page.evaluate(() =>
    window.calls.some(c => c.name === 'voice_live_close'))).toBe(true);
});

test('the backend ending the session takes the overlay down and says why', async ({ page }) => {
  await boot(page);
  await start(page);
  await page.evaluate(() => window.emitLive({ kind: 'ended', reason: 'The voice service disconnected.' }));
  await expect(page.locator('.voice-live-overlay')).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Voice conversation with Chat composer' }))
    .toHaveAttribute('title', 'The voice service disconnected.');
});

test('closing the pane releases a running conversation', async ({ page }) => {
  await boot(page);
  await start(page);
  // Panes are keyed by the label chat-panel generates, not by the tab id.
  await page.evaluate(() => window.xnautDestroyChatPane(window.chatEntry.pane.dataset.chatLabel));
  await expect.poll(() => page.evaluate(() =>
    window.calls.some(c => c.name === 'voice_live_close'))).toBe(true);
  await expect(page.locator('.voice-live-overlay')).toHaveCount(0);
});

test('a pane torn out of the DOM releases its conversation too', async ({ page }) => {
  await boot(page);
  await start(page);
  // Not every surface closes through destroyChatPane; a detached composer must
  // not leave the microphone and a billable socket running.
  await page.evaluate(() => window.chatEntry.pane.remove());
  await expect.poll(() => page.evaluate(() =>
    window.calls.some(c => c.name === 'voice_live_close'))).toBe(true);
  await expect(page.locator('.voice-live-overlay')).toHaveCount(0);
});

test('without a voice profile the control explains itself instead of failing on click', async ({ page }) => {
  await boot(page, { ready: false });
  const button = page.getByRole('button', { name: 'Voice conversation with Chat composer' });
  await expect(button).toBeDisabled();
  await expect(button).toHaveAttribute('title', /voice-live\.json/);
  expect(await page.evaluate(() =>
    window.calls.some(c => c.name === 'voice_live_open'))).toBe(false);
});

test('a failure to open reports the reason and starts no other provider', async ({ page }) => {
  await boot(page);
  await page.evaluate(() => {
    const invoke = window.__TAURI__.core.invoke;
    window.__TAURI__.core.invoke = (name, args) => name === 'voice_live_open'
      ? Promise.reject(new Error('Public voice setup required.'))
      : invoke(name, args);
  });
  await page.getByRole('button', { name: 'Voice conversation with Chat composer' }).click();
  await expect(page.locator('.voice-live-overlay')).toHaveCount(0);
  expect(await page.evaluate(() => window.calls.some(c =>
    ['voice_start', 'voice_local_open', 'chat_send_tools'].includes(c.name)))).toBe(false);
});
