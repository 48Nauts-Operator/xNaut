// XNAUT-86: the path from "I want to work on project X" to "I am in a session".
//
// The band lives in the Observatory, not on the project page: XNAUT-340 moved
// sessions there on 2026-09-12 and that decision stands (asked and confirmed,
// in-543bf61d-5efe-428e-bb01-3c219af46388). What XNAUT-86 adds is the substance
// the band did not have — it could only ever open Claude, it said nothing about
// what resuming an exited session restores, and the name it invented for a new
// session was one the sidebar could not match.
//
// Three states, because they are three different answers rather than three
// renderings of one: running (attach), exited (attach, and it resurrects the
// conversation from the serialized layout), none (one button).
//
// The name rule gets its own tests. It is the part that fails silently: a
// session under the wrong name still opens and still works, and only the
// sidebar's live dot — somewhere else entirely, hours later — reports it.
import { test, expect } from '@playwright/test';

// A capital in the name on purpose: 41 of the 45 projects in the control repo
// have one, and the case is exactly what the old sidebar rule got wrong.
const PROJECTS = [
  { key: 'BUCKY', name: 'Bucky', source_path: '/Users/x/dev/bucky', stage: 'build', status: 'active', tickets: [] },
  { key: 'NAUTGATE', name: 'NautGate', source_path: '/Users/x/dev/nautgate', stage: 'build', status: 'active', tickets: [] },
];

const RUNNING = { name: 'cl-Bucky', exited: false, created: '12m', created_ms: 1000, last_active_ms: 2000 };
const EXITED = { name: 'cx-bucky', exited: true, created: '3h', created_ms: 900, last_active_ms: 1000 };
const OTHER = { name: 'cl-nautgate', exited: false, created: '1m', created_ms: 3000, last_active_ms: 3000 };

const NOT_CONFIGURED = [
  { env: 'local', ready: true, detail: 'zellij on this machine; always available' },
  { env: 'exe-dev', ready: false, detail: 'no "exe-dev" entry in settings.sandboxes' },
  { env: 'gitvm', ready: false, detail: 'no "gitvm" entry in settings.sandboxes; a CLI key alone does not opt the fleet in' },
];

/** Opens the Observatory with one zellij world, on the Bucky project. */
async function band(page, world) {
  await page.addInitScript(() => localStorage.setItem('xnaut-sidebar-visible', '1'));
  await page.goto('/?stub=1');
  await page.waitForSelector('#btn-help');
  await page.evaluate((w) => {
    Object.assign(window.__xnautStub, {
      pm_project_list: w.projects || [],
      zellij_sessions_info: w.zellij || [],
      launch_env_options: w.launchEnvs,
      agent_sessions_list: [], loom_runs_list: [], run_registry_list: [],
      max_usage: null, codex_usage: null,
      create_command_session: { session_id: 'pty-1' },
      ...(w.stub || {}),
    });
  }, world);
  await page.evaluate(() => window.xnautAttachObservatoryTab());
  await expect(page.locator('.obs')).toBeVisible();
  const list = page.locator('[data-sessions]');
  await expect(list.locator('[data-sess-project]')).toBeVisible();
  await list.locator('[data-sess-project]').selectOption('BUCKY');
  await expect(list.locator('[data-sess-open]')).toBeVisible();
  return list;
}

const WORLD = { projects: PROJECTS, launchEnvs: NOT_CONFIGURED };

test('a running session is listed for this project, and another project\'s is not', async ({ page }) => {
  const list = await band(page, { ...WORLD, zellij: [RUNNING, OTHER] });

  await expect(list.locator('.obs-sess-row')).toHaveCount(1);
  await expect(list.locator('.obs-sess-row .t')).toHaveText('cl-Bucky');
  await expect(list.locator('.obs-sess-row .s')).toContainText('Claude Code');
  await expect(list.locator('.obs-sess-row .s')).toContainText('running');
  await expect(list.locator('[data-sess-count]')).toHaveText('1 running · 0 resumable');
});

test('an exited session says what Resume actually restores', async ({ page }) => {
  const list = await band(page, { ...WORLD, zellij: [EXITED] });

  await expect(list.locator('[data-sess-attach="cx-bucky"]')).toHaveText('Resume');
  // Not "exited · resumable", which reads as a shell in the right folder. The
  // serialized layout carries the cwd and the exact command, `claude --continue`
  // included, so what comes back is the conversation.
  await expect(list.locator('.obs-sess-row .s')).toContainText('restores the conversation');
  await expect(list.locator('.obs-sess-row .s')).toContainText('Codex');
  await expect(list.locator('[data-sess-count]')).toHaveText('0 running · 1 resumable');
});

test('running sessions sort above resurrectable ones', async ({ page }) => {
  // EXITED is listed first and is not older by created order, so a band that
  // kept zellij's order would leave it on top.
  const list = await band(page, { ...WORLD, zellij: [EXITED, RUNNING] });
  await expect(list.locator('.obs-sess-row .t')).toHaveText(['cl-Bucky', 'cx-bucky']);
});

test('with no session at all the band is one primary button', async ({ page }) => {
  const list = await band(page, { ...WORLD, zellij: [OTHER] });

  await expect(list.locator('.obs-sess-row')).toHaveCount(0);
  await expect(list.locator('[data-sess-count]')).toHaveText('none');
  const open = list.locator('[data-sess-open]');
  await expect(open).toHaveText('Open a new session');
  await expect(open).toHaveClass(/primary/);
  // Zellij is not a checkbox anywhere in this flow: the session has to outlive
  // the app, so it is stated rather than offered.
  await expect(list.locator('.obs-empty')).toContainText('inside zellij');
});

test('the agent picker offers exactly Claude Code, Codex and Pi', async ({ page }) => {
  const list = await band(page, { ...WORLD, zellij: [] });
  await expect(list.locator('[data-sess-agent] option')).toHaveText(['Claude Code', 'Codex', 'Pi']);
});

test('opening a Codex session names it cx-<project> and runs codex, not claude', async ({ page }) => {
  const list = await band(page, {
    ...WORLD,
    zellij: [],
    stub: { zellij_open_command: { name: 'cx-bucky', command: 'zellij --session cx-bucky' } },
  });

  await list.locator('[data-sess-agent]').selectOption('cx');
  await list.locator('[data-sess-open]').click();

  const open = await page.evaluate(() => window.__xnautInvokes.find((c) => c.cmd === 'zellij_open_command'));
  expect(open, 'Open did not ask the backend for a zellij command').toBeTruthy();
  // Not xnaut-tab-<timestamp>, and not cl- for every agent: the name is what
  // every other surface matches on.
  expect(open.args.session).toBe('cx-bucky');
  expect(open.args.cwd).toBe('/Users/x/dev/bucky');
  expect(open.args.command).toContain('codex');
  expect(open.args.command).not.toContain('claude');
  // NautGate registers a Claude Max launch and sets ANTHROPIC_*. Handing that
  // to Codex points it at an endpoint it does not speak.
  const gate = await page.evaluate(() => window.__xnautInvokes.some((c) => c.cmd === 'nautgate_max_launch_register'));
  expect(gate, 'a Codex session was routed through the Claude wrapper').toBe(false);
});

test('opening a Claude session still routes through NautGate', async ({ page }) => {
  const list = await band(page, {
    ...WORLD,
    zellij: [],
    stub: {
      zellij_open_command: { name: 'cl-bucky', command: 'zellij --session cl-bucky' },
      nautgate_max_launch_register: 'http://127.0.0.1:1/v1',
    },
  });

  await list.locator('[data-sess-open]').click();

  const open = await page.evaluate(() => window.__xnautInvokes.find((c) => c.cmd === 'zellij_open_command'));
  expect(open.args.session).toBe('cl-bucky');
  expect(open.args.command).toContain('claude');
  const gate = await page.evaluate(() => window.__xnautInvokes.find((c) => c.cmd === 'nautgate_max_launch_register'));
  expect(gate, 'the Claude path lost its wrapper').toBeTruthy();
  expect(gate.args.nativeSession).toBe('cl-bucky');
});

test('the provider pair is disabled for an agent it does not configure', async ({ page }) => {
  const list = await band(page, { ...WORLD, zellij: [] });

  await expect(list.locator('[data-sess-provider]')).toBeEnabled();
  await list.locator('[data-sess-agent]').selectOption('pi');
  await expect(list.locator('[data-sess-provider]')).toBeDisabled();
  await expect(list.locator('[data-sess-model]')).toBeDisabled();
  await expect(list.locator('[data-sess-hint]')).toContainText("Pi runs with its own configuration");
});

test('Connect is refused with the reason the backend itself gives', async ({ page }) => {
  const list = await band(page, { ...WORLD, zellij: [] });
  const connect = list.locator('[data-sess-connect]');

  await expect(connect).toBeDisabled();
  // Quoted, not paraphrased, and naming EVERY remote environment: saying only
  // "no exe-dev entry" to somebody reaching for GitVM is the wrong sentence.
  await expect(connect).toHaveAttribute('title', /gitvm: no "gitvm" entry in settings.sandboxes/);
  await expect(connect).toHaveAttribute('title', /exe-dev: no "exe-dev" entry/);
});

test('a configured sandbox enables Connect', async ({ page }) => {
  const list = await band(page, {
    ...WORLD,
    zellij: [],
    launchEnvs: [
      { env: 'local', ready: true, detail: 'zellij on this machine; always available' },
      { env: 'gitvm', ready: true, detail: 'configured with an api key' },
    ],
  });
  await expect(list.locator('[data-sess-connect]')).toBeEnabled();
});

// ── the name rule ────────────────────────────────────────────────────────────
//
// Checked directly because this is the failure that leaves no trace: a session
// under a name nothing matches opens fine, works fine, and is simply invisible
// on every surface that looks for it.

test('a session xNAUT opened for Bucky matches the project Bucky', async ({ page }) => {
  await page.goto('/?stub=1');
  await page.waitForFunction(() => Boolean(window.xnautSessions));
  const answers = await page.evaluate(() => {
    const rule = window.xnautSessions;
    const project = { key: 'BUCKY', name: 'Bucky', source_path: '/tmp/bucky' };
    return {
      // zellij::session_name lowercases, so this is the name a session opened
      // from xNAUT actually gets. The old sidebar rule compared raw strings and
      // answered false here — for 41 of 45 projects.
      lowercased: rule.belongsTo('cl-bucky', project),
      handStarted: rule.belongsTo('cx-Bucky', project),
      // zellij caps a name at 24 characters, so the name can be shorter than
      // the project…
      truncated: rule.belongsTo('cl-nautflow-incident-loo', { name: 'nautflow-incident-loop' }),
      // …or longer than it.
      longer: rule.belongsTo('cx-xnaut-safety-net', { name: 'xnaut' }),
      // Separators are not part of the identity: DAT.AG, dat-ag and datag are
      // one project.
      separators: rule.belongsTo('cl-dat-ag', { name: 'DAT.AG' }),
      // A different project's session is not this project's.
      other: rule.belongsTo('cl-nautgate', project),
      // A plain terminal tab is xnaut-tab-<timestamp> and belongs to no
      // project, so it must never be claimed by one.
      plainTab: rule.belongsTo('xnaut-tab-1757793600000', project),
    };
  });
  expect(answers).toEqual({
    lowercased: true, handStarted: true, truncated: true, longer: true,
    separators: true, other: false, plainTab: false,
  });
});

test('a new name mirrors zellij::session_name, cap included', async ({ page }) => {
  await page.goto('/?stub=1');
  await page.waitForFunction(() => Boolean(window.xnautSessions));
  const answers = await page.evaluate(() => {
    const rule = window.xnautSessions;
    return {
      simple: rule.nameFor('cl', { name: 'Bucky' }),
      codex: rule.nameFor('cx', { name: 'Bucky' }),
      // 24 characters is zellij 0.44's cap; past it zellij rejects the name
      // outright, so a UI that did not truncate would name sessions that never
      // open.
      capped: rule.nameFor('cl', { name: 'Polymarket Signal Terminal' }),
      // Falls back through key and path rather than producing a bare "cl-".
      fromKey: rule.nameFor('pi', { key: 'WEBBUILDER' }),
      fromPath: rule.nameFor('cl', { source_path: '/Users/x/dev/company-manager/' }),
    };
  });
  expect(answers.simple).toBe('cl-bucky');
  expect(answers.codex).toBe('cx-bucky');
  expect(answers.capped).toBe('cl-polymarket-signal-ter');
  expect(answers.capped.length).toBeLessThanOrEqual(24);
  expect(answers.fromKey).toBe('pi-webbuilder');
  expect(answers.fromPath).toBe('cl-company-manager');
});

test('the name a session is opened under is a name the rule then matches', async ({ page }) => {
  await page.goto('/?stub=1');
  await page.waitForFunction(() => Boolean(window.xnautSessions));
  // The round trip is the property that matters, and it is the one that broke:
  // naming and matching lived in different files and disagreed about case.
  const missed = await page.evaluate(() => {
    const rule = window.xnautSessions;
    const projects = [
      { key: 'BUCKY', name: 'Bucky', source_path: '/tmp/bucky' },
      { key: 'CHESSTRAINER', name: 'ChessTrainer', source_path: '/x/chess-trainer' },
      { key: 'DATAG', name: 'DAT.AG', source_path: '/x/dat-ag' },
      { key: 'WEBBUILDER', name: 'WebBuilder', source_path: '/x/webbuilder' },
      { key: 'POLY', name: 'Polymarket Signal Terminal', source_path: '/x/poly' },
    ];
    return projects.flatMap((project) => rule.AGENTS.map((agent) => ({
      project: project.name,
      name: rule.nameFor(agent.key, project),
      matches: rule.belongsTo(rule.nameFor(agent.key, project), project),
    }))).filter((row) => !row.matches);
  });
  expect(missed, 'a session opened under this name would be invisible to every surface').toEqual([]);
});

// ── and the surface the naming exists for ────────────────────────────────────
//
// The Observatory names a session; the SIDEBAR is what has to find it again.
// Asserting the rule in isolation would not have caught the original bug,
// because the rule was fine — the sidebar carried its own copy of it.

test('the sidebar lights a project whose session xNAUT itself opened', async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.clear();
    localStorage.setItem('xnaut-sidebar-visible', '1');
  });
  await page.goto('/?stub=1');
  await page.waitForSelector('#btn-help');
  await page.waitForTimeout(900);
  await page.evaluate(() => {
    Object.assign(window.__xnautStub, {
      // Opening a session registers the project here (tasks_create_project), so
      // this is the row that exists a moment after the band opens one.
      tasks_list: [
        { id: 'bucky', name: 'Bucky', path: '/repo/bucky', kind: 'project' },
        { id: 'nautgate', name: 'NautGate', path: '/repo/nautgate', kind: 'project' },
      ],
      git_worktree_list: [], loom_runs_list: [], agent_sessions_list: [],
      // Exactly what `zellij_open_command` produces for the project "Bucky":
      // lowercased, because zellij::session_name lowercases. The sidebar's old
      // rule compared raw strings, so this left the project dark while the
      // session was running.
      zellij_sessions_info: [{ name: 'cl-bucky', exited: false, created: '5m', created_ms: 1, last_active_ms: 1 }],
    });
    window.xnautSidebarRefresh();
  });

  const bucky = page.locator('.sbar-group[data-group="bucky"]');
  await expect(bucky).toBeVisible();
  await expect(bucky.locator('.sbar-dot.sbar-live'),
    'the project with a running session is not lit').toHaveCount(1);
  // The chip is how you attach from here, and it names the agent.
  await expect(bucky.locator('.sbar-chip.sbar-sess').first()).toHaveText('cl');
  // The project with no session stays dark, so the dot means something.
  await expect(page.locator('.sbar-group[data-group="nautgate"] .sbar-dot.sbar-live'))
    .toHaveCount(0);
});

test.beforeEach(async ({ page }) => {
  page.on('dialog', (dialog) => dialog.dismiss().catch(() => {}));
});
