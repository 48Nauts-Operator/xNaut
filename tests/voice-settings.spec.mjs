import { test, expect } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  await page.goto('/?stub=1');
  await page.waitForFunction(() => window.xnautOpenVoiceSettings && window.__xnautStub);
  // Boot opens Mesh after restoring workspace visibility; don't race that
  // navigation with creation of the composer whose readiness we exercise.
  await expect(page.locator('.mesh')).toBeVisible();
  await page.evaluate(() => {
    let saved = { configured: false, model: 'gpt-live-1' };
    const invoke = window.__TAURI__.core.invoke;
    window.__TAURI__.core.invoke = async (name, args) => {
      if (name === 'voice_live_settings_get') return saved;
      if (name === 'voice_live_ready') return saved.configured;
      if (name === 'voice_live_settings_save') {
        window.savedVoiceArgs = args;
        if (window.failVoiceSave) throw new Error('Cannot write profile');
        saved = { configured: true, model: args.model };
        return saved;
      }
      if (name === 'voice_live_settings_test') {
        window.voiceTestCalls = (window.voiceTestCalls || 0) + 1;
        if (window.failVoiceTest) throw new Error('Voice service rejected the session');
        return null;
      }
      return invoke(name, args);
    };
  });
});

async function openSettings(page) {
  await page.getByRole('button', { name: 'More actions', exact: true }).click();
  await page.getByRole('menuitem', { name: 'xNAUT settings', exact: true }).click();
  await page.getByRole('button', { name: 'Voice settings', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Public voice', exact: true })).toBeVisible();
}

test('settings save a masked key without persisting it in the webview, and blank preserves it', async ({ page }) => {
  await openSettings(page);
  const key = page.getByLabel('API key', { exact: true });
  await expect(key).toHaveAttribute('type', 'password');
  await expect(page.getByRole('button', { name: 'Test connection', exact: true })).toBeDisabled();
  await key.fill('test-voice-secret');
  await page.getByRole('button', { name: 'Save voice settings' }).click();
  await expect(key).toHaveValue('');
  await expect(page.locator('[data-voice-status]')).toContainText('Saved.');
  expect(await page.evaluate(() => window.savedVoiceArgs)).toEqual({ apiKey: 'test-voice-secret', model: 'gpt-live-1' });
  expect(await page.evaluate(() => JSON.stringify(localStorage))).not.toContain('test-voice-secret');
  await page.getByLabel('Voice model', { exact: true }).fill('another-voice-model');
  await expect(page.getByRole('button', { name: 'Test connection', exact: true })).toBeDisabled();
  await page.getByRole('button', { name: 'Save voice settings' }).click();
  expect(await page.evaluate(() => window.savedVoiceArgs)).toEqual({ apiKey: null, model: 'another-voice-model' });
  await page.getByRole('button', { name: 'Test connection', exact: true }).click();
  await expect(page.locator('[data-voice-status]')).toContainText('Connected.');
  await page.evaluate(() => { window.failVoiceTest = true; });
  await page.getByRole('button', { name: 'Test connection', exact: true }).click();
  await expect(page.locator('[data-voice-status]')).toContainText('Connection failed:');
});

test('unconfigured mic opens Voice settings and saving enables the same composer immediately', async ({ page }) => {
  await page.getByRole('button', { name: 'New terminal', exact: true }).click();
  await page.locator('#new-tab-menu').getByText('New Chat', { exact: true }).click();
  const mic = page.locator('.chatp-pane .voice-live-button');
  await expect(mic).toHaveAttribute('title', /Settings → Voice/);
  await mic.click();
  await expect(page.getByRole('heading', { name: 'Public voice', exact: true })).toBeVisible();
  await page.getByLabel('API key', { exact: true }).fill('test-new-user-key');
  await page.getByRole('button', { name: 'Save voice settings' }).click();
  await expect(mic).toHaveAttribute('title', 'Start a voice conversation');
  await page.getByRole('button', { name: 'Close settings', exact: true }).click();
  await mic.click();
  await expect(page.getByRole('menuitem', { name: 'STS · Speech to Speech', exact: true })).toBeVisible();
  expect(await page.evaluate(() => window.__xnautInvokes.some(i => i.cmd === 'voice_live_open'))).toBe(false);
});

test('save failure stays visible and does not claim the microphone is configured', async ({ page }) => {
  await openSettings(page);
  await page.evaluate(() => { window.failVoiceSave = true; });
  await page.getByLabel('API key', { exact: true }).fill('test-key');
  await page.getByRole('button', { name: 'Save voice settings' }).click();
  await expect(page.locator('[data-voice-status]')).toContainText('Could not save: Cannot write profile');
  await expect(page.getByRole('button', { name: 'Test connection', exact: true })).toBeDisabled();
});
