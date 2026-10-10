// Update UI contract with Tauri 2's staged API. No production payload is
// installed by these tests; native discovery/signing has separate smoke gates.
import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';
const APP_VERSION = JSON.parse(await readFile(new URL('../src-tauri/tauri.conf.json', import.meta.url), 'utf8')).version;
const RELEASE_API = 'https://api.github.com/repos/48Nauts-Operator/xNaut/releases/latest';
const MB = 1048576;

async function fixture(page, options = {}) {
  await page.route('**/__stub.js', async route => {
    const response = await route.fetch();
    const script = ({ current, plugin = true, process = true, path = '/Applications/xNAUT.app/Contents/Resources' }) => {
      const state = { checks: 0, downloads: 0, installs: 0, restarts: 0, closed: [], options: {}, release: '99.0.0' };
      window.__updates = state;
      window.xnautUpdateStallMs = 400;
      window.__TAURI__.app.getVersion = async () => { if (current === 'error') throw new Error('Version unavailable'); return current; };
      window.__TAURI__.path = { ...window.__TAURI__.path, resourceDir: async () => path };
      if (process) window.__TAURI__.process = { relaunch: async () => { state.restarts++; if (state.restartError) throw new Error(state.restartError); } };
      if (!plugin) return;
      window.__TAURI__.updater = { check: async options => {
        state.checks++; state.options.check = options;
        if (state.checkError) throw new Error(state.checkError);
        if (state.holdCheck) await new Promise(resolve => { state.resolveCheck = resolve; });
        const id = state.checks;
        return state.release ? {
          version: state.release, body: 'Changes for this release. <img src=x onerror="window.unsafeNotes=true">',
          close: async () => { state.closed.push(id); },
          download: (emit, options) => {
            state.downloads++; state.options.download = options; state.emit = emit;
            return new Promise((resolve, reject) => { state.resolveDownload = resolve; state.rejectDownload = why => reject(new Error(why)); });
          },
          install: async () => {
            state.installs++;
            if (state.installError) throw new Error(state.installError);
            if (state.holdInstall) await new Promise(resolve => { state.resolveInstall = resolve; });
          },
        } : null;
      } };
    };
    await route.fulfill({ response, body: `${await response.text()}\n(${script.toString()})(${JSON.stringify({ current: APP_VERSION, ...options })});` });
  });
  await page.route(RELEASE_API, route => route.fulfill({ json: { tag_name: 'v99.0.0', body: 'Fallback release notes' } }));
  await page.goto('/?stub=1');
  await page.waitForFunction(() => !!document.getElementById('btn-more-menu')?.onclick);
}
async function open(page) {
  await page.getByRole('button', { name: 'More actions', exact: true }).click();
  await page.getByRole('menuitem', { name: 'Updates…', exact: true }).click();
  await expect(page.getByRole('dialog', { name: 'Updates', exact: true })).toBeVisible();
  return page.locator('#update-panel');
}
const status = page => page.locator('#update-panel [data-status]');
const action = page => page.locator('#update-panel [data-primary]');
async function ready(page) {
  await action(page).click();
  await page.evaluate(() => { window.__updates.emit({ event: 'Finished' }); window.__updates.resolveDownload(); });
  await expect(action(page)).toHaveText('Install and restart now');
}

test('automatic discovery is quiet, leaves controls clickable, and never downloads', async ({ page }) => {
  await fixture(page);
  await expect(page.locator('#btn-updates')).toBeVisible({ timeout: 6000 });
  await expect(page.locator('#update-panel')).toHaveCount(0);
  expect(await page.evaluate(() => window.__updates.downloads)).toBe(0);
  const covered = await page.evaluate(() => [...document.querySelectorAll('.top-bar button')].filter(el => {
    const r = el.getBoundingClientRect();
    const hit = r.width && r.height && document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
    return hit && !el.contains(hit) && !hit.contains(el);
  }).map(el => el.id));
  expect(covered).toEqual([]);
  await page.locator('#btn-updates').click();
  await expect(status(page)).toContainText('99.0.0 is available');
});

test('manual checks remain reachable when current, offline or version lookup fails', async ({ page }) => {
  await fixture(page);
  await page.evaluate(version => { window.__updates.release = version; }, APP_VERSION);
  await open(page);
  await expect(status(page)).toHaveText('You’re up to date.');
  await expect(page.locator('#btn-updates')).toBeHidden();
  await page.evaluate(() => { window.__updates.checkError = 'Network unavailable'; });
  await page.getByRole('button', { name: 'Check now', exact: true }).click();
  await expect(status(page)).toContainText('Network unavailable');
  await page.evaluate(() => { window.__updates.checkError = ''; window.__updates.release = '99.0.0'; });
  await page.getByRole('button', { name: 'Check now', exact: true }).click();
  await expect(status(page)).toContainText('99.0.0 is available');
  expect(await page.evaluate(() => window.__updates.options.check.timeout)).toBe(20000);
});

test('unknown installed version never offers an update', async ({ page }) => {
  await fixture(page, { current: 'error' }); await open(page);
  await expect(status(page)).toContainText('Version unavailable');
  await expect(action(page)).toBeHidden();
  expect(await page.evaluate(() => window.__updates.checks)).toBe(0);
});

for (const path of ['/tmp/.worktrees/test/xNAUT.app', '/tmp/target/debug', '/tmp/target/release']) {
  test(`development bundle is never updated: ${path}`, async ({ page }) => {
    await fixture(page, { path }); await open(page);
    await expect(status(page)).toContainText('disabled for development');
    expect(await page.evaluate(() => window.__updates.checks)).toBe(0);
    await expect(action(page)).toBeHidden();
  });
}

test('fallback uses the CORS-enabled API, safe text notes and an honest external download action', async ({ page }) => {
  await fixture(page, { plugin: false }); await open(page);
  await expect(action(page)).toHaveText('Download from website');
  await page.getByText('What’s new', { exact: true }).click();
  await expect(page.locator('[data-notes]')).toHaveText('Fallback release notes');
  await page.evaluate(() => { window.__TAURI__.shell = { open: async url => { window.__openedRelease = url; } }; });
  await action(page).click();
  expect(await page.evaluate(() => window.__openedRelease)).toBe('https://github.com/48Nauts-Operator/xNaut/releases/latest');
});

test('progress survives closing and reopening; Finished is verification, not installation', async ({ page }) => {
  await fixture(page); await open(page); await action(page).click();
  expect(await page.evaluate(() => window.__updates.options.download.timeout)).toBe(15 * 60 * 1000);
  await page.evaluate(MB => {
    window.__updates.emit({ event: 'Started', data: { contentLength: 26 * MB } });
    window.__updates.emit({ event: 'Progress', data: { chunkLength: 7 * MB } });
  }, MB);
  await expect(status(page)).toContainText('7.0 MB / 26.0 MB');
  await page.getByRole('button', { name: 'Close updates', exact: true }).click();
  await page.locator('#btn-updates').click();
  await expect(status(page)).toContainText('7.0 MB / 26.0 MB');
  await page.evaluate(() => window.__updates.emit({ event: 'Finished' }));
  await expect(status(page)).toHaveText('Verifying the downloaded update…');
  expect(await page.evaluate(() => window.__updates.installs)).toBe(0);
  await page.evaluate(() => window.__updates.resolveDownload());
  await expect(status(page)).toContainText('downloaded and verified');
  expect(await page.evaluate(() => [window.__updates.installs, window.__updates.restarts])).toEqual([0, 0]);
});

test('stall does not manufacture failure or allow overlapping requests; actual failure can retry', async ({ page }) => {
  await fixture(page); await open(page); await action(page).click();
  await expect(status(page)).toContainText('No response yet');
  await expect(action(page)).toBeDisabled();
  await page.evaluate(() => window.__updates.emit({ event: 'Progress', data: { chunkLength: 1048576 } }));
  await expect(status(page)).toContainText('Downloading 1.0 MB');
  await page.evaluate(() => window.__updates.rejectDownload('TLS connection failed'));
  await expect(status(page)).toContainText('TLS connection failed');
  await expect(action(page)).toHaveText('Retry download');
  await action(page).click();
  expect(await page.evaluate(() => window.__updates.downloads)).toBe(2);
});

test('signature rejection after Finished is never presented as a verified or installed update', async ({ page }) => {
  await fixture(page); await open(page); await action(page).click();
  await page.evaluate(() => { window.__updates.emit({ event: 'Finished' }); window.__updates.rejectDownload('Signature mismatch'); });
  await expect(status(page)).toContainText('Download or verification failed: Signature mismatch');
  expect(await page.evaluate(() => [window.__updates.installs, window.__updates.restarts])).toEqual([0, 0]);
});

test('explicit install waits for saved conversations and only then installs and restarts', async ({ page }) => {
  await fixture(page); await open(page); await ready(page);
  await page.evaluate(() => {
    const invoke = window.__TAURI__.core.invoke;
    window.__TAURI__.core.invoke = async (cmd, args) => {
      if (cmd === 'conversation_store_put') await new Promise(resolve => { window.__finishSave = resolve; });
      return invoke(cmd, args);
    };
    window.xnautConversationStorage.setItem('xnaut-chat-history:pending', '[{"role":"user","content":"Keep this"}]');
  });
  await action(page).click();
  await expect(status(page)).toContainText('Checking saved conversations');
  expect(await page.evaluate(() => [window.__updates.installs, window.__updates.restarts])).toEqual([0, 0]);
  await page.evaluate(() => window.__finishSave());
  await expect.poll(() => page.evaluate(() => window.__updates.restarts)).toBe(1);
  expect(await page.evaluate(() => window.__updates.installs)).toBe(1);
  expect(await page.evaluate(() => localStorage.getItem('xnaut-conversation-pending:xnaut-chat-history:pending'))).toBeNull();
});

test('failed native saves block installation and preserve the outbox', async ({ page }) => {
  await fixture(page); await open(page); await ready(page);
  await page.evaluate(async () => {
    window.__xnautStub.conversation_store_put = { __reject: 'Disk full' };
    window.xnautConversationStorage.setItem('xnaut-chat-history:failure', 'Keep this');
    await window.xnautConversationStorage.flush();
  });
  await action(page).click();
  await expect(status(page)).toContainText('Restart paused: Conversation changes are not saved');
  expect(await page.evaluate(() => [window.__updates.installs, window.__updates.restarts])).toEqual([0, 0]);
  expect(await page.evaluate(() => localStorage.getItem('xnaut-conversation-pending:xnaut-chat-history:failure'))).toContain('Keep this');
});

test('install failure retries the staged payload; restart failure never reinstalls', async ({ page }) => {
  await fixture(page); await open(page); await ready(page);
  await page.evaluate(() => { window.__updates.installError = 'Cannot replace app'; });
  await action(page).click();
  await expect(status(page)).toContainText('Installation failed: Cannot replace app');
  expect(await page.evaluate(() => window.__updates.restarts)).toBe(0);
  await page.evaluate(() => { window.__updates.installError = ''; window.__updates.restartError = 'Process unavailable'; });
  await action(page).click();
  await expect(status(page)).toContainText('restart failed: Process unavailable');
  await expect(action(page)).toHaveText('Retry restart');
  await page.evaluate(() => { window.__updates.restartError = ''; });
  await action(page).click();
  expect(await page.evaluate(() => [window.__updates.downloads, window.__updates.installs, window.__updates.restarts])).toEqual([1, 2, 2]);
});

test('missing relaunch plugin gives accurate manual completion instructions', async ({ page }) => {
  await fixture(page, { process: false }); await open(page); await ready(page); await action(page).click();
  await expect(status(page)).toContainText('Quit and reopen xNAUT to finish');
  await expect(action(page)).toBeHidden();
});

test('checks serialize; download and staged update cannot be replaced by wake or reconnect', async ({ page }) => {
  await fixture(page);
  await page.evaluate(() => { window.__updates.holdCheck = true; });
  await open(page);
  await page.evaluate(() => { window.dispatchEvent(new Event('online')); window.dispatchEvent(new Event('focus')); });
  expect(await page.evaluate(() => window.__updates.checks)).toBe(1);
  await page.evaluate(() => { window.__updates.holdCheck = false; window.__updates.resolveCheck(); });
  await expect(action(page)).toHaveText('Download update');
  await action(page).click();
  await page.evaluate(() => { window.dispatchEvent(new Event('online')); window.dispatchEvent(new Event('focus')); });
  expect(await page.evaluate(() => window.__updates.checks)).toBe(1);
  await page.evaluate(() => window.__updates.resolveDownload());
  await expect(action(page)).toHaveText('Install and restart now');
  await page.evaluate(() => window.dispatchEvent(new Event('online')));
  expect(await page.evaluate(() => window.__updates.checks)).toBe(1);
  await expect(page.getByRole('button', { name: 'Check now', exact: true })).toBeDisabled();
});

test('rechecks close replaced native resources and render release notes as text', async ({ page }) => {
  await fixture(page); await open(page);
  await expect(action(page)).toHaveText('Download update');
  await page.getByText('What’s new', { exact: true }).click();
  await expect(page.locator('[data-notes]')).toContainText('<img');
  expect(await page.evaluate(() => window.unsafeNotes)).toBeUndefined();
  await page.getByRole('button', { name: 'Check now', exact: true }).click();
  await expect.poll(() => page.evaluate(() => window.__updates.closed)).toEqual([1]);
  await page.evaluate(() => { window.__updates.release = null; });
  await page.getByRole('button', { name: 'Check now', exact: true }).click();
  await expect(status(page)).toHaveText('You’re up to date.');
  expect(await page.evaluate(() => window.__updates.closed)).toEqual([1, 2]);
});

test('snooze survives reload without hiding Updates from the menu; a newer version appears', async ({ page }) => {
  await fixture(page); await open(page);
  await page.getByRole('button', { name: 'Remind me tomorrow', exact: true }).click();
  await expect(page.locator('#btn-updates')).toBeHidden();
  await page.reload();
  await page.waitForFunction(() => !!document.getElementById('btn-more-menu')?.onclick);
  await open(page); await expect(status(page)).toContainText('99.0.0 is available');
  await expect(page.locator('#btn-updates')).toBeHidden();
  await page.evaluate(() => { window.__updates.release = '99.0.1'; });
  await page.getByRole('button', { name: 'Check now', exact: true }).click();
  await expect(status(page)).toContainText('99.0.1 is available');
  await expect(page.locator('#btn-updates')).toBeVisible();
});

test('automatic checks are opt-out and manual checks still work after reload', async ({ page }) => {
  await fixture(page); await open(page);
  await page.getByLabel('Automatically check for updates', { exact: true }).uncheck();
  await page.reload(); await page.waitForTimeout(3600);
  expect(await page.evaluate(() => window.__updates.checks)).toBe(0);
  await open(page); await expect(status(page)).toContainText('99.0.0 is available');
  await expect(page.getByLabel('Automatically check for updates', { exact: true })).not.toBeChecked();
});

test('reconnect retries a failed check and normal focus is throttled', async ({ page }) => {
  await fixture(page);
  await page.evaluate(() => { window.__updates.checkError = 'Offline'; });
  await open(page); await expect(status(page)).toContainText('Offline');
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  expect(await page.evaluate(() => window.__updates.checks)).toBe(1);
  await page.evaluate(() => { window.__updates.checkError = ''; window.dispatchEvent(new Event('online')); });
  await expect(status(page)).toContainText('99.0.0 is available');
  expect(await page.evaluate(() => window.__updates.checks)).toBe(2);
});

test('checks again after six hours, without checking on every focus event', async ({ page }) => {
  await fixture(page); await open(page);
  await expect(status(page)).toContainText('99.0.0 is available');
  await page.clock.setFixedTime(Date.now() + 6 * 60 * 60 * 1000 + 1000);
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await expect.poll(() => page.evaluate(() => window.__updates.checks)).toBe(2);
  await expect(status(page)).toContainText('99.0.0 is available');
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  expect(await page.evaluate(() => window.__updates.checks)).toBe(2);
});

test('invalid release metadata is reported, not mistaken for up to date', async ({ page }) => {
  await fixture(page);
  await page.evaluate(() => { window.__updates.release = 'not-a-version'; });
  await open(page);
  await expect(status(page)).toContainText('invalid stable version');
  await expect(action(page)).toBeHidden();
  expect(await page.evaluate(() => window.__updates.closed)).toEqual([1]);
});

test('installation is serialized even if the action is triggered twice', async ({ page }) => {
  await fixture(page); await open(page); await ready(page);
  await page.evaluate(() => { window.__updates.holdInstall = true; });
  await action(page).click();
  await expect(status(page)).toContainText('Installing xNAUT');
  await page.evaluate(() => document.querySelector('[data-primary]').dispatchEvent(new MouseEvent('click')));
  expect(await page.evaluate(() => window.__updates.installs)).toBe(1);
  await page.evaluate(() => window.__updates.resolveInstall());
  await expect.poll(() => page.evaluate(() => window.__updates.restarts)).toBe(1);
  await expect(action(page)).toBeHidden();
});

test('conversation writes queued while installation finishes are saved before relaunch', async ({ page }) => {
  await fixture(page); await open(page); await ready(page);
  await page.evaluate(() => { window.__updates.holdInstall = true; });
  await action(page).click();
  await expect(status(page)).toContainText('Installing xNAUT');
  await page.evaluate(async () => {
    window.__xnautStub.conversation_store_put = { __reject: 'Disk full' };
    window.xnautConversationStorage.setItem('xnaut-chat-history:late', 'Retain late write');
    await window.xnautConversationStorage.flush();
    window.__updates.resolveInstall();
  });
  await expect(status(page)).toContainText('Update installed; restart failed: Conversation changes are not saved');
  expect(await page.evaluate(() => window.__updates.restarts)).toBe(0);
});

test('update dialog is centred, readable, and returns keyboard focus on Escape', async ({ page }) => {
  await fixture(page); await open(page);
  await expect(action(page)).toHaveText('Download update');
  const visual = await page.evaluate(() => {
    const rect = document.getElementById('update-panel').getBoundingClientRect();
    const style = getComputedStyle(document.querySelector('#update-panel [data-primary]'));
    const luminance = value => value.match(/[\d.]+/g).slice(0, 3).map(Number).map(n => n / 255).map(v => v <= .04045 ? v / 12.92 : ((v + .055) / 1.055) ** 2.4).reduce((sum, v, i) => sum + v * [.2126, .7152, .0722][i], 0);
    const colors = [luminance(style.color), luminance(style.backgroundColor)].sort((a, b) => b - a);
    return { offsetX: Math.abs(rect.left + rect.width / 2 - innerWidth / 2), offsetY: Math.abs(rect.top + rect.height / 2 - innerHeight / 2), contrast: (colors[0] + .05) / (colors[1] + .05) };
  });
  expect(visual.offsetX).toBeLessThan(2); expect(visual.offsetY).toBeLessThan(2);
  expect(visual.contrast).toBeGreaterThanOrEqual(4.5);
  await page.keyboard.press('Escape');
  await expect(page.locator('#update-panel')).not.toBeVisible();
  await expect(page.getByRole('button', { name: 'More actions', exact: true })).toBeFocused();
});
