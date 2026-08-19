// An SSH session has to carry the key path out and the shell's output back.
//
// Everything in this file was broken at once (XNAUT-200), and each half hid the
// other: the profile editor sent `privateKey` while the backend read `keyPath`,
// so key auth always failed; the saved session was dropped the moment it was
// made; write_to_ssh was a TODO; and nothing emitted ssh-output. On top of that
// the modal was opened without ever rendering the profile list, so there was no
// Connect button to press in the first place.
//
// The backend half is proved by cargo (ssh::tests, including a live server run
// behind --ignored). This is the frontend half, driven through the real menu,
// the real modal and the real xterm instance, with the Tauri bridge stubbed.

import { test, expect } from '@playwright/test';

const KEY_PATH = '/tmp/xnaut-test/id_ed25519';
const PROFILE = {
  id: 'ssh-1', name: 'Cosmos', host: 'cosmos.example', port: 2222,
  username: 'andre', authMethod: 'privateKey', keyPath: KEY_PATH,
};

test('an SSH profile connects, types, and shows what the far end said', async ({ page }) => {
  await page.addInitScript((profile) => {
    localStorage.setItem('xnaut-ssh-profiles', JSON.stringify([profile]));
  }, PROFILE);
  await page.goto('/?stub=1');
  await page.waitForTimeout(900);
  await page.evaluate(() => { window.__xnautStub.create_ssh_session = { session_id: 'S1' }; });

  // The menu entry, not a direct call: opening the modal without rendering the
  // list is what left the feature with no way in.
  await page.click('#btn-more-menu');
  await page.click('[data-action="ssh"]');
  const connect = page.locator('#ssh-modal [data-action="connect"]');
  await expect(connect).toBeVisible();

  await connect.click();
  await page.waitForTimeout(400);

  const config = await page.evaluate(() =>
    window.__xnautInvokes.find((i) => i.cmd === 'create_ssh_session')?.args?.config);
  expect(config).toBeTruthy();
  expect(config.keyPath).toBe('/tmp/xnaut-test/id_ed25519');
  expect(config.privateKey).toBeUndefined();

  // The channel opens at a placeholder 80x24, so the real size has to follow.
  const resize = await page.evaluate(() =>
    window.__xnautInvokes.find((i) => i.cmd === 'resize_ssh')?.args);
  expect(resize?.sessionId).toBe('S1');
  expect(resize.cols).toBeGreaterThan(0);

  // Output arrives base64 encoded, same contract as terminal-output.
  const seen = await page.evaluate(async () => {
    const data = btoa(unescape(encodeURIComponent('remote says: ✓ ok')));
    window.__xnautEmit('ssh-output-S1', { sessionId: 'S1', data });
    await new Promise((r) => setTimeout(r, 300));
    const term = window.xnaut.tabs.find((t) => t.isSSH).terminals[0].term;
    const buffer = term.buffer.active;
    let text = '';
    for (let i = 0; i < buffer.length; i += 1) {
      text += `${buffer.getLine(i)?.translateToString(true) || ''}\n`;
    }
    return text;
  });
  // The tick proves the bytes were decoded as UTF-8 rather than latin-1.
  expect(seen).toContain('remote says: ✓ ok');

  // And a keystroke has to reach the backend rather than being swallowed.
  const typed = await page.evaluate(async () => {
    window.xnaut.tabs.find((t) => t.isSSH).terminals[0].term.paste('uptime\r');
    await new Promise((r) => setTimeout(r, 200));
    return window.__xnautInvokes.filter((i) => i.cmd === 'write_to_ssh').map((i) => i.args);
  });
  expect(typed).toEqual([{ sessionId: 'S1', data: 'uptime\r' }]);
});
