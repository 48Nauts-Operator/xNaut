import { test, expect } from '@playwright/test';
import { fileURLToPath } from 'node:url';

test.beforeEach(async ({ page }) => {
  // Exercise the real coordinator, dictation handler and overlay. Only native
  // audio/IPC is mocked; no microphone, model download or API call in CI.
  await page.route('**/voice-test.html', (route) => route.fulfill({
    contentType: 'text/html',
    body: '<main><div id="first"><textarea aria-label="Chat"></textarea><button id="mic1">Dictate Chat</button></div><textarea id="other" aria-label="Agent"></textarea><button id="mic2">Dictate Agent</button></main>',
  }));
  await page.goto('/voice-test.html');
  await page.addStyleTag({ path: fileURLToPath(new URL('../src/css/voice-overlay.css', import.meta.url)) });
  await page.evaluate(() => {
    window.calls = [];
    window.__TAURI__ = { core: { invoke: async (name, args) => {
      window.calls.push({ name, args });
      if (name === 'voice_model_ready') return true;
      if (name === 'voice_stop') return new Promise((resolve) => { window.resolveTranscript = resolve; });
      return null;
    } } };
  });
  for (const file of ['voice-session', 'voice-overlay', 'voice-dictate']) {
    await page.addScriptTag({ path: fileURLToPath(new URL(`../src/js/${file}.js`, import.meta.url)) });
  }
  await page.evaluate(() => {
    window.xnautAttachDictation(document.querySelector('#mic1'), (text) => {
      window.xnautDictationAppend(document.querySelector('#first textarea'), text);
    }, 'Chat destination');
    window.xnautAttachDictation(document.querySelector('#mic2'), (text) => {
      window.xnautDictationAppend(document.querySelector('#other'), text);
    }, 'Agent destination');
  });
});

test('focus changes do not reroute a transcript; both composers share one lease', async ({ page }) => {
  await page.locator('#mic1').click();
  await expect(page.locator('.voice-overlay')).toContainText('Chat destination');
  await expect(page.locator('#mic1')).toHaveAttribute('aria-pressed', 'true');
  await page.locator('#mic2').click();
  await page.locator('#other').focus();
  expect(await page.evaluate(() => window.calls.filter((c) => c.name === 'voice_start').length)).toBe(1);
  await page.getByRole('button', { name: 'Finish dictation' }).click();
  await expect(page.getByRole('status')).toHaveText('Microphone off · transcribing…');
  await page.evaluate(() => window.resolveTranscript({ text: 'Pinned text' }));
  await expect(page.getByRole('textbox', { name: 'Chat' })).toHaveValue('Pinned text');
  await expect(page.getByRole('textbox', { name: 'Agent' })).toHaveValue('');
  await expect(page.locator('.voice-overlay')).toHaveCount(0);
  const calls = await page.evaluate(() => window.calls);
  const start = calls.find((c) => c.name === 'voice_start');
  const stop = calls.find((c) => c.name === 'voice_stop');
  expect(stop.args.captureId).toBe(start.args.captureId);
});

test('Escape during transcription drops the late result and permits another session', async ({ page }) => {
  await page.locator('#mic1').click();
  await page.getByRole('button', { name: 'Finish dictation' }).click();
  await page.keyboard.press('Escape');
  await expect(page.locator('.voice-overlay')).toHaveCount(0);
  await page.locator('#mic2').click();
  await page.evaluate(() => window.resolveTranscript({ text: 'Cancelled text' }));
  await expect(page.getByRole('textbox', { name: 'Chat' })).toHaveValue('');
  await expect(page.getByRole('textbox', { name: 'Agent' })).toHaveValue('');
  await expect(page.locator('.voice-overlay')).toContainText('Agent destination');
  // A stale finish handler must not end the new owner's recording.
  await expect(page.locator('#mic2')).toHaveAttribute('aria-pressed', 'true');
});

test('removing the owning composer cancels capture without starting transcription', async ({ page }) => {
  await page.locator('#mic1').click();
  await page.evaluate(() => document.querySelector('#first').remove());
  await expect(page.locator('.voice-overlay')).toHaveCount(0);
  await expect.poll(() => page.evaluate(() => window.calls.filter((c) => c.name === 'voice_cancel').length)).toBe(1);
  expect(await page.evaluate(() => window.calls.some((c) => c.name === 'voice_stop'))).toBe(false);
});

test('transcription errors are visible and a cancelled error can be retried', async ({ page }) => {
  await page.evaluate(() => {
    const invoke = window.__TAURI__.core.invoke;
    window.__TAURI__.core.invoke = (name, args) => name === 'voice_stop'
      ? Promise.reject(new Error('Local speech engine unavailable')) : invoke(name, args);
  });
  await page.locator('#mic1').click();
  await page.getByRole('button', { name: 'Finish dictation' }).click();
  await expect(page.getByRole('status')).toContainText('Local speech engine unavailable');
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  await page.locator('#mic2').click();
  await expect(page.locator('.voice-overlay')).toContainText('Agent destination');
});

test('cancel before native startup returns also retires the late microphone', async ({ page }) => {
  await page.evaluate(() => {
    const invoke = window.__TAURI__.core.invoke;
    window.__TAURI__.core.invoke = async (name, args) => {
      const result = await invoke(name, args);
      if (name === 'voice_start') await new Promise((resolve) => { window.resolveStart = resolve; });
      return result;
    };
  });
  await page.locator('#mic1').click();
  await expect(page.getByRole('status')).toHaveText('Starting microphone…');
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect.poll(() => page.evaluate(() => window.calls.filter((c) => c.name === 'voice_cancel').length)).toBe(1);
  await page.evaluate(() => window.resolveStart());
  await expect.poll(() => page.evaluate(() => window.calls.filter((c) => c.name === 'voice_cancel').length)).toBe(2);
  await expect(page.locator('.voice-overlay')).toHaveCount(0);
  await expect(page.locator('#mic1')).toHaveAttribute('aria-pressed', 'false');
});

test('first-use model download is disclosed in the overlay', async ({ page }) => {
  await page.evaluate(() => {
    const invoke = window.__TAURI__.core.invoke;
    window.__TAURI__.core.invoke = (name, args) => name === 'voice_model_ready'
      ? Promise.resolve(false) : invoke(name, args);
  });
  await page.locator('#mic1').click();
  await expect(page.locator('.voice-overlay')).toContainText('First transcription downloads a speech model (~148 MB).');
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  expect(await page.evaluate(() => window.calls.some((c) => c.name === 'voice_stop'))).toBe(false);
});
