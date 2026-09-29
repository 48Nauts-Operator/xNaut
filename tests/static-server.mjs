// Minimal zero-dependency static file server for the src/ frontend.
// Used by the Playwright tests and the demo to serve the app over http://.
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { extname, join, normalize } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = fileURLToPath(new URL('../src/', import.meta.url));
const PORT = Number(process.env.PORT || 4173);

// The real version, so the stub answers app.getVersion() the way the app does.
// Without it usage-footer.js renders no version at all and a test reads that as
// "the frontend never shows its version" — a false failure, filed once already.
const APP_VERSION = JSON.parse(
  await readFile(new URL('../src-tauri/tauri.conf.json', import.meta.url), 'utf8'),
).version;

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
  const AGENT = { handle:'builder', display_name:'Builder', tagline:'Turns product intent into working software.',
    purpose:'Build and validate xNaut features.', runtime_id:'codex', provider:'openai', model:'gpt-5.6-codex',
    reasoning_effort:'high',
    execution:'local', role:'coding-agent', capabilities:['terminal','code'], notifications:true,
    accent_color:'#f5b840', default_project:'/tmp/smoke', created_at:'2026-08-14T08:00:00Z', updated_at:'2026-08-14T08:00:00Z' };
  const NAUTBOT = { handle:'nautbot', display_name:'NautBot', tagline:'Your guide and control layer for xNaut.',
    purpose:'Guide and coordinate xNaut.', runtime_id:'codex', provider:'nautgate', model:'gpt-5.6-sol', reasoning_effort:'high',
    execution:'local', role:'core-orchestrator', capabilities:['guide','coordinate'], notifications:true, max_parallel:3,
    accent_color:'#f5b840', default_project:null, created_at:'2026-08-14T08:00:00Z', updated_at:'2026-08-14T08:00:00Z' };
  // Kept in step with tests/console-clean.spec.mjs. The shapes matter: a
  // too-thin stub does not merely under-test, it changes behaviour. Returning
  // null for settings_get instead of the shaped object sent the frontend into a
  // spin that pegged Chrome at 22% CPU and made the page undrivable.
  const BY = {
    pm_project_list: [PROJECT],
    // The panel's first load calls import_existing, not list (see
    // project-management-panel.js:4268). Without this the stub answers null, the
    // board paints with no projects, and the workspace is unreachable — which is
    // why "a project workspace opens and shows its tabs" was reported UNTESTED
    // by every run rather than being tested and passing.
    pm_project_import_existing: [PROJECT],
    pm_ticket_list: [{ id:'SMOKE-1', project:'SMOKE', title:'First project ticket', type:'feature', status:'ready', priority:'high', owner:'Builder', body:['The Issue','','Full ticket text shown after expansion.','','The Fix','','Use the corrected layout.'].join(String.fromCharCode(10)), updated_at:'2026-08-22T20:00:00Z' }],
    // XNAUT-153: Dispatch answers with the branch and worktree it opened.
    pm_ticket_dispatch: { ticket_id:'SMOKE-1', handle:'builder', branch:'agent/builder/smoke-1', worktree_path:'/tmp/smoke-worktrees/agent-builder-smoke-1', session_id:'smoke-dispatch' },
    // XNAUT-354: the Observatory reads dispatched runs off the registry rather
    // than a pane's in-memory queue, so the band needs real rows. The third has
    // no ticket on purpose: it is a session somebody opened, and the band must
    // not claim it as swarm work. zellij_session is null on all three so this
    // does not also feed the attribution join above it.
    run_registry_list: [
      { run_id:'01JSWARMA', kind:'agent', ticket:'SMOKE-1', project:'SMOKE', agent_handle:'builder',
        runtime_id:'codex', zellij_session:null, worktree_path:'/tmp/smoke-worktrees/agent-builder-smoke-1',
        branch:'agent/builder/smoke-1', state:'running', started_at:1757793600000, last_seen_at:1757793600000 },
      { run_id:'01JSWARMB', kind:'agent', ticket:'SMOKE-2', project:'SMOKE', agent_handle:'codex',
        runtime_id:'codex', zellij_session:null, worktree_path:'/tmp/smoke-worktrees/agent-codex-smoke-2',
        branch:'agent/codex/smoke-2', state:'done', started_at:1757790000000, last_seen_at:1757793000000 },
      { run_id:'01JSWARMC', kind:'agent', ticket:null, project:'SMOKE', agent_handle:'builder',
        runtime_id:'codex', zellij_session:null, worktree_path:'/tmp/smoke', branch:'',
        state:'running', started_at:1757789000000, last_seen_at:1757793000000 },
    ],
    swarm_plan_dispatch: { plan_id:'swarm-abc12345', project:'SMOKE',
      started:[
        { ticket:'SMOKE-1', handle:'builder', branch:'agent/builder/smoke-1', worktree_path:'/tmp/wt1', session_id:'s1' },
        { ticket:'SMOKE-2', handle:'codex', branch:'agent/codex/smoke-2', worktree_path:'/tmp/wt2', session_id:'s2' },
      ], failed:[] },
    git_ticket_files: [{ path:'src/example.js', status:'M', additions:12, deletions:3 }],
    // The real ModuleStatus shape (src-tauri/src/project_management.rs). The
    // old stub was ok/dirty/branch, three fields none of which exist,
    // so every test ran against a status the backend can never return.
    pm_module_status: { enabled: true, configured: true, valid: true,
      repo_path: '/tmp/smoke-control', remote_url: '', git_repository: true,
      project_count: 1, ticket_count: 0, error: '', warning: '',
      branch: 'main', last_commit: '', dirty: false, ahead: 0, behind: 0 },
    tasks_list: [],
    zellij_sessions_info: [],
    // XNAUT-86: where a session may be opened, and why not. The default is this
    // machine's real answer — local only, nothing else configured — because a
    // stub that claimed a ready sandbox would enable a button no test drives.
    launch_env_options: [
      { env: 'local', ready: true, detail: 'zellij on this machine; always available' },
      { env: 'exe-dev', ready: false, detail: 'no "exe-dev" entry in settings.sandboxes' },
      { env: 'gitvm', ready: false, detail: 'no "gitvm" entry in settings.sandboxes; a CLI key alone does not opt the fleet in' },
    ],
    agent_sessions_list: [],
    exe_machines: [],
    agent_profile_list: [NAUTBOT, AGENT],
    agent_profile_get: NAUTBOT,
    // Three runtimes, because a harness switch needs somewhere to switch TO
    // (XNAUT-150). One entry made the dropdown a single-option no-op.
    agent_list: [
      { id:'claude', label:'Claude Code', available:true, injection_mode:'argv' },
      { id:'codex', label:'Codex', available:true, injection_mode:'argv' },
      { id:'pi', label:'Pi', available:true, injection_mode:'argv' },
    ],
    skill_list: ['code-review'],
    chat_list_provider_models: [{ provider:'openai', model:'gpt-5.6-codex', label:'GPT-5.6 Codex' }],
    vault_init: '/tmp/.xnaut-vault',
    vault_open: null,
    vault_close: null,
    vault_tree: { dirs: ['Architecture', 'xnaut', 'xnaut/Development', 'xnaut/Development/features'], notes: [
      { rel:'Architecture/pm-space.md', stem_key:'pm-space', title:'PM Space', tags:['pinned'], mtime:2 },
      { rel:'welcome.md', stem_key:'welcome', title:'Welcome', tags:[], mtime:1 },
      // Three levels deep: the case that made the filter necessary, invisible
      // in the collapsed tree until you expand every folder by hand.
      { rel:'xnaut/Development/features/vault-filter.md', stem_key:'vault-filter', title:'Vault Filter', tags:[], mtime:3 },
    ] },
    vault_note_read: '# PM Space' + String.fromCharCode(10) + String.fromCharCode(10) + 'Related ticket: SMOKE-1.',
    vault_backlinks: [],
    // The Changes tab's four commands. Unstubbed they resolved null, which is
    // both what killed the pane and why vault-layout.spec.mjs:36-40 had never
    // once executed: an earlier assertion always failed first.
    git_uncommitted_files: [{ path:'src/example.js', additions:12, deletions:3, status:'M' }],
    git_outgoing_files: [],
    git_commit_log: [],
    git_worktree_list: [],
    vault_tags: [],
    get_home_directory: '/tmp',
    list_directory: [
      { name: 'README.md', path: '/tmp/smoke/README.md', is_directory: false },
      { name: 'src', path: '/tmp/smoke/src', is_directory: true },
    ],
    // Shaped like slice_diff.rs's SliceChanges / FileDiff. The merge base, not
    // the working tree, is what these answer — a stub that returned git status
    // would hide the whole reason the module exists (XNAUT-106). No backticks:
    // this whole block lives inside a template literal (see the NOTE below).
    slice_changes: {
      base: 'abc1234def', base_ref: 'feature/dashboard', branch: 'nautloom/engine', head: '9f81cc0',
      files: [
        { path: 'src/engine/chain.js', added: 84, removed: 12, status: 'modified' },
        { path: 'src/engine/README.md', added: 21, removed: 0, status: 'new' },
      ],
      added: 105, removed: 12, commits: 3,
    },
    slice_file_diff: {
      path: 'src/engine/chain.js', base: 'abc1234def', added: 2, removed: 1, truncated: false,
      lines: [
        { kind: 'hunk', n: 0, text: '@@ -14,3 +14,4 @@' },
        { kind: 'ctx', n: 14, text: 'export function chain(steps) {' },
        { kind: 'del', n: 0, text: '  return steps[0];' },
        { kind: 'add', n: 15, text: '  return steps.reduce(run, null);' },
      ],
    },
    create_terminal_session: { session_id:'smoke-terminal' },
    agent_profile_launch: { session_id:'smoke-agent', agent_id:'builder', injection_mode:'argv', conversation_id:null },
    agent_project_prepare: '/tmp/new-honey',
    // The chat-first flow (XNAUT-159): a message is answered by the agent's own
    // model. A test that wants the BUILD path overrides this one answer.
    agent_chat_turn: 'The release is tagged and the cask is on 1.15.0.',
    agent_build_workspace: '/tmp/smoke/.worktrees/run-the-checks',
    skill_catalog: [],
    canvas_get: { title: 'How an Agentic Loop Works', nodes: [
      { id:'observe', kind:'component', label:'1. Observe', description:'', x:60, y:60 },
      { id:'reason', kind:'agent', label:'2. Reason', description:'', x:320, y:60 },
    ], edges: [{ id:'e1', source:'observe', target:'reason', label:'context' }] },
    canvas_set: { title:'How an Agentic Loop Works', nodes: [], edges: [] },
    // NOTE: STUB_JS is a template literal, so an escape sequence here is
    // resolved before the browser ever sees it — a literal backslash-n becomes
    // a real newline and breaks this string. Build newlines at runtime.
    document_get: { title: 'Release notes',
      content: ['## What shipped', '', 'The canvas, and a document beside it.'].join(String.fromCharCode(10)) },
    document_set: { title: 'Release notes', content: '' },
    document_save_to_vault: '/Users/x/.xnaut-vault/work/xNAUT/Development/features/2026-08-15_Release-notes.md',
    // Echoes back what the UI sent, so a test can assert what was saved.
    plugin_save: null,
    plugin_connect: { ok: true, id: 'stripe', verified: 'the server started and stayed up' },
    agent_profile_get: null,
    plugin_catalog: [
      { id:'context7', name:'Context7', description:'Library docs', transport:'stdio', command:'npx', args:['-y','@upstash/context7-mcp'],
        url:'', headers:{}, category:'Docs & search', note:'', env:{}, required_env:[], enabled:true,
        docs_url:'https://github.com/upstash/context7', skills:[], seeded:true },
      { id:'stripe', name:'Stripe', description:'Payments', transport:'http', command:'', args:[],
        // No URL on purpose: that is what a plugin waiting to be configured
        // looks like, and it is the case where credentials get typed.
        url:'', headers:{}, category:'Business', note:'Use a restricted key.', env:{ STRIPE_KEY:'' }, required_env:['STRIPE_KEY'], enabled:false,
        docs_url:'https://docs.stripe.com/mcp', skills:[], seeded:true },
    ],
    // XNAUT-75: the startup diagnostics surface reads the log tail and reveals
    // the file. Unstubbed both resolve null, which the surface renders as
    // "debug.log is empty" — an honest answer, but not the one that proves the
    // tail reaches the pane.
    debug_log_tail: ['2026-09-09T09:00:00.000Z [error] [startup] step "chat sessions" failed',
      '2026-09-09T09:00:00.001Z [log] carrying on'].join(String.fromCharCode(10)),
    debug_log_reveal: '/tmp/xnaut/debug.log',
    terminal_output_snapshot: '',
    chat_send_provider: 'NautBot reply',
    chat_send_tools: 'NautBot reply',
    chat_check_endpoint: true,
    // Every non-Option field of the Rust Settings struct (src-tauri/src/settings.rs)
    // has to be here. Omitting engram threw
    // "Cannot read properties of undefined" out of the Tasks Mode settings
    // section -- the same class of bug as the old pm_module_status stub: not a
    // product defect, a fake that does not match what the backend can return.
    settings_get: {
      project_root: '/tmp/smoke', categories: [],
      llm: { provider: 'anthropic', model: '', endpoint: '', api_key: '' },
      llm_providers: [], engram: { enabled: false, url: '' },
      project_management: { enabled: false, repo_path: '', remote_url: '' },
      loops: {}, mcp_servers: [], forges: [], editor: '', mcp_port: 8791, mcp_token: '',
    },
    // XNAUT-370: the Observatory's instance card and ledger band. Shaped like
    // instance::Stamp and ledger::Entry, because a panel that reads a field the
    // backend does not send is the failure this stub exists to catch.
    instance_stamp: { id: 'inst-smoke-0001', role: 'fleet', version: '${APP_VERSION}', machine: 'smoke-box' },
    ledger_recent: [
      { at: '2026-09-14T09:00:00Z', kind: 'sweep_dispatch', agent: 'nautbot', ticket: 'SMOKE-1',
        detail: 'launched builder on agent/builder/smoke-1', session: '', elapsed_secs: 12,
        instance: 'inst-smoke-0001', role: 'fleet', version: '${APP_VERSION}' },
    ],
    projects_activity: [],
    audit_list: [],
    dag_step: { ready: [], unreachable: [], deadlocked: [] },
    dag_validate: [],
    designer_list: [],
    // The one place that builds a headless agent command line (XNAUT-266).
    // Every pane that used to paste its own now asks for this, so a null here
    // is a script with the word "null" in it rather than a run. What the real
    // line contains is asserted in Rust, beside the builder.
    agent_headless_command: 'claude -p --dangerously-skip-permissions "$(cat .loom-goal.txt)"',
  };
  // Exposed so a test can change one command's answer and re-open a panel,
  // rather than the server growing a query flag per scenario.
  window.__xnautStub = BY;
  window.__xnautInvokes = [];
  window.__xnautErrors  = [];
  const eventListeners = new Map();
  window.__xnautEmit = (name, payload) => {
    (eventListeners.get(name) || []).forEach((handler) => handler({ payload }));
  };
  window.__TAURI__ = {
    core: { invoke: (cmd, args) => {
      window.__xnautInvokes.push({ cmd, args });
      if (cmd === 'settings_set' && args?.settings) BY.settings_get = args.settings;
      if (cmd === 'conversation_store_load' && !Object.prototype.hasOwnProperty.call(BY, cmd)) {
        const records = JSON.parse(localStorage.getItem('__fixture_conversations') || '{}');
        for (const [key,value] of Object.entries(args.legacy || {})) if (!(key in records)) records[key]={value,revision:1};
        localStorage.setItem('__fixture_conversations',JSON.stringify(records)); return Promise.resolve(records);
      }
      if (cmd === 'conversation_store_put' && !Object.prototype.hasOwnProperty.call(BY, cmd)) {
        const records=JSON.parse(localStorage.getItem('__fixture_conversations') || '{}');
        if ((records[args.key]?.revision || 0)!==args.revision) return Promise.reject('Conversation changed in another window');
        const next={value:args.value,revision:args.revision+1};records[args.key]=next;
        localStorage.setItem('__fixture_conversations',JSON.stringify(records));return Promise.resolve(next);
      }

      const has = Object.prototype.hasOwnProperty.call(BY, cmd);
      const value = has ? BY[cmd] : null;
      // A stub of { __reject: "why" } fails the command instead of answering it.
      // Every stub used to resolve, so no test could reach a failure branch —
      // which is exactly where XNAUT-257 lived: panels that could not tell a
      // broken fetch from an honest empty. A plain object rather than an Error
      // because this crosses page.evaluate, which does not clone Errors.
      if (value && typeof value === 'object' && typeof value.__reject === 'string') {
        return Promise.reject(value.__reject);
      }
      // The real store collapses by id, sorts newest-first and truncates to the
      // caller's limit BEFORE anyone filters on status (nautloom.rs collapse_runs).
      // A stub that ignores the limit hides exactly the bug XNAUT-185 hit: a
      // window too small to hold the live runs drops agents off the Observatory.
      if (cmd === 'loom_runs_list' && Array.isArray(BY.loom_runs_list)) {
        const out = BY.loom_runs_list.slice().sort((x, y) => (y.started_ms || 0) - (x.started_ms || 0));
        return Promise.resolve(args && args.limit ? out.slice(0, args.limit) : out);
      }
      return Promise.resolve(value);
    } },
    event:  { listen: (name, handler) => {
      const handlers = eventListeners.get(name) || [];
      handlers.push(handler); eventListeners.set(name, handlers);
      return Promise.resolve(() => eventListeners.set(name, (eventListeners.get(name) || []).filter((item) => item !== handler)));
    }, emit: () => Promise.resolve() },
    window: { getCurrentWindow: () => ({ listen: () => Promise.resolve(noop) }) },
    app:    { getVersion: () => Promise.resolve('${APP_VERSION}') },
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
