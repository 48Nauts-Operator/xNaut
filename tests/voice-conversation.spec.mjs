import { test, expect } from '@playwright/test';
import { fileURLToPath } from 'node:url';

test.beforeEach(async ({ page }) => {
  await page.route('**/voice-chat.html', route => route.fulfill({ contentType: 'text/html', body: '<main id="chat"></main>' }));
  await page.goto('/voice-chat.html');
  await page.evaluate(() => {
    window.calls = [];
    window.answer = 'The complete written answer.\n\n```sh\nsecret_tool_payload\n```';
    window.__TAURI__ = {
      event: { listen: async () => () => {} },
      core: { invoke: async (name, args) => {
        window.calls.push({ name, args });
        if (name === 'settings_get') return { llm: {}, categories: [], forges: [] };
        if (name === 'agent_list') return [];
        if (name === 'voice_local_transcribe') return { text: 'Please explain the change.' };
        if (name === 'chat_send_tools') return window.answer;
        if (name === 'voice_local_speak') return new Promise(resolve => { window.resolveSpeech = resolve; });
        if (name === 'voice_local_interrupt' || name === 'voice_local_close') window.resolveSpeech?.();
        return null;
      } },
    };
  });
  for (const file of ['voice-session', 'voice-overlay', 'voice-dictate', 'voice-conversation', 'voice-live', 'chat-panel']) {
    await page.addScriptTag({ path: fileURLToPath(new URL(`../src/js/${file}.js`, import.meta.url)) });
  }
  await page.evaluate(async () => {
    window.chatEntry = await window.xnautCreateChatPane('test', document.querySelector('#chat'), { chatKey: 'voice-integration' });
  });
});

test('local conversation uses the real Chat send/history path and reads only presentation prose', async ({ page }) => {
  await page.getByRole('button', { name: 'Talk to Chat composer' }).click();
  await expect(page.getByRole('button', { name: 'Finish & send' })).toBeEnabled();
  await page.getByRole('button', { name: 'Finish & send' }).click();
  await expect.poll(() => page.evaluate(() => window.calls.filter(c => c.name === 'chat_send_tools').length)).toBe(1);
  await expect.poll(() => page.evaluate(() => window.calls.filter(c => c.name === 'voice_local_speak').length)).toBe(1);
  const result = await page.evaluate(() => ({
    history: window.chatEntry.history,
    speech: window.calls.find(c => c.name === 'voice_local_speak').args.text,
    commands: window.calls.map(c => c.name),
  }));
  expect(result.history.at(-2).content).toBe('Please explain the change.');
  expect(result.history.at(-1).content).toContain('secret_tool_payload');
  expect(result.speech).toContain('The complete written answer.');
  expect(result.speech).not.toContain('secret_tool_payload');
  expect(result.commands).not.toContain('voice_stop'); // no Whisper or download
  await page.getByRole('button', { name: 'Stop speaking' }).click();
  await expect.poll(() => page.evaluate(() => window.calls.some(c => c.name === 'voice_local_interrupt'))).toBe(true);
  await expect(page.getByRole('button', { name: 'Speak again' })).toBeEnabled();
  await page.getByRole('button', { name: 'Speak again' }).click();
  expect(await page.evaluate(() => new Set(window.calls.filter(c => c.name === 'voice_start').map(c => c.args.captureId)).size)).toBe(2);
});

test('Silent keeps the full written answer without invoking synthesis', async ({ page }) => {
  await page.getByRole('button', { name: 'Talk to Chat composer' }).click();
  await page.getByRole('combobox', { name: 'Spoken output' }).selectOption('silent');
  await page.getByRole('button', { name: 'Finish & send' }).click();
  await expect.poll(() => page.evaluate(() => window.chatEntry.history.length)).toBe(2);
  expect(await page.evaluate(() => window.calls.some(c => c.name === 'voice_local_speak'))).toBe(false);
});

test('service failure preserves typed input and does not start another provider', async ({ page }) => {
  await page.locator('.chatp-input').fill('Keep my draft');
  await page.evaluate(() => {
    const invoke = window.__TAURI__.core.invoke;
    window.__TAURI__.core.invoke = (name, args) => name === 'voice_local_open'
      ? Promise.reject(new Error('Local speech service unavailable')) : invoke(name, args);
  });
  await page.getByRole('button', { name: 'Talk to Chat composer' }).click();
  await expect(page.getByRole('status')).toContainText('Local speech service unavailable');
  await expect(page.locator('.chatp-input')).toHaveValue('Keep my draft');
  expect(await page.evaluate(() => window.calls.some(c => ['voice_start', 'voice_model_ready', 'chat_send_tools'].includes(c.name)))).toBe(false);
});

test('changing the conversation key ends the old voice destination', async ({ page }) => {
  await page.getByRole('button', { name: 'Talk to Chat composer' }).click();
  await page.evaluate(() => window.chatEntry.setChatKey('different-project'));
  await expect(page.locator('.voice-overlay')).toHaveCount(0);
  await expect.poll(() => page.evaluate(() => window.calls.some(c => c.name === 'voice_local_close'))).toBe(true);
});

test('an action payload is displayed by Chat but is never sent to the speaker', async ({ page }) => {
  await page.evaluate(() => { window.answer = '{"action":"unknown_action","secret":"tool arguments"}'; });
  await page.getByRole('button', { name: 'Talk to Chat composer' }).click();
  await page.getByRole('button', { name: 'Finish & send' }).click();
  await expect.poll(() => page.evaluate(() => window.chatEntry.busy)).toBe(false);
  expect(await page.evaluate(() => window.calls.some(c => c.name === 'voice_local_speak'))).toBe(false);
});

test('an incomplete action payload is not treated as spoken prose', async ({ page }) => {
  await page.evaluate(() => { window.answer = '{"action":"unknown_action","secret":"partial tool arguments'; });
  await page.getByRole('button', { name: 'Talk to Chat composer' }).click();
  await page.getByRole('button', { name: 'Finish & send' }).click();
  await expect.poll(() => page.evaluate(() => window.chatEntry.history.length)).toBe(2);
  expect(await page.evaluate(() => window.calls.some(c => c.name === 'voice_local_speak'))).toBe(false);
});
