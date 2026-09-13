// The core team's one surface (XNAUT-357).
//
// The loop is experimental and it is OFF, so the only way it ever runs is the
// switch on this pane. A switch nothing renders is a feature that exists in
// the source and nowhere else, which this codebase has now shipped twice
// (XNAUT-189's policy editor, the roster panel's missing global).
//
// The second half is the one that would actually cost something: the pane
// edits ONE key of a settings object that also carries API keys, provider
// endpoints and the MCP token. Writing a fresh object instead of editing the
// read-back one is how a save on this page silently strips the rest.
import { test, expect } from '@playwright/test';

const SETTINGS = {
  project_root: '/tmp/x',
  mcp_token: 'keep-me',
  llm_providers: [{ name: 'perplexity', endpoint: 'https://api.perplexity.ai', enabled: true }],
  core_team: {
    enabled: false,
    project: 'CORE',
    decision_project: 'xnaut',
    topics: ['agent harnesses'],
    poc_threshold: 70,
    beat_days: 7,
    poc_minutes: 90,
  },
};

const STATUS = {
  enabled: false,
  project: 'CORE',
  threshold: 70,
  beat_days: 7,
  topics: ['agent harnesses'],
  blocked: ['the core team is off in Settings — it is experimental'],
  researcher: 'researcher',
  reviewer: 'reviewer',
  reviewer_provider: 'anthropic',
  last_beat_ms: 0,
};

async function mount(page) {
  await page.addInitScript(([settings, status]) => {
    window.__settings = settings;
    window.__saved = [];
    window.__scans = 0;
    window.__TAURI__ = {
      core: {
        invoke: async (cmd, args) => {
          if (cmd === 'settings_get') return JSON.parse(JSON.stringify(window.__settings));
          if (cmd === 'settings_set') {
            window.__saved.push(args.settings);
            window.__settings = args.settings;
            return null;
          }
          if (cmd === 'core_team_status') return status;
          if (cmd === 'core_team_scan') {
            window.__scans += 1;
            return {
              filed: ['CORE-1', 'CORE-2'],
              weighed: [
                { ticket: 'CORE-1', score: 82, verdict: 'poc', error: '' },
                { ticket: 'CORE-2', score: 41, verdict: 'shelved', error: '' },
              ],
              skipped: [{ repo_url: 'a/b', reason: 'already seen' }],
              blocked: '',
            };
          }
          return null;
        },
      },
      event: { listen: async () => () => {} },
    };
  }, [SETTINGS, STATUS]);

  await page.goto('/index.html');
  await page.waitForFunction(() => typeof window.xnautRenderCoreTeamSettings === 'function');
  await page.evaluate(() => {
    const host = document.createElement('div');
    host.id = 'core-team-settings-host';
    document.body.appendChild(host);
    return window.xnautRenderCoreTeamSettings(host);
  });
  return page.locator('#core-team-settings-host');
}

test('the core team pane shows the knob, the roster and why it is idle', async ({ page }) => {
  const host = await mount(page);
  await expect(host, 'the pane never rendered').toContainText('Core Team');
  await expect(host, 'the threshold is the cost knob and must say so').toContainText('cost knob');
  await expect(host.locator('[data-ct-threshold]')).toHaveValue('70');
  await expect(host.locator('[data-ct-enabled]'), 'experimental means off').not.toBeChecked();
  await expect(host, 'the pane must name who researches').toContainText('@researcher');
  await expect(host, 'and who reviews, on which provider').toContainText('@reviewer');
  await expect(host).toContainText('anthropic');
  await expect(host, 'an idle loop must say why').toContainText('experimental');
  await expect(host, 'a beat that never ran is not "just now"').toContainText('never');
});

test('turning it on saves the knob and keeps every other setting', async ({ page }) => {
  const host = await mount(page);
  // app.js migrates provider credentials into the durable store on load, which
  // is its own settings_set. Count from here so this test measures OUR save.
  const before = await page.evaluate(() => window.__saved.length);
  await host.locator('[data-ct-enabled]').check();
  await host.locator('[data-ct-threshold]').fill('85');
  await host.locator('[data-ct-days]').fill('14');
  await host.locator('[data-ct-topics]').fill('agent harnesses\n\nsandboxing\n');
  await host.locator('[data-ct-save]').click();
  await expect(host).toContainText('Saved.');

  const saved = await page.evaluate((n) => window.__saved.slice(n), before);
  expect(saved.length, 'the save never reached settings_set').toBe(1);
  const [written] = saved;
  expect(written.core_team.enabled).toBe(true);
  expect(written.core_team.poc_threshold).toBe(85);
  expect(written.core_team.beat_days).toBe(14);
  expect(written.core_team.topics, 'blank lines are not topics').toEqual([
    'agent harnesses',
    'sandboxing',
  ]);
  // The whole point of reading the settings back before editing one key.
  expect(written.mcp_token, 'the save stripped an unrelated setting').toBe('keep-me');
  // Not a count: app.js's own migration adds provider rows of its own on load.
  // What matters is that the ones already there are still there.
  expect(
    written.llm_providers.some((item) => item.name === 'perplexity'),
    'the save stripped the providers',
  ).toBe(true);
  // Untouched core-team fields survive too.
  expect(written.core_team.decision_project).toBe('xnaut');
});

test('a threshold outside 0-100 is clamped rather than saved', async ({ page }) => {
  const host = await mount(page);
  const before = await page.evaluate(() => window.__saved.length);
  await host.locator('[data-ct-threshold]').fill('400');
  await host.locator('[data-ct-save]').click();
  await expect(host).toContainText('Saved.');
  const saved = await page.evaluate((n) => window.__saved.slice(n), before);
  expect(saved[0].core_team.poc_threshold).toBe(100);
});

test('Scan now reports what it filed, skipped and weighed', async ({ page }) => {
  const host = await mount(page);
  await host.locator('[data-ct-scan]').click();
  // "Filed 2" and "1 of them earns an agent run" are different news, and the
  // second is the one that costs money next.
  await expect(host, 'a scan that says nothing looks like a quiet week').toContainText(
    'Filed 2; skipped 1; 1 to PoC.',
  );
  expect(await page.evaluate(() => window.__scans)).toBe(1);
});
