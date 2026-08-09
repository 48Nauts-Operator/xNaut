// Minimal zero-dependency static file server for the src/ frontend.
// Used by the Playwright tests and the demo to serve the app over http://.
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { extname, join, normalize } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = fileURLToPath(new URL('../src/', import.meta.url));
const PORT = Number(process.env.PORT || 4173);

const TYPES = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.svg': 'image/svg+xml',
  '.ttf': 'font/ttf',
  '.json': 'application/json',
};


// The Tauri bridge stub, served at /__stub.js and injected as the FIRST script
// of index.html when the URL carries ?stub=1.
//
// It has to run before the app's modules: without a bridge they render nothing,
// and a plain load of index.html produces a body of five characters. Injected
// server-side rather than over CDP so every driver behaves identically, whether
// that is Browser Harness, Playwright, or a person opening the URL.
//
// It also records every invoke, so a test can assert on the conversation
// between frontend and backend rather than only on pixels.
const STUB_JS = `
(() => {
  const noop = () => {};
  const PROJECT = { key:'SMOKE', name:'Smoke Test', purpose:'exercise the panels',
    source_path:'/tmp/smoke', stage:'build', status:'active', tickets:[] };
  // Kept in step with tests/console-clean.spec.mjs. The shapes matter: a
  // too-thin stub does not merely under-test, it changes behaviour. Returning
  // null for settings_get instead of the shaped object sent the frontend into a
  // spin that pegged Chrome at 22% CPU and made the page undrivable.
  const BY = {
    pm_project_list: [PROJECT],
    pm_ticket_list: [],
    pm_change_list: [],
    pm_module_status: { ok: true, dirty: false, branch: 'main' },
    tasks_list: [],
    zellij_sessions_info: [],
    agent_sessions_list: [],
    settings_get: { llm: { provider: 'anthropic', model: '', endpoint: '' }, llm_providers: [], forges: [] },
    projects_activity: [],
    audit_list: [],
    dag_step: { ready: [], unreachable: [], deadlocked: [] },
    dag_validate: [],
    designer_list: [],
  };
  window.__xnautInvokes = [];
  window.__xnautErrors  = [];
  window.__TAURI__ = {
    core: { invoke: (cmd, args) => {
      window.__xnautInvokes.push({ cmd, args });
      return Promise.resolve(Object.prototype.hasOwnProperty.call(BY, cmd) ? BY[cmd] : null);
    } },
    event:  { listen: () => Promise.resolve(noop), emit: () => Promise.resolve() },
    window: { getCurrentWindow: () => ({ listen: () => Promise.resolve(noop) }) },
  };
  addEventListener('error', (e) => window.__xnautErrors.push(String(e.message)));
  addEventListener('unhandledrejection', (e) => window.__xnautErrors.push('unhandled: ' + e.reason));
})();
`;

const server = createServer(async (req, res) => {
  try {
    const wantStub = (req.url || '').includes('stub=1');
    let path = decodeURIComponent((req.url || '/').split('?')[0]);
    if (path === '/__stub.js') {
      res.writeHead(200, { 'Content-Type': 'text/javascript; charset=utf-8' });
      res.end(STUB_JS);
      return;
    }
    if (path === '/') path = '/index.html';
    const filePath = join(ROOT, normalize(path).replace(/^(\.\.[/\\])+/, ''));
    if (!filePath.startsWith(ROOT)) { res.writeHead(403).end('Forbidden'); return; }
    let body = await readFile(filePath);
    if (wantStub && filePath.endsWith('index.html')) {
      body = Buffer.from(String(body).replace(/<head(\s[^>]*)?>/i,
        (m) => m + '\n<script src="/__stub.js"></script>'));
    }
    res.writeHead(200, { 'Content-Type': TYPES[extname(filePath)] || 'application/octet-stream' });
    res.end(body);
  } catch (_) {
    res.writeHead(404, { 'Content-Type': 'text/plain' }).end('Not found');
  }
});

server.listen(PORT, () => console.log(`static-server: http://127.0.0.1:${PORT}`));
