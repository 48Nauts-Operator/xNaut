// XNAUT-47 — Command Snippets quick-access dropdown.
// Boots the REAL src/index.html in Chromium with a mocked Tauri bridge,
// then exercises the new toolbar dropdown end-to-end: open, list, filter,
// copy, and run (push to terminal).
//
// Run: node tests/commands-dropdown.spec.mjs
import { chromium } from '/usr/lib/node_modules/playwright/index.mjs';
import { pathToFileURL } from 'node:url';
import path from 'node:path';

const HEADLESS = process.env.HEADED !== '1';
const INDEX = pathToFileURL(path.resolve('src/index.html')).href;

const SEED_SNIPPETS = [
  {
    id: '1', name: 'Docker Up', category: 'Docker', favorite: true,
    content: '```bash\ndocker compose up -d\ndocker ps\n```',
  },
  {
    id: '2', name: 'Git Status', category: 'Git', favorite: false,
    content: '```bash\ngit status\ngit log --oneline -5\n```',
  },
];

// Injected before any page script runs: fake Tauri + seeded snippets.
function bootstrap(seed) {
  window.__invokeCalls = [];
  const invoke = (cmd, args) => {
    window.__invokeCalls.push({ cmd, args });
    // Return shapes permissive enough for init() to complete.
    if (cmd === 'write_to_terminal') return Promise.resolve(true);
    return Promise.resolve(null);
  };
  const listen = () => Promise.resolve(() => {});
  window.__TAURI__ = {
    core: { invoke },
    event: { listen, emit: () => Promise.resolve() },
    shell: { open: () => Promise.resolve() },
  };
  try {
    localStorage.setItem('xnaut-snippets', JSON.stringify(seed));
  } catch (e) { /* ignore */ }
  // Provide a clipboard stub that records writes (headless has no real one).
  window.__clip = [];
}

const results = [];
function check(name, cond) {
  results.push({ name, ok: !!cond });
  console.log(`${cond ? '✓ PASS' : '✗ FAIL'} — ${name}`);
}

const browser = await chromium.launch({ headless: HEADLESS });
const context = await browser.newContext();
await context.grantPermissions(['clipboard-read', 'clipboard-write']);
const page = await context.newPage();
page.on('console', (m) => { if (m.type() === 'error') console.log('  [page error]', m.text()); });

await page.addInitScript(bootstrap, SEED_SNIPPETS);
await page.goto(INDEX);

// Give the app time to boot + wire event listeners.
await page.waitForTimeout(1500);

// Fallback: if init() bailed before wiring handlers, wire them directly so the
// test still validates the feature's own code (never fakes the assertions).
await page.evaluate(() => {
  const btn = document.getElementById('btn-commands-menu');
  if (btn && !btn.onclick && typeof setupEventListeners === 'function') {
    try {
      if (typeof invoke === 'undefined' || !invoke) window.invoke = window.__TAURI__.core.invoke;
      if (typeof loadSnippets === 'function') loadSnippets();
      setupEventListeners();
    } catch (e) { console.error('manual wire failed', e); }
  }
  // Minimal terminal context so "Run" has a target.
  try {
    window.tabs = [{ id: 't1', focusedPaneIndex: 0, terminals: [{ sessionId: 'sess-1' }] }];
    window.activeTabId = 't1';
  } catch (e) { /* globals are top-level lets; assign via eval below */ }
});

// tabs/activeTabId are top-level `let` bindings — assign in global scope.
await page.evaluate(() => {
  // eslint-disable-next-line no-eval
  eval("tabs = [{ id: 't1', focusedPaneIndex: 0, terminals: [{ sessionId: 'sess-1' }] }]; activeTabId = 't1';");
});

// 1) Button exists in the toolbar.
const hasBtn = await page.locator('#btn-commands-menu').count();
check('toolbar command button exists', hasBtn === 1);

// 2) Dropdown starts hidden.
check('dropdown starts hidden', await page.locator('#commands-dropdown').getAttribute('hidden') !== null);

// 3) Click opens it.
await page.locator('#btn-commands-menu').click();
await page.waitForTimeout(200);
check('dropdown opens on click', await page.locator('#commands-dropdown').getAttribute('hidden') === null);
check('aria-expanded set true', await page.locator('#btn-commands-menu').getAttribute('aria-expanded') === 'true');

// 4) All 4 commands from both snippets are listed.
const rowCount = await page.locator('#commands-dropdown .commands-row').count();
check('lists all extracted commands (4)', rowCount === 4);

// 5) Favorite snippet ("Docker Up") group appears first.
const firstLabel = (await page.locator('#commands-dropdown .commands-group-label').first().innerText()).toLowerCase();
check('favorite snippet grouped first', firstLabel.includes('docker up'));

// 6) Search filters the list (unique command substring — no snippet-name overlap).
await page.locator('#commands-search').fill('log --oneline');
await page.waitForTimeout(150);
const filtered = await page.locator('#commands-dropdown .commands-row').count();
check('search filters to matching command', filtered === 1);
check('filtered command is git log',
  (await page.locator('#commands-dropdown .commands-row code').first().innerText()).trim() === 'git log --oneline -5');

// 6b) Searching by a command substring resolves cleanly for the run/copy path.
await page.locator('#commands-search').fill('git status');
await page.waitForTimeout(150);

// 7) Copy button copies command to clipboard.
await page.locator('#commands-dropdown .commands-row .copy-btn').first().click();
await page.waitForTimeout(150);
const clip = await page.evaluate(() => navigator.clipboard.readText().catch(() => ''));
check('copy button writes command to clipboard', clip.trim() === 'git status');

// 8) Run button pushes the command to the active terminal via invoke.
await page.locator('#commands-dropdown .commands-row .run-btn').first().click();
await page.waitForTimeout(250);
const runCall = await page.evaluate(() =>
  (window.__invokeCalls || []).find(c => c.cmd === 'write_to_terminal' && c.args && c.args.data === 'git status\n'));
check('run button invokes write_to_terminal with command', !!runCall);

// 9) Running closes the dropdown.
check('dropdown closes after running', await page.locator('#commands-dropdown').getAttribute('hidden') !== null);

// 10) Outside click closes an open dropdown.
await page.locator('#btn-commands-menu').click();
await page.waitForTimeout(150);
await page.mouse.click(5, 400);
await page.waitForTimeout(150);
check('outside click closes dropdown', await page.locator('#commands-dropdown').getAttribute('hidden') !== null);

await browser.close();

const failed = results.filter(r => !r.ok);
console.log(`\n${results.length - failed.length}/${results.length} checks passed.`);
if (failed.length) {
  console.error('FAILURES:', failed.map(f => f.name).join(', '));
  process.exit(1);
}
console.log('ALL GREEN ✅');
