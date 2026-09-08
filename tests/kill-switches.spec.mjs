// XNAUT-231 shipped four kill switches with no surface: they could only be
// flipped by editing kill-switches.json, and nothing in the app said a layer
// was off. NautBot then spent a run failing every ticket write on "the
// read_only kill-switch is engaged" while the owner could not find it
// (2026-09-09). This proves the panel is mounted, shows the engaged state,
// and that a flip reaches kill_switches_set.
import { test, expect } from '@playwright/test';

test('the guardrails page shows the kill switches and a flip reaches the backend', async ({ page }) => {
  let state = { freeze_merges: false, read_only: true, approve_everything: false, quarantined: [] };
  const sets = [];

  await page.addInitScript(() => {
    window.__TAURI__ = {
      core: {
        invoke: async (cmd, args) => {
          if (cmd === 'kill_switches_get') return window.__killSwitches;
          if (cmd === 'kill_switches_set') {
            window.__killSets.push(args.switches);
            window.__killSwitches = args.switches;
            return args.switches;
          }
          return null;
        },
      },
      event: { listen: async () => () => {} },
    };
    window.__killSwitches = { freeze_merges: false, read_only: true, approve_everything: false, quarantined: [] };
    window.__killSets = [];
  });

  await page.goto('/index.html');
  await page.waitForFunction(() => typeof window.xnautRenderKillSwitches === 'function');

  await page.evaluate(() => {
    const host = document.createElement('div');
    host.id = 'kill-switches-host';
    document.body.appendChild(host);
    return window.xnautRenderKillSwitches(host);
  });

  const host = page.locator('#kill-switches-host');
  await expect(host, 'the panel never rendered').toContainText('Kill switches');
  await expect(host, 'an engaged switch must say so').toContainText('ENGAGED');
  await expect(host).toContainText('1 engaged');

  const readOnly = host.locator('[data-switch="read_only"]');
  await expect(readOnly, 'the checkbox must reflect the stored state').toBeChecked();
  await readOnly.uncheck();

  await expect(host, 'lifting it must be confirmed, not silent').toContainText('Saved');
  const written = await page.evaluate(() => window.__killSets);
  expect(written.length, 'the flip never reached kill_switches_set').toBe(1);
  expect(written[0].read_only, 'read_only was not lifted').toBe(false);
  expect(written[0].freeze_merges, 'an unrelated switch was changed').toBe(false);
  await expect(host, 'the panel must re-read the saved state').toContainText('All off');
});
