// Issue intake's one surface (XNAUT-382).
//
// The loop is headless: it rides the sweep and files tickets nobody watched it
// file. So the pane is the only place a person can see which projects take
// issues in, switch one on, or find out why one that is switched on is doing
// nothing. A switch nothing renders is a feature that exists in the source and
// nowhere else, which this codebase has now shipped twice (XNAUT-189's policy
// editor, the roster panel's missing global).
//
// The second half is the one that would cost something: the Linear key is
// edited on a settings object that also carries the MCP token and every
// provider credential. Writing a fresh object instead of editing the read-back
// one is how a save on this page silently strips the rest.
import { test, expect } from '@playwright/test';

const SETTINGS = {
  project_root: '/tmp/x',
  mcp_token: 'keep-me',
  llm_providers: [{ name: 'perplexity', endpoint: 'https://api.perplexity.ai', enabled: true }],
  linear: { api_key: '', endpoint: '' },
};

const STATUS = {
  blocked: '',
  projects: [
    {
      key: 'XNAUT',
      name: 'xNAUT',
      forge_remote: 'https://github.com/48Nauts/xnaut.git',
      enabled: true,
      trigger: 'labelled',
      label: 'xnaut',
      linear_team: '',
      origin: 'github:48Nauts/xnaut',
      cursor: '000000000075',
      tracked: 3,
      blocked: '',
    },
    {
      key: 'PLOUGH',
      name: 'Plough',
      forge_remote: 'https://github.com/48Nauts/plough.git',
      enabled: false,
      trigger: 'all',
      label: 'xnaut',
      linear_team: '',
      origin: '',
      cursor: '',
      tracked: 0,
      blocked: 'no token configured for github host https://api.github.com',
    },
  ],
};

async function mount(page, status = STATUS) {
  await page.addInitScript(([settings, initial]) => {
    window.__settings = settings;
    window.__saved = [];
    window.__configured = [];
    window.__runs = [];
    window.__TAURI__ = {
      core: {
        invoke: async (cmd, args) => {
          if (cmd === 'settings_get') return JSON.parse(JSON.stringify(window.__settings));
          if (cmd === 'settings_set') {
            window.__saved.push(args.settings);
            window.__settings = args.settings;
            return null;
          }
          if (cmd === 'issue_intake_status') return initial;
          if (cmd === 'issue_intake_configure') {
            window.__configured.push(args);
            return args.intake;
          }
          if (cmd === 'issue_intake_run_now') {
            window.__runs.push(args);
            return [{
              project: args.project,
              origin: 'github:48Nauts/xnaut',
              created: args.rescan ? [] : ['XNAUT-400'],
              mirrored: 2,
              already_tracked: args.rescan ? 4 : 1,
              blocked: '',
            }];
          }
          return null;
        },
      },
      event: { listen: async () => () => {} },
    };
  }, [SETTINGS, status]);

  await page.goto('/index.html');
  await page.waitForFunction(() => typeof window.xnautRenderIssueIntakeSettings === 'function');
  await page.evaluate(() => {
    const host = document.createElement('div');
    host.id = 'issue-intake-settings-host';
    document.body.appendChild(host);
    return window.xnautRenderIssueIntakeSettings(host);
  });
  return page.locator('#issue-intake-settings-host');
}

test('the pane lists every project that could take issues in, and what stops the ones that cannot', async ({ page }) => {
  const host = await mount(page);
  await expect(host, 'the pane never rendered').toContainText('Issue Intake');
  await expect(host, 'an issue must not read as an assignment').toContainText('nothing is dispatched');

  const xnaut = host.locator('[data-ii-project="XNAUT"]');
  await expect(xnaut.locator('[data-ii-enabled]')).toBeChecked();
  await expect(xnaut.locator('[data-ii-trigger]')).toHaveValue('labelled');
  await expect(xnaut.locator('[data-ii-label]')).toHaveValue('xnaut');
  await expect(xnaut, 'the remote it reads must be visible').toContainText('github.com/48Nauts/xnaut');
  await expect(xnaut, 'how many it already tracks is the useful number').toContainText('3 issue(s) tracked');
  // The cursor is stored zero-padded; showing "000000000075" to a person is
  // an implementation detail leaking onto the screen.
  await expect(xnaut).toContainText('seen up to 75');

  const plough = host.locator('[data-ii-project="PLOUGH"]');
  await expect(plough.locator('[data-ii-enabled]')).not.toBeChecked();
  await expect(plough.locator('[data-ii-blocked]'), 'an idle intake must say why').toContainText(
    'no token configured',
  );
});

test('switching a project on saves only that project', async ({ page }) => {
  const host = await mount(page);
  // app.js migrates provider credentials into the durable store on load, which
  // is its own settings_set. Count from here so this measures OUR save.
  const before = await page.evaluate(() => window.__saved.length);
  const plough = host.locator('[data-ii-project="PLOUGH"]');
  await plough.locator('[data-ii-enabled]').check();
  await plough.locator('[data-ii-trigger]').selectOption('labelled');
  await plough.locator('[data-ii-label]').fill('triage');
  await plough.locator('[data-ii-save]').click();
  await expect(plough).toContainText('Saved.');

  const configured = await page.evaluate(() => window.__configured);
  expect(configured.length, 'the save never reached issue_intake_configure').toBe(1);
  expect(configured[0].project).toBe('PLOUGH');
  expect(configured[0].intake).toEqual({
    enabled: true,
    trigger: 'labelled',
    label: 'triage',
    linear_team: '',
  });
  // A per-project toggle lives on the board, not in this machine's settings,
  // and must not touch the object carrying the MCP token and the API keys.
  expect(await page.evaluate((n) => window.__saved.length - n, before)).toBe(0);
});

test('an empty label falls back to xnaut rather than matching every issue', async ({ page }) => {
  const host = await mount(page);
  const xnaut = host.locator('[data-ii-project="XNAUT"]');
  await xnaut.locator('[data-ii-label]').fill('   ');
  await xnaut.locator('[data-ii-save]').click();
  const configured = await page.evaluate(() => window.__configured);
  expect(configured[0].intake.label, 'a blank label under a label trigger matches everything').toBe(
    'xnaut',
  );
});

test('Run now saves first, then reports what it filed', async ({ page }) => {
  const host = await mount(page);
  const xnaut = host.locator('[data-ii-project="XNAUT"]');
  await xnaut.locator('[data-ii-trigger]').selectOption('all');
  await xnaut.locator('[data-ii-run]').click();
  await expect(xnaut).toContainText('Filed XNAUT-400');
  await expect(xnaut, 'the label mirror is separate news from the filing').toContainText(
    '2 label(s) updated',
  );

  // Pressing Run with an unsaved trigger and watching it use the old one is
  // the kind of surprise that makes a pane untrustworthy.
  const configured = await page.evaluate(() => window.__configured);
  expect(configured.at(-1).intake.trigger).toBe('all');
  const runs = await page.evaluate(() => window.__runs);
  expect(runs).toEqual([{ project: 'XNAUT', rescan: false }]);
});

test('Rescan is an explicit button, because the cursor is a high-water mark', async ({ page }) => {
  // An issue labelled AFTER it was created sits below the cursor forever. The
  // escape hatch has to be pressable or it is a silent no-op.
  const host = await mount(page);
  const xnaut = host.locator('[data-ii-project="XNAUT"]');
  await xnaut.locator('[data-ii-rescan]').click();
  await expect(xnaut).toContainText('Filed nothing');
  await expect(xnaut).toContainText('4 already tracked');
  const runs = await page.evaluate(() => window.__runs);
  expect(runs.at(-1)).toEqual({ project: 'XNAUT', rescan: true });
});

test('saving the Linear key keeps every other setting', async ({ page }) => {
  const host = await mount(page);
  const before = await page.evaluate(() => window.__saved.length);
  await host.locator('[data-ii-key]').fill('lin_api_secret');
  await host.locator('[data-ii-save-key]').click();
  await expect(host.locator('[data-ii-key-status]')).toContainText('Saved.');

  const saved = await page.evaluate((n) => window.__saved.slice(n), before);
  expect(saved.length, 'the save never reached settings_set').toBe(1);
  const [written] = saved;
  expect(written.linear.api_key).toBe('lin_api_secret');
  // The whole point of reading the settings back before editing one key.
  expect(written.mcp_token, 'the save stripped an unrelated setting').toBe('keep-me');
  expect(
    written.llm_providers.some((item) => item.name === 'perplexity'),
    'the save stripped the providers',
  ).toBe(true);
  // A credential is never rendered in the clear.
  await expect(host.locator('[data-ii-key]')).toHaveAttribute('type', 'password');
});

test('a board that cannot be read says so instead of rendering an empty list', async ({ page }) => {
  const host = await mount(page, {
    projects: [],
    blocked: 'Project Management is not configured on this machine',
  });
  await expect(host.locator('[data-ii-global-blocked]')).toContainText('not configured');
  await expect(host.locator('[data-ii-project="XNAUT"]')).toHaveCount(0);
});
