#!/usr/bin/env node
// xNaut Pre-Flight Check — a STANDALONE health/regression harness (run outside
// the app). It can't click the GUI, but it verifies the contracts that have
// actually broken in practice: build health, backend tests, ACL command
// coverage, JS validity, version consistency, CSP/CDN, and live services.
//
// Usage:
//   node scripts/preflight.mjs            # full run (includes cargo build/test)
//   node scripts/preflight.mjs --fast     # skip slow cargo build/test/clippy
//   node scripts/preflight.mjs --open     # open the HTML report when done
//
// Output: preflight-report.html in the repo root. Exit code 1 if any FAIL.

import { execSync } from 'node:child_process';
import { readFileSync, existsSync, writeFileSync, readdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join, basename } from 'node:path';
import { homedir } from 'node:os';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const FAST = process.argv.includes('--fast');
const OPEN = process.argv.includes('--open');
const results = []; // { group, name, status: 'pass'|'fail'|'warn'|'skip', detail }

const add = (group, name, status, detail = '') => { results.push({ group, name, status, detail }); log(group, name, status, detail); };
function log(group, name, status, detail) {
  const icon = { pass: '✅', fail: '❌', warn: '⚠️ ', skip: '➖' }[status] || '?';
  let line = `${icon} [${group}] ${name}`;
  if (detail && status !== 'pass') line += ` — ${String(detail).split('\n')[0].slice(0, 140)}`;
  console.log(line);
}
function read(p) { try { return readFileSync(join(ROOT, p), 'utf8'); } catch { return ''; } }
function sh(cmd, opts = {}) {
  try { return { ok: true, out: execSync(cmd, { cwd: ROOT, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'], ...opts }) }; }
  catch (e) { return { ok: false, out: (e.stdout || '') + (e.stderr || '') + (e.message || '') }; }
}

// ── 1. Build & Tests ─────────────────────────────────────────────────────────
function checkBuildAndTests() {
  const G = 'Build & Tests';
  if (FAST) { add(G, 'cargo build / test / clippy', 'skip', '--fast'); return; }

  let r = sh('cargo build --manifest-path src-tauri/Cargo.toml 2>&1', { timeout: 600000 });
  add(G, 'cargo build', r.ok ? 'pass' : 'fail', r.ok ? '' : tail(r.out));

  r = sh('cargo test --manifest-path src-tauri/Cargo.toml 2>&1', { timeout: 600000 });
  const m = r.out.match(/test result: \w+\. (\d+) passed; (\d+) failed/);
  if (m) add(G, 'cargo test', +m[2] === 0 ? 'pass' : 'fail', `${m[1]} passed, ${m[2]} failed`);
  else add(G, 'cargo test', r.ok ? 'pass' : 'fail', r.ok ? '' : tail(r.out));

  r = sh('cargo fmt --manifest-path src-tauri/Cargo.toml -- --check 2>&1', { timeout: 120000 });
  add(G, 'cargo fmt --check', r.ok ? 'pass' : 'warn', r.ok ? '' : 'formatting drift');

  r = sh('cargo clippy --manifest-path src-tauri/Cargo.toml 2>&1', { timeout: 600000 });
  const warns = (r.out.match(/^warning:/gm) || []).length;
  add(G, 'cargo clippy', r.ok ? (warns ? 'warn' : 'pass') : 'fail', r.ok ? `${warns} warnings` : tail(r.out));
}

// ── 2. Static wiring ─────────────────────────────────────────────────────────
function checkAclCoverage() {
  const G = 'Static Wiring';
  const main = read('src-tauri/src/main.rs');
  const block = main.match(/generate_handler!\[([\s\S]*?)\]/);
  if (!block) { add(G, 'ACL command coverage', 'fail', 'could not find generate_handler! block'); return; }
  const cmds = [...block[1].matchAll(/([a-zA-Z_][\w]*)\s*,/g)]
    .map((x) => x[1])
    .filter((n) => n && !['', 'mut'].includes(n));
  // last path segment names (module::command -> command); our regex already grabs the bare ident before comma
  const perms = read('src-tauri/permissions/default.toml');
  const missing = cmds.filter((c) => !new RegExp(`"${c}"`).test(perms));
  if (missing.length === 0) add(G, 'ACL command coverage', 'pass', `${cmds.length} commands all permitted`);
  else add(G, 'ACL command coverage', 'fail', `missing permission for: ${missing.join(', ')}`);
}

function checkVersions() {
  const G = 'Static Wiring';
  const cargo = (read('src-tauri/Cargo.toml').match(/^version\s*=\s*"([^"]+)"/m) || [])[1];
  const conf = (read('src-tauri/tauri.conf.json').match(/"version"\s*:\s*"([^"]+)"/) || [])[1];
  const badge = (read('README.md').match(/version-([\d.]+)-blue/) || [])[1];
  const changelog = (read('CHANGELOG.md').match(/##\s*\[([\d.]+)\]/) || [])[1];
  const all = { 'Cargo.toml': cargo, 'tauri.conf.json': conf, 'README badge': badge, 'CHANGELOG': changelog };
  const vals = Object.values(all).filter(Boolean);
  const consistent = vals.length === 4 && new Set(vals).size === 1;
  add(G, 'Version consistency', consistent ? 'pass' : 'warn',
    Object.entries(all).map(([k, v]) => `${k}=${v || '?'}`).join('  '));
}

// ── 3. Frontend ──────────────────────────────────────────────────────────────
function checkJsSyntax() {
  const G = 'Frontend';
  const dir = join(ROOT, 'src/js');
  const files = existsSync(dir) ? readdirSync(dir).filter((f) => f.endsWith('.js')) : [];
  const bad = [];
  for (const f of files) {
    const r = sh(`node --check ${JSON.stringify(join('src/js', f))} 2>&1`);
    if (!r.ok) bad.push(`${f}: ${r.out.split('\n')[0]}`);
  }
  add(G, `JS syntax (${files.length} files)`, bad.length ? 'fail' : 'pass', bad.join(' | '));
}

function checkLoopsRenderer() {
  const G = 'Frontend';
  const source = read('frontend/loops-editor-entry.js');
  const bundle = read('src/js/vendor/loops-editor.bundle.js');
  const registration = "customElements.define('xnaut-workflow-node', XnautWorkflowNode)";
  const valid = source.includes(registration) && bundle.includes('xnaut-workflow-node');
  add(G, 'Loops custom node registration', valid ? 'pass' : 'fail',
    valid ? '' : 'custom Lit node must be registered before Rete instantiates it');
}

function checkChatContracts() {
  const G = 'Frontend';
  const r = sh('node scripts/chat-contracts.mjs 2>&1');
  add(G, 'Chat settings/tool-call contracts', r.ok ? 'pass' : 'fail', r.ok ? '' : tail(r.out));
}

function checkIndexIncludes() {
  const G = 'Frontend';
  const html = read('src/index.html');
  const required = ['markdown-render.js', 'markdown-pane.js', 'plan-pane.js', 'chat-panel.js',
    'right-pane.js', 'right-pane-files.js', 'sidebar.js', 'tasks-mode-glue.js', 'app.js'];
  const missing = required.filter((s) => !html.includes(`js/${s}`));
  add(G, 'index.html script includes', missing.length ? 'fail' : 'pass',
    missing.length ? `missing: ${missing.join(', ')}` : `${required.length} core scripts present`);
}

function checkPaneFactories() {
  const G = 'Frontend';
  // Every kind switched on in app.js splitPane/switchTab must have a factory defined somewhere.
  const all = readdirSync(join(ROOT, 'src/js')).filter((f) => f.endsWith('.js'))
    .map((f) => read(`src/js/${f}`)).join('\n');
  const factories = ['xnautCreateChatPane', 'xnautCreatePlanPane', 'xnautCreateMarkdownPane',
    'xnautCreateBrowserPane', 'xnautCreateDiffPane', 'xnautCreatePmPanel'];
  const missing = factories.filter((fn) => !new RegExp(`window\\.${fn}\\s*=`).test(all));
  add(G, 'Pane factories registered', missing.length ? 'fail' : 'pass',
    missing.length ? `missing: ${missing.join(', ')}` : `${factories.length} factories present`);
}

function checkCsp() {
  const G = 'Frontend';
  const conf = read('src-tauri/tauri.conf.json');
  const csp = (conf.match(/"csp"\s*:\s*"([^"]+)"/) || [])[1] || '';
  const needs = csp.includes('cdn.jsdelivr.net');
  add(G, 'CSP allows jsdelivr (marked/hljs/mermaid)', needs ? 'pass' : 'fail',
    needs ? '' : 'script-src missing cdn.jsdelivr.net');
}

// ── 4. Services (live) ───────────────────────────────────────────────────────
function settings() {
  const p = join(homedir(), 'Library/Application Support/xnaut/settings.json');
  try { return JSON.parse(readFileSync(p, 'utf8')); } catch { return null; }
}
function curlHead(url, timeoutS = 8) {
  const r = sh(`curl -s -o /dev/null -w "%{http_code}" --max-time ${timeoutS} ${JSON.stringify(url)}`);
  return r.out.trim();
}
function checkServices() {
  const G = 'Services (live)';
  // CDNs the markdown stack depends on.
  for (const [name, url] of [
    ['marked', 'https://cdn.jsdelivr.net/npm/marked@12.0.2/marked.min.js'],
    ['highlight.js', 'https://cdn.jsdelivr.net/gh/highlightjs/cdn-release@11.9.0/build/highlight.min.js'],
    ['mermaid', 'https://cdn.jsdelivr.net/npm/mermaid@10.9.1/dist/mermaid.min.js'],
  ]) {
    const code = curlHead(url);
    add(G, `CDN ${name}`, code.startsWith('2') ? 'pass' : 'warn', `HTTP ${code || 'no response'}`);
  }

  const s = settings();
  // LLM endpoint reachability.
  const ep = s && s.llm && s.llm.endpoint;
  if (ep) {
    const base = ep.replace(/\/+$/, '').replace(/\/(chat\/completions|v1)$/, '');
    const code = curlHead(`${base}/models`, 6) || curlHead(ep, 6);
    add(G, 'LLM endpoint', code && code !== '000' ? 'pass' : 'warn', `${ep} → HTTP ${code || 'unreachable'}`);
  } else add(G, 'LLM endpoint', 'skip', 'no llm.endpoint in settings');

  // Engram.
  const eng = s && s.engram && s.engram.url;
  if (s && s.engram && s.engram.enabled && eng) {
    const code = curlHead(eng, 6);
    add(G, 'Engram', code && code !== '000' ? 'pass' : 'warn', `${eng} → HTTP ${code || 'unreachable'}`);
  } else add(G, 'Engram', 'skip', 'disabled or unset');

  // Forge hosts.
  const forges = (s && s.forges) || [];
  if (!forges.length) add(G, 'Forge hosts', 'skip', 'none configured');
  for (const f of forges) {
    if (!f.base_url) continue;
    const ver = `${f.base_url.replace(/\/+$/, '')}/api/v1/version`;
    const code = curlHead(ver, 6);
    add(G, `Forge ${f.owner || f.kind}`, code && code !== '000' ? 'pass' : 'warn', `${f.base_url} → HTTP ${code || 'unreachable'}`);
  }
}

function tail(s, n = 12) { return String(s).trim().split('\n').slice(-n).join('\n'); }

// ── HTML report ──────────────────────────────────────────────────────────────
function esc(s) { return String(s).replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c])); }
function renderHtml(version, when) {
  const counts = { pass: 0, fail: 0, warn: 0, skip: 0 };
  results.forEach((r) => { counts[r.status]++; });
  const groups = [...new Set(results.map((r) => r.group))];
  const color = { pass: '#3fb950', fail: '#f85149', warn: '#d29922', skip: '#8b949e' };
  const icon = { pass: '✅', fail: '❌', warn: '⚠️', skip: '➖' };
  const rows = (g) => results.filter((r) => r.group === g).map((r) => `
    <tr>
      <td style="white-space:nowrap"><span style="color:${color[r.status]}">${icon[r.status]} ${r.status.toUpperCase()}</span></td>
      <td><strong>${esc(r.name)}</strong></td>
      <td><pre>${esc(r.detail || '')}</pre></td>
    </tr>`).join('');
  return `<!doctype html><html><head><meta charset="utf-8"><title>xNaut Pre-Flight — ${esc(version)}</title>
<style>
  body{font-family:-apple-system,Segoe UI,Roboto,sans-serif;background:#0d1117;color:#e6edf3;margin:0;padding:32px;line-height:1.5}
  h1{margin:0 0 4px} .sub{color:#8b949e;margin-bottom:20px;font-size:13px}
  .summary{display:flex;gap:10px;margin-bottom:24px;flex-wrap:wrap}
  .chip{padding:8px 16px;border-radius:10px;background:#161b22;border:1px solid #30363d;font-weight:600}
  h2{margin:24px 0 8px;font-size:16px;border-bottom:1px solid #30363d;padding-bottom:6px}
  table{width:100%;border-collapse:collapse} td{padding:6px 10px;border-bottom:1px solid #21262d;vertical-align:top;font-size:13px}
  pre{margin:0;white-space:pre-wrap;color:#8b949e;font-size:12px;font-family:SFMono-Regular,Menlo,monospace}
</style></head><body>
  <h1>xNaut Pre-Flight Check</h1>
  <div class="sub">version ${esc(version)} · ${esc(when)} · ${FAST ? 'fast mode (cargo skipped)' : 'full run'}</div>
  <div class="summary">
    <div class="chip" style="color:${color.pass}">✅ ${counts.pass} passed</div>
    <div class="chip" style="color:${color.fail}">❌ ${counts.fail} failed</div>
    <div class="chip" style="color:${color.warn}">⚠️ ${counts.warn} warnings</div>
    <div class="chip" style="color:${color.skip}">➖ ${counts.skip} skipped</div>
  </div>
  ${groups.map((g) => `<h2>${esc(g)}</h2><table>${rows(g)}</table>`).join('')}
  <p class="sub" style="margin-top:24px">A standalone harness verifies build/wiring/contracts/services — not GUI rendering, drag-drop, or native menus (test those manually).</p>
</body></html>`;
}

// ── Run ──────────────────────────────────────────────────────────────────────
console.log(`\nxNaut Pre-Flight Check ${FAST ? '(fast)' : ''}\n${'='.repeat(40)}`);
checkBuildAndTests();
checkAclCoverage();
checkVersions();
checkJsSyntax();
checkLoopsRenderer();
checkChatContracts();
checkIndexIncludes();
checkPaneFactories();
checkCsp();
checkServices();

const version = (read('src-tauri/tauri.conf.json').match(/"version"\s*:\s*"([^"]+)"/) || [])[1] || '?';
const when = new Date().toISOString().replace('T', ' ').slice(0, 19) + ' UTC';
const outPath = join(ROOT, 'preflight-report.html');
writeFileSync(outPath, renderHtml(version, when));

const fails = results.filter((r) => r.status === 'fail').length;
console.log(`${'='.repeat(40)}\nReport: ${outPath}\n${fails ? `❌ ${fails} failure(s)` : '✅ no failures'}\n`);
if (OPEN) sh(`open ${JSON.stringify(outPath)}`);
process.exit(fails ? 1 : 0);
