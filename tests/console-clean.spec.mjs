// The frontend must load without a ReferenceError.
//
// Written after two bugs in one evening that nothing else caught:
//
//   1. `window.xnautActiveProjectKey` was called and had never been defined. An
//      undefined *property* read is a silent no-op — the per-project scope just
//      quietly fell back to global, no error anywhere.
//   2. `roleFrontierModel` reached for `project`, a parameter of its callers and
//      not a binding in its scope. Referencing an UNDECLARED IDENTIFIER throws
//      ReferenceError, so the NAUT-Flow stage render died and the tab looked
//      unclickable — again with nothing visible.
//
// Both survived `node --check`, a clean Rust build and 295 passing Rust tests,
// because none of those execute the frontend.
//
// SCOPE, deliberately narrow. The app is a Tauri client: outside the shell there
// is no backend, so most features cannot work and a "zero console errors" bar
// would fail for reasons that are not bugs. What IS meaningful without a backend
// is whether our own code references things that do not exist. So:
//
//   * a minimal window.__TAURI__ stub is installed before any script runs, so
//     modules find a bridge and take their normal paths;
//   * the assertion is specifically ReferenceError / "is not defined", which is
//     never explained by an absent backend and is always our mistake.
//
// A missing tab is the silent sibling of the same problem, so it is counted too.

import { test, expect } from '@playwright/test';
import { readFile, readdir } from 'node:fs/promises';

/** Every right-pane view module, for the source-level checks below. */
async function glob() {
  const dir = new URL('../src/js/', import.meta.url);
  const names = await readdir(dir);
  return names.filter((n) => n.endsWith('.js')).map((n) => new URL(n, dir));
}

/** Enough of the Tauri bridge, and enough data, that panels actually RENDER.
 *
 * A null-returning stub was the first attempt and it proved nothing: with no
 * projects, the project panel never renders a stage, so the code paths where
 * today's ReferenceError lived were never executed and the test passed with the
 * bug reintroduced. Verified by putting the bug back — 4/4 still green.
 *
 * So the stub answers the few commands that gate rendering with plausible
 * shapes. It is a fake, and it only reaches as far as the fake is faithful —
 * but it reaches the panels, which is the whole point. */
const TAURI_STUB = () => {
  const noop = () => {};
  const PROJECT = {
    key: 'SMOKE', name: 'Smoke Test', purpose: 'exercise the panels',
    source_path: '/tmp/smoke', stage: 'build', status: 'active', tickets: [],
  };
  const AGENT = {
    handle: 'builder', display_name: 'Builder', tagline: 'Turns product intent into working software.',
    purpose: 'Build and validate xNaut features.', runtime_id: 'codex', provider: 'openai', model: 'gpt-5.6-codex',
    reasoning_effort: 'high',
    execution: 'local', role: 'coding-agent', capabilities: ['terminal', 'code'], notifications: true,
    accent_color: '#f5b840', default_project: '/tmp/smoke', created_at: '2026-08-14T08:00:00Z', updated_at: '2026-08-14T08:00:00Z',
  };
  const NAUTBOT = {
    handle: 'nautbot', display_name: 'NautBot', tagline: 'Your guide and control layer for xNaut.',
    purpose: 'Guide and coordinate xNaut.', runtime_id: 'codex', provider: 'nautgate', model: 'gpt-5.6-sol', reasoning_effort: 'high',
    execution: 'local', role: 'core-orchestrator', capabilities: ['guide', 'coordinate'], notifications: true,
    accent_color: '#f5b840', default_project: null, created_at: '2026-08-14T08:00:00Z', updated_at: '2026-08-14T08:00:00Z',
  };
  const BY_COMMAND = {
    pm_project_list: [PROJECT],
    pm_ticket_list: [],
    pm_change_list: [],
    // The real ModuleStatus shape (src-tauri/src/project_management.rs). This
    // used to be `{ ok, dirty, branch }` — three fields none of which exist, so
    // the panel ran against a status the backend can never return.
    pm_module_status: {
      enabled: true, configured: true, valid: true, repo_path: '/tmp/smoke-control',
      remote_url: '', git_repository: true, project_count: 1, ticket_count: 0,
      error: '', warning: '', branch: 'main', last_commit: '', dirty: false, ahead: 0, behind: 0,
    },
    tasks_list: [],
    zellij_sessions_info: [],
    agent_sessions_list: [],
    agent_profile_list: [NAUTBOT, AGENT],
    agent_profile_get: NAUTBOT,
    agent_list: [{ id:'codex', label:'Codex', available:true, injection_mode:'argv' }],
    skill_list: ['code-review'],
    chat_list_provider_models: [{ provider:'openai', model:'gpt-5.6-codex', label:'GPT-5.6 Codex' }],
    get_home_directory: '/tmp',
    create_terminal_session: { session_id:'smoke-terminal' },
    agent_profile_launch: { session_id:'smoke-agent', agent_id:'builder', injection_mode:'argv' },
    chat_send_provider: 'NautBot reply',
    chat_check_endpoint: true,
    // Every non-Option field of the Rust Settings struct. `engram` was missing
    // and the Tasks Mode settings section reads `s.engram.enabled` without
    // optional chaining, so it threw the moment anything opened that section.
    settings_get: {
      project_root: '/tmp/smoke', categories: [],
      llm: { provider: 'anthropic', model: '', endpoint: '', api_key: '' },
      llm_providers: [], engram: { enabled: false, url: '' },
      project_management: { enabled: false, repo_path: '', remote_url: '' },
      loops: {}, mcp_servers: [], forges: [], editor: '', mcp_port: 8791, mcp_token: '',
    },
    projects_activity: [],
    audit_list: [],
    dag_step: { ready: [], unreachable: [], deadlocked: [] },
    dag_validate: [],
  };
  window.__TAURI__ = {
    core: {
      invoke: (cmd) => Promise.resolve(
        Object.prototype.hasOwnProperty.call(BY_COMMAND, cmd) ? BY_COMMAND[cmd] : null,
      ),
    },
    event: { listen: () => Promise.resolve(noop), emit: () => Promise.resolve() },
    window: { getCurrentWindow: () => ({ listen: () => Promise.resolve(noop) }) },
  };
};

const REFERENCE_ERROR = /ReferenceError|is not defined/i;

function watchForReferenceErrors(page) {
  const found = [];
  page.on('pageerror', (e) => {
    if (REFERENCE_ERROR.test(e.message)) found.push(`uncaught: ${e.message}`);
  });
  page.on('console', (m) => {
    if (m.type() === 'error' && REFERENCE_ERROR.test(m.text())) found.push(`console: ${m.text()}`);
  });
  return found;
}

test('loading the app references nothing that does not exist', async ({ page }) => {
  await page.addInitScript(TAURI_STUB);
  const bad = watchForReferenceErrors(page);
  await page.goto('/');
  await page.waitForLoadState('domcontentloaded');
  await page.waitForTimeout(1500); // views register and listeners wire asynchronously
  expect(bad, bad.join('\n')).toEqual([]);
});

test('every right-pane view has an icon', async () => {
  // The SILENT sibling of a thrown error: a view added to VIEW_ORDER without a
  // matching ICONS entry renders `undefined` into the tab bar — no exception,
  // nothing in the console, just a tab that looks broken or missing.
  //
  // Checked against the source rather than the DOM on purpose: the tab bar is
  // built into a host element that only exists once a pane is mounted, which
  // needs a workspace, which needs a backend. Counting tabs in a headless
  // browser measures the environment, not the code.
  const src = await readFile(new URL('../src/js/right-pane.js', import.meta.url), 'utf8');

  const iconBlock = src.match(/const ICONS = \{([\s\S]*?)\n {2}\};/);
  const orderBlock = src.match(/const VIEW_ORDER = \[([\s\S]*?)\n {2}\];/);
  expect(iconBlock, 'ICONS block should be parseable').toBeTruthy();
  expect(orderBlock, 'VIEW_ORDER block should be parseable').toBeTruthy();

  const icons = new Set([...iconBlock[1].matchAll(/^\s{4}(\w+):/gm)].map((m) => m[1]));
  const views = [...orderBlock[1].matchAll(/key:\s*'([^']+)'/g)].map((m) => m[1]);

  expect(views.length, 'VIEW_ORDER should not be empty').toBeGreaterThan(5);
  const missing = views.filter((v) => !icons.has(v));
  expect(missing, `views in VIEW_ORDER with no icon: ${missing.join(', ')}`).toEqual([]);
});

test('right-pane views queue in the shape the drain expects', async () => {
  // right-pane.js drains the queue with `item.key && item.view`. A module that
  // pushes an array instead is silently dropped — and only when it happens to
  // load before right-pane.js, which makes it a load-order landmine.
  const files = await glob();
  const wrong = [];
  for (const f of files) {
    const src = await readFile(f, 'utf8');
    if (/__xnautRightPaneQueue[\s\S]{0,80}?\.push\(\s*\[/.test(src)) wrong.push(f.pathname.split('/').pop());
  }
  expect(wrong, `these push an array, not {key, view}: ${wrong.join(', ')}`).toEqual([]);
});

test('clicking through the interface references nothing that does not exist', async ({ page }) => {
  await page.addInitScript(TAURI_STUB);
  const bad = watchForReferenceErrors(page);
  await page.goto('/');
  await page.waitForTimeout(1200);

  // Click whatever navigation is present. Handlers will bail early without a
  // backend; the point is that bailing must not throw a ReferenceError.
  const targets = page.locator('[data-rpane-view], .nav-item, [data-nav], .sidebar-nav button');
  const n = Math.min(await targets.count(), 12);
  for (let i = 0; i < n; i += 1) {
    await targets.nth(i).click({ timeout: 1500 }).catch(() => {});
    await page.waitForTimeout(120);
  }
  expect(bad, bad.join('\n')).toEqual([]);
});
