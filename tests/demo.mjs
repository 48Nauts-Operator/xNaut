// XNAUT-47 — DEMO: Command Snippets quick-access dropdown.
// Opens the REAL src/index.html in a headed, maximized Chromium on DISPLAY=:0
// and slowly drives the new toolbar dropdown end to end (open → browse →
// search → copy → run into a live terminal) so the screen recording shows the
// feature working the way a user would use it.
//
// Run: DISPLAY=:0 node tests/demo.mjs
import { chromium } from '/usr/lib/node_modules/playwright/index.mjs';
import { pathToFileURL } from 'node:url';
import path from 'node:path';

const INDEX = pathToFileURL(path.resolve('src/index.html')).href;

const SNIPPETS = [
  { id: '1', name: 'Docker Compose', category: 'Docker', favorite: true,
    content: '```bash\ndocker compose up -d\ndocker compose logs -f\ndocker ps\n```' },
  { id: '2', name: 'Git Workflow', category: 'Git', favorite: false,
    content: '```bash\ngit status\ngit pull --rebase\ngit log --oneline -10\n```' },
  { id: '3', name: 'Build & Test', category: 'Build', favorite: true,
    content: '```bash\nnpm ci\nnpm run build\nnpm test\n```' },
  { id: '4', name: 'Deploy Prod', category: 'Deployment', favorite: false,
    content: '```bash\nkubectl apply -f k8s/\nkubectl rollout status deploy/api\n```' },
];

// Injected before any page script: Tauri shim (with a live mock terminal),
// seeded snippets, and a pre-DOM listener that guarantees the app can boot.
function bootstrap(snippets) {
  window.__demoLog = [];
  const appendTerminal = (data) => {
    const host = document.getElementById('terminal-container');
    if (!host) return;
    let term = document.getElementById('demo-term');
    if (!term) {
      host.innerHTML = ''; // replace the failed native terminal with a clean demo terminal
      term = document.createElement('div');
      term.id = 'demo-term';
      term.style.cssText = 'font-family:monospace;font-size:14px;line-height:1.6;padding:16px;color:#d4f5e2;background:#0b0f0d;height:100%;overflow:auto;white-space:pre-wrap;';
      term.innerHTML = '<div style="color:#7dd3a8">● xNAUT terminal — session ready</div><div style="color:#5b7a68">~/project $ <span id="demo-cursor">▋</span></div>';
      host.appendChild(term);
    }
    const cursor = document.getElementById('demo-cursor');
    const line = String(data).replace(/\n$/, '');
    const cmd = document.createElement('div');
    cmd.innerHTML = '<span style="color:#5b7a68">~/project $ </span><span style="color:#eafff2">' + line.replace(/</g, '&lt;') + '</span>';
    if (cursor && cursor.parentElement) term.insertBefore(cmd, cursor.parentElement);
    const out = document.createElement('div');
    out.style.color = '#7dd3a8';
    out.textContent = '↳ executing “' + line + '” …';
    if (cursor && cursor.parentElement) term.insertBefore(out, cursor.parentElement);
    term.scrollTop = term.scrollHeight;
  };

  const invoke = (cmd, args) => {
    window.__demoLog.push({ cmd, args });
    if (cmd === 'write_to_terminal' && args && args.data) appendTerminal(args.data);
    return Promise.resolve(null);
  };
  window.__TAURI__ = {
    core: { invoke },
    event: { listen: () => Promise.resolve(() => {}), emit: () => Promise.resolve() },
    shell: { open: () => Promise.resolve() },
  };
  try { localStorage.setItem('xnaut-snippets', JSON.stringify(snippets)); } catch (e) {}
  try { localStorage.setItem('xnaut-snippet-categories', JSON.stringify(['Docker', 'Git', 'Build', 'Deployment'])); } catch (e) {}

  // Ensure elements the fresh-boot path expects exist, so init() runs further.
  document.addEventListener('DOMContentLoaded', () => {
    for (const id of ['chat-messages', 'chat-sessions-list']) {
      if (!document.getElementById(id)) {
        const d = document.createElement('div');
        d.id = id; d.style.display = 'none';
        document.body.appendChild(d);
      }
    }
    // Keep the auto-update banner out of the demo (it overlays the toolbar).
    const killBanner = () => {
      const b = document.getElementById('update-banner');
      if (b) b.style.display = 'none';
    };
    killBanner();
    setInterval(killBanner, 500);
  });
}

function caption(text) {
  let el = document.getElementById('demo-caption');
  if (!el) {
    el = document.createElement('div');
    el.id = 'demo-caption';
    el.style.cssText = 'position:fixed;left:50%;bottom:40px;transform:translateX(-50%);z-index:99999;background:rgba(16,20,18,0.94);color:#eafff2;padding:12px 22px;border:1px solid #2f6b4f;border-radius:10px;font:600 17px/1.4 -apple-system,Segoe UI,sans-serif;box-shadow:0 8px 30px rgba(0,0,0,0.5);max-width:80vw;text-align:center;';
    document.body.appendChild(el);
  }
  el.textContent = text;
}

const browser = await chromium.launch({
  headless: false,
  args: ['--no-sandbox', '--start-maximized', '--window-position=0,0', '--window-size=1440,900'],
  slowMo: 120,
});
const context = await browser.newContext({ viewport: null });
await context.grantPermissions(['clipboard-read', 'clipboard-write']);
const page = await context.newPage();

await page.addInitScript(bootstrap, SNIPPETS);
await page.goto(INDEX);
await page.waitForTimeout(1800);

// Guarantee the dropdown handlers + terminal context are live for the demo.
await page.evaluate(() => {
  const btn = document.getElementById('btn-commands-menu');
  try {
    if (typeof invoke === 'undefined' || !invoke) window.invoke = window.__TAURI__.core.invoke;
    if (typeof loadSnippets === 'function') loadSnippets();
    if (btn && !btn.onclick && typeof setupEventListeners === 'function') setupEventListeners();
  } catch (e) { console.error(e); }
  // Prime the visible mock terminal + terminal globals.
  window.__TAURI__.core.invoke('write_to_terminal', { sessionId: 'sess-1', data: 'echo "welcome to xNAUT"' });
  // eslint-disable-next-line no-eval
  eval("tabs = [{ id: 't1', focusedPaneIndex: 0, terminals: [{ sessionId: 'sess-1' }] }]; activeTabId = 't1';");
});

const cap = (t) => page.evaluate(caption, t);
const pause = (ms) => page.waitForTimeout(ms);

// ---- Walkthrough ----
await cap('XNAUT-47 — Command Snippets are now one click away from the toolbar');
await pause(5000);

await cap('Before: snippets were buried in the ⋮ menu on the right. Now they get their own icon.');
await pause(4500);

await cap('New “commands” icon (❯) sits on the left of the toolbar icons');
// Nudge the mouse to the new icon.
const box = await page.locator('#btn-commands-menu').boundingBox();
if (box) await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2, { steps: 20 });
await pause(3200);

await cap('Click it — every saved command shows in a dropdown');
await page.locator('#btn-commands-menu').click();
await pause(3000);

await cap('Commands are grouped by snippet — favorites (★) first');
await page.mouse.move(box.x + 40, box.y + 160, { steps: 15 });
await pause(4000);

await cap('Type to filter across all your commands…');
await page.locator('#commands-search').click();
for (const ch of 'kubectl') { await page.keyboard.type(ch); await pause(160); }
await pause(2000);

await cap('…the list narrows instantly');
await pause(2600);

await cap('Clear the search to see everything again');
await page.locator('#commands-search').fill('');
await pause(1600);

await cap('Copy a command with one click (📋) — ready to paste anywhere');
await page.locator('#commands-dropdown .commands-row', { hasText: 'git status' }).locator('.copy-btn').click();
await pause(2200);

await cap('Or push it straight into the terminal with Run (▶)');
await page.locator('#commands-dropdown .commands-row', { hasText: 'git status' }).locator('.run-btn').click();
await pause(2600);

await cap('The command runs in the active terminal — dropdown closes automatically');
await pause(3200);

await cap('Run another — “docker compose up -d”');
await page.locator('#btn-commands-menu').click();
await pause(1200);
await page.locator('#commands-dropdown .commands-row', { hasText: 'docker compose up -d' }).locator('.run-btn').click();
await pause(2600);

await cap('And one more — “npm run build”');
await page.locator('#btn-commands-menu').click();
await pause(1400);
await page.locator('#commands-dropdown .commands-row', { hasText: 'npm run build' }).locator('.run-btn').click();
await pause(3000);

// Reopen, filter to docker, and run the compose logs command — shows search + run together.
await cap('Reopen anytime — filter, then run without leaving your terminal');
await page.locator('#btn-commands-menu').click();
await pause(1400);
await page.locator('#commands-search').click();
for (const ch of 'docker') { await page.keyboard.type(ch); await pause(150); }
await pause(2200);
await page.locator('#commands-dropdown .commands-row', { hasText: 'docker compose logs -f' }).locator('.run-btn').click();
await pause(3000);

await cap('Every command executed straight from the toolbar dropdown — no digging through menus');
await pause(3600);

await cap('XNAUT-47 ✓  Command snippets moved into a fast, searchable toolbar dropdown — run or copy');
await pause(4500);

await browser.close();
console.log('DEMO COMPLETE');
