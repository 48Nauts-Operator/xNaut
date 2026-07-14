// Right-pane Workspace view — Phase 1 of the Workspace orchestrator (XNAUT-38).
//
// The shell: a right-pane view with two sub-tabs —
//   • Agentic — capture/preserve artifacts, links, decisions from the session
//     (wired to the transcript in Phase 2).
//   • Loops   — call a loop → GitVM sandbox, self-test/record/iterate
//     (wired to loops_run_start → sandbox.rs in Phase 3).
// State (active sub-tab) persists per project/root. Registers itself as the
// right-pane view 'workspace' (or queues if the host isn't up yet).
(function () {
  'use strict';

  function register(key, view) {
    if (typeof window.xnautRightPaneRegisterView === 'function') {
      window.xnautRightPaneRegisterView(key, view);
    } else {
      (window.__xnautRightPaneQueue = window.__xnautRightPaneQueue || []).push({ key, view });
    }
  }

  const invoke = (...a) => window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke(...a);
  function openTarget(t) { if (typeof window.xnautOpenUrl === 'function') window.xnautOpenUrl(t); else if (window.__TAURI__?.shell?.open) window.__TAURI__.shell.open(t); }
  function openInternalBrowser(url) {
    if (typeof window.xnautAttachBrowserTab === 'function') { window.xnautAttachBrowserTab(url); return true; }
    return false;
  }
  // Read a captured item in place: .md/.txt → in-app markdown viewer; .html and
  // artifact URLs → the internal browser tab; anything else → OS default.
  function openItem(it) {
    const t = it && it.target;
    if (!t) return;
    const isUrl = /^[a-z][a-z0-9+.-]*:\/\//i.test(t);
    if (it.kind !== 'artifact' && /\.(md|markdown|txt)$/i.test(t) && typeof window.xnautOpenMarkdownFile === 'function') {
      window.xnautOpenMarkdownFile(t);
      return;
    }
    if (isUrl || /\.html?$/i.test(t)) {
      const url = isUrl ? t : 'file://' + t;
      if (openInternalBrowser(url)) return;
    }
    openTarget(t);
  }

  function escapeText(s) {
    return String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
  }
  const basename = (p) => String(p || '').replace(/\/+$/, '').split('/').pop() || '';

  const SUBTABS = [
    ['agentic', 'History'],
    ['loops', 'Playbooks'],
  ];
  const SUBTAB_KEY = 'xnaut-workspace-subtab';

  // Playbook templates — reusable instruction sets that drive the agent through a
  // GitVM sandbox run. The `gitvm` CLI is on PATH (warm-up / run <cmd> / artifacts).
  // Running a playbook = compose(template + your workload) → push to the terminal agent.
  const PLAYBOOKS = [
    { id: 'build-verify', name: 'Build & Verify',
      desc: 'Warm up a GitVM sandbox, build + test until green, record a video, pull artifacts, report back.',
      body: [
        'Run the WORKLOAD below in a GitVM sandbox and only return when it is done AND tested. The `gitvm` CLI is on PATH:',
        '1. `gitvm warm-up` — spin up the sandbox (also syncs your claude/codex auth).',
        '2. Use `gitvm run <cmd>` for every build/test step — it rsyncs your uncommitted changes into /workspace and runs there.',
        '3. Do the work in the WORKLOAD. Iterate with `gitvm run <tests>` until green.',
        '4. To record proof it works, drive the UI HEADED (headed Playwright `headless:false`, or an Xfce terminal on display :0) — an SSH-only run records a blank screen. The recorder writes /artifacts/session-*.mp4.',
        '5. `gitvm artifacts pull` — bring back logs, the video, and outputs.',
        '6. Report what changed + test results + the artifact/video path, then `gitvm stop` to clean up.',
      ].join('\n') },
    { id: 'quick-run', name: 'Quick run',
      desc: 'Warm up, do the task in the sandbox, report. Minimal — no video.',
      body: [
        'Run the WORKLOAD below in a GitVM sandbox (`gitvm` CLI is on PATH):',
        '1. `gitvm warm-up`.',
        '2. Do the WORKLOAD; use `gitvm run <cmd>` for anything that executes code.',
        '3. `gitvm artifacts pull` if it produced outputs.',
        '4. Report a concise summary + results.',
      ].join('\n') },
    { id: 'ui-record', name: 'UI test + video',
      desc: 'Spin up agent-desktop, run the UI/Playwright flow, screen-record it, report with the video.',
      body: [
        'Run the UI WORKLOAD below in a GitVM agent-desktop sandbox and hand back a recording.',
        '1. `gitvm warm-up`.',
        '2. `gitvm run <ui/playwright cmd>` — the sandbox has Playwright + a browser + ffmpeg + noVNC.',
        '3. Record the run as a video of the actual UI.',
        '4. `gitvm artifacts pull` to fetch the video + screenshots.',
        '5. Report with the video path and what you observed.',
      ].join('\n') },
    { id: 'swarm', name: 'Swarm (parallel agents)',
      desc: 'Lead agent spawns specialist sub-agents in their own GitVM sandboxes, coordinating via the mesh, then synthesizes.',
      body: [
        'Act as the LEAD agent for the WORKLOAD below. Decompose it and run a swarm:',
        '1. `gitvm warm-up` your own sandbox.',
        '2. For each independent sub-task, spin up a NEW sandbox (`gitvm warm-up`) and launch a specialist there (e.g. `gitvm run codex exec "<sub-task>"`).',
        '3. Coordinate with the specialists over the Engram mesh (signed messages); collect their results.',
        '4. Each specialist self-tests + records its work; you synthesize, resolve conflicts, and may spawn a round 2.',
        '5. `gitvm artifacts pull` from each; dissolve the sandboxes when done.',
        '6. Report back a single synthesis + links to each artifact/video.',
      ].join('\n') },
    { id: 'blank', name: 'Blank',
      desc: 'No template — push exactly what you type to the agent.',
      body: '' },
  ];

  const ICON = {
    // artifact / link — Agentic empty state
    artifact: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.4" width="26" height="26"><path d="M8 12h8M8 8h8M8 16h5"/><rect x="4" y="3" width="16" height="18" rx="2"/></svg>',
    // sandbox / play — Loops empty state
    loop: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.4" width="26" height="26"><path d="M21 12a9 9 0 1 1-3-6.7"/><path d="M21 4v4h-4"/><path d="M10.5 9.5l4 2.5-4 2.5z" fill="currentColor" stroke="none"/></svg>',
  };

  function injectStyles() {
    if (document.getElementById('right-pane-workspace-styles')) return;
    const s = document.createElement('style');
    s.id = 'right-pane-workspace-styles';
    s.textContent = `
.rpws { display:flex; flex-direction:column; height:100%; min-height:0; }
.rpws-head { flex:0 0 auto; padding:9px 12px 0; }
.rpws-scope { font-size:11px; color:var(--muted-foreground); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
.rpws-scope strong { color:var(--foreground); font-weight:650; }
.rpws-nav { display:flex; gap:18px; padding:8px 12px 0; border-bottom:1px solid var(--border); flex:0 0 auto; }
.rpws-nav button { padding:6px 0 8px; border:0; border-bottom:2px solid transparent; background:transparent; color:var(--muted-foreground); font:inherit; font-size:12px; cursor:pointer; }
.rpws-nav button:hover { color:var(--foreground); }
.rpws-nav button.active { border-bottom-color:var(--xnaut-yellow); color:var(--foreground); font-weight:650; }
.rpws-body { flex:1 1 auto; min-height:0; overflow-y:auto; }
.rpws-page { display:none; flex-direction:column; height:100%; min-height:0; }
.rpws-page.active { display:flex; }
.rpws-empty { flex:1 1 auto; display:flex; flex-direction:column; align-items:center; justify-content:center; gap:10px; padding:28px 20px; text-align:center; color:var(--muted-foreground); }
.rpws-empty .rpws-ico { color:var(--muted-foreground); opacity:.55; }
.rpws-empty h3 { margin:2px 0 0; font-size:13px; color:var(--foreground); font-weight:650; }
.rpws-empty p { margin:0; font-size:12px; line-height:1.5; max-width:34ch; }
.rpws-phase { font-size:10px; letter-spacing:.04em; text-transform:uppercase; color:var(--muted-foreground); border:1px solid var(--border); border-radius:999px; padding:2px 8px; margin-top:4px; }
.rpws-actions { flex:0 0 auto; display:flex; gap:8px; padding:10px 12px; border-top:1px solid var(--border); }
.rpws-btn { flex:1 1 auto; min-height:32px; padding:6px 12px; border:1px solid var(--border); border-radius:var(--radius-md,8px); background:transparent; color:var(--foreground); font:inherit; font-size:12px; cursor:pointer; }
.rpws-btn:hover:not(:disabled) { background:var(--accent,#2a2a2f); }
.rpws-btn:disabled { opacity:.4; cursor:default; }
.rpws-btn-primary { border-color:var(--xnaut-yellow); background:var(--xnaut-yellow); color:var(--primary-foreground); font-weight:650; }
.rpws-feed-head { display:flex; align-items:center; gap:8px; padding:8px 12px 4px; flex:0 0 auto; }
.rpws-feed-head .rpws-count { font-size:10px; letter-spacing:.04em; text-transform:uppercase; color:var(--muted-foreground); white-space:nowrap; }
.rpws-session { flex:1 1 auto; min-width:0; height:26px; padding:2px 6px; border:1px solid var(--border); border-radius:var(--radius-md,6px); background:var(--secondary,#262626); color:var(--foreground); font:inherit; font-size:11px; outline:none; }
.rpws-session:focus { border-color:var(--xnaut-yellow); }
.rpws-icon-btn { width:26px; height:26px; display:flex; align-items:center; justify-content:center; border:1px solid transparent; border-radius:6px; background:transparent; color:var(--muted-foreground); cursor:pointer; }
.rpws-icon-btn:hover { background:var(--accent,#2a2a2f); color:var(--foreground); }
.rpws-icon-btn svg { width:14px; height:14px; }
.rpws-list { display:flex; flex-direction:column; gap:1px; padding:2px 8px 10px; }
.rpws-item { display:flex; align-items:center; gap:10px; padding:7px 9px; border-radius:var(--radius-md,8px); cursor:pointer; text-align:left; appearance:none; -webkit-appearance:none; background:transparent; border:0; border-left:2px solid transparent; color:inherit; }
.rpws-item:hover { background:var(--xnaut-yellow); background:color-mix(in srgb, var(--xnaut-yellow) 80%, #fff); }
.rpws-item:hover .rpws-item-label, .rpws-item:hover .rpws-item-sub { color:var(--primary-foreground,#171717); }
.rpws-item:hover .rpws-item-sub { opacity:.72; }
.rpws-item:hover .rpws-item-mark { background:rgba(0,0,0,.14); color:var(--primary-foreground,#171717); }
.rpws-item-mark { flex:0 0 auto; width:26px; height:26px; display:flex; align-items:center; justify-content:center; border-radius:6px; background:var(--bg-tertiary); color:var(--muted-foreground); }
.rpws-item-mark svg { width:15px; height:15px; }
.rpws-item-kind-artifact { border-left-color:var(--xnaut-yellow); }
.rpws-item-kind-artifact .rpws-item-mark { color:var(--xnaut-yellow); }
.rpws-item-copy { min-width:0; flex:1 1 auto; display:flex; flex-direction:column; gap:1px; }
.rpws-item-label { font-size:12.5px; font-weight:550; color:var(--foreground); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
.rpws-item-sub { font-size:10px; color:var(--muted-foreground); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
.rpws-pb-chips { display:flex; flex-wrap:wrap; gap:6px; padding:10px 12px 4px; flex:0 0 auto; }
.rpws-pb-chip { appearance:none; -webkit-appearance:none; background:transparent; color:var(--muted-foreground); border:1px solid var(--border); border-radius:999px; padding:4px 10px; font:inherit; font-size:11px; cursor:pointer; }
.rpws-pb-chip:hover { color:var(--foreground); }
.rpws-pb-chip.active { background:var(--xnaut-yellow); border-color:var(--xnaut-yellow); color:var(--primary-foreground,#171717); font-weight:650; }
.rpws-pb-desc { padding:4px 12px 8px; font-size:11px; color:var(--muted-foreground); line-height:1.45; flex:0 0 auto; }
.rpws-pb-workload { flex:1 1 auto; min-height:110px; margin:0 12px; padding:8px 10px; border:1px solid var(--border); border-radius:var(--radius-md,8px); background:var(--secondary,#262626); color:var(--foreground); font:inherit; font-size:12px; line-height:1.5; resize:none; outline:none; }
.rpws-pb-workload:focus { border-color:var(--xnaut-yellow); }
.rpws-pb-status { flex:0 0 auto; padding:0 12px 10px; font-size:11px; color:var(--muted-foreground); min-height:14px; }
.rpws-pb-overlay { position:fixed; inset:0; z-index:960; display:flex; align-items:center; justify-content:center; background:rgba(0,0,0,.5); }
.rpws-pb-dialog { width:min(360px,90vw); background:var(--card,#171717); border:1px solid var(--border); border-radius:10px; padding:16px; display:flex; flex-direction:column; gap:10px; box-shadow:0 20px 60px rgba(0,0,0,.5); }
.rpws-pb-dialog-head { font-size:12px; font-weight:650; color:var(--foreground); }
`;
    document.head.appendChild(s);
  }

  function createWorkspaceView() {
    let container = null;
    let root = null;
    let active = localStorage.getItem(SUBTAB_KEY) || 'agentic';
    let sessionId = '';        // '' = newest; else a specific session to backscan
    let sessions = [];
    let pbSelected = PLAYBOOKS[0].id;   // active playbook template

    function scopeKey() { return root ? `${SUBTAB_KEY}:${root}` : SUBTAB_KEY; }

    const REFRESH_ICON = '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M13.5 8a5.5 5.5 0 1 1-1.6-3.9"/><path d="M13.5 1.5v3h-3"/></svg>';
    const FILE_ICON = '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3"><path d="M4 1.5h5l3 3V14a.5.5 0 0 1-.5.5h-7.5A.5.5 0 0 1 3.5 14V2a.5.5 0 0 1 .5-.5z"/><path d="M9 1.5v3h3"/></svg>';
    const LINK_ICON = '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3"><path d="M6.5 9.5a2.5 2.5 0 0 0 3.5 0l2-2a2.5 2.5 0 0 0-3.5-3.5l-1 1"/><path d="M9.5 6.5a2.5 2.5 0 0 0-3.5 0l-2 2a2.5 2.5 0 0 0 3.5 3.5l1-1"/></svg>';

    function agenticPage() {
      // Live feed of clean captures: artifacts (links) + created documents.
      return `<div class="rpws-page" data-page="agentic">
        <div class="rpws-feed-head">
          <select class="rpws-session" data-session title="Session to read (backscan any past session)"></select>
          <span class="rpws-count" data-count></span>
          <button class="rpws-icon-btn" data-act="refresh" title="Rescan">${REFRESH_ICON}</button>
        </div>
        <div class="rpws-list" data-list>
          <div class="rpws-empty"><div class="rpws-ico">${ICON.artifact}</div><h3>Loading…</h3></div>
        </div>
      </div>`;
    }

    function relTime(ms) {
      if (!ms) return '';
      const s = Math.max(0, (Date.now() - ms) / 1000);
      if (s < 90) return 'just now';
      if (s < 5400) return `${Math.round(s / 60)}m ago`;
      if (s < 129600) return `${Math.round(s / 3600)}h ago`;
      return `${Math.round(s / 86400)}d ago`;
    }

    async function loadSessions() {
      const sel = container && container.querySelector('[data-session]');
      if (!sel || !root) return;
      try { sessions = (await invoke('workspace_sessions', { projectPath: root })) || []; }
      catch (_) { sessions = []; }
      const opts = ['<option value="">Newest session (auto)</option>'].concat(
        sessions.map((s) => {
          const glyph = s.agent === 'codex' ? '⬡' : '✳';
          return `<option value="${escapeText(s.id)}">${glyph} ${escapeText(s.title)} · ${relTime(s.updated_ms)}</option>`;
        }),
      );
      sel.innerHTML = opts.join('');
      sel.value = sessionId;
      sel.onchange = () => { sessionId = sel.value; loadItems(); };
    }

    async function loadAgentic() { await loadSessions(); await loadItems(); }

    async function loadItems() {
      const page = container && container.querySelector('.rpws-page[data-page="agentic"]');
      if (!page || !root) return;
      const list = page.querySelector('[data-list]');
      const count = page.querySelector('[data-count]');
      let items = [];
      const args = { projectPath: root };
      if (sessionId) args.session = sessionId;
      try { items = (await invoke('workspace_agentic_items', args)) || []; }
      catch (e) { console.error('[workspace] workspace_agentic_items failed', e); list.innerHTML = `<div class="rpws-empty"><p>${escapeText(String((e && e.message) || e))}</p></div>`; return; }
      count.textContent = items.length ? `${items.length} captured` : '';
      if (!items.length) {
        list.innerHTML = `<div class="rpws-empty">
          <div class="rpws-ico">${ICON.artifact}</div>
          <h3>Nothing captured yet</h3>
          <p>Artifacts and documents this session produces show up here — so a served HTML page or a report link survives scrollback. Brainstorm/idea capture lands next.</p>
          <span class="rpws-phase">Capture · artifacts + documents</span></div>`;
        return;
      }
      // Newest first.
      items = items.slice().reverse();
      list.innerHTML = items.map((it, i) => {
        const isArt = it.kind === 'artifact';
        const sub = isArt ? it.target : it.target.replace(/^.*\/(?=[^/]+\/[^/]+$)/, '…/');
        return `<button class="rpws-item rpws-item-kind-${escapeText(it.kind)}" data-i="${i}">
          <span class="rpws-item-mark">${isArt ? LINK_ICON : FILE_ICON}</span>
          <span class="rpws-item-copy"><span class="rpws-item-label">${escapeText(it.label)}</span><span class="rpws-item-sub">${escapeText(sub)}</span></span>
        </button>`;
      }).join('');
      list.querySelectorAll('.rpws-item').forEach((el) => {
        el.onclick = () => openItem(items[+el.dataset.i]);
      });
    }

    function loopsPage() {
      // Pick a playbook → attach a workload → push the composed instructions to
      // the terminal agent, which drives GitVM (warm-up/run/artifacts) itself.
      return `<div class="rpws-page" data-page="loops">
        <div class="rpws-pb-chips" data-pb-chips></div>
        <div class="rpws-pb-desc" data-pb-desc></div>
        <textarea class="rpws-pb-workload" data-pb-workload placeholder="Workload — the task for the agent to run in the GitVM sandbox (or Load a doc)."></textarea>
        <div class="rpws-actions">
          <button class="rpws-btn" data-act="pb-loaddoc">Load doc…</button>
          <button class="rpws-btn rpws-btn-primary" data-act="pb-run">Push playbook ▸</button>
        </div>
        <div class="rpws-pb-status" data-pb-status></div>
      </div>`;
    }

    async function loadPlaybooks() {
      const page = container && container.querySelector('.rpws-page[data-page="loops"]');
      if (!page) return;
      const chips = page.querySelector('[data-pb-chips]');
      const desc = page.querySelector('[data-pb-desc]');
      const workload = page.querySelector('[data-pb-workload]');
      const status = page.querySelector('[data-pb-status]');
      const sel = () => PLAYBOOKS.find((p) => p.id === pbSelected) || PLAYBOOKS[0];
      const paint = () => {
        chips.innerHTML = PLAYBOOKS.map((p) => `<button class="rpws-pb-chip${p.id === pbSelected ? ' active' : ''}" data-pb="${escapeText(p.id)}">${escapeText(p.name)}</button>`).join('');
        desc.textContent = sel().desc;
        chips.querySelectorAll('.rpws-pb-chip').forEach((b) => { b.onclick = () => { pbSelected = b.dataset.pb; paint(); }; });
      };
      paint();
      page.querySelector('[data-act="pb-run"]').onclick = () => {
        const wl = (workload.value || '').trim();
        if (!wl) { workload.focus(); return; }
        const pb = sel();
        const composed = pb.body ? `${pb.body}\n\nWORKLOAD:\n${wl}` : wl;
        const ok = typeof window.xnautPushToTerminal === 'function' && window.xnautPushToTerminal(composed);
        status.textContent = ok
          ? '↳ Pushed to the terminal agent — press Enter there to run it.'
          : 'No active terminal to push to — focus a terminal tab first.';
      };
      page.querySelector('[data-act="pb-loaddoc"]').onclick = () => loadDocInto(workload, status);
    }

    // Pick a doc from the work Vault and reference it in the workload.
    async function loadDocInto(workload, status) {
      let tree;
      try { tree = await invoke('vault_tree', { vault: 'work' }); }
      catch (_) { try { await invoke('vault_open', { vault: 'work' }); tree = await invoke('vault_tree', { vault: 'work' }); } catch (e) { if (status) status.textContent = 'Vault not available.'; return; } }
      const notes = ((tree && tree.notes) || []).filter((n) => /\.md$/i.test(n.rel || ''));
      if (!notes.length) { if (status) status.textContent = 'No vault docs found.'; return; }
      const ov = document.createElement('div');
      ov.className = 'rpws-pb-overlay';
      ov.innerHTML = `<div class="rpws-pb-dialog"><div class="rpws-pb-dialog-head">Load a doc into the workload</div>
        <select class="rpws-session" data-pb-doc>${notes.map((n) => `<option value="${escapeText(n.rel)}">${escapeText(n.title || n.rel)}</option>`).join('')}</select>
        <div class="rpws-actions"><button class="rpws-btn" data-x>Cancel</button><button class="rpws-btn rpws-btn-primary" data-ok>Add</button></div></div>`;
      container.appendChild(ov);
      const close = () => ov.remove();
      ov.querySelector('[data-x]').onclick = close;
      ov.onclick = (e) => { if (e.target === ov) close(); };
      ov.querySelector('[data-ok]').onclick = () => {
        const rel = ov.querySelector('[data-pb-doc]').value;
        const ref = `Read the doc at work:${rel} (in the vault) and follow it.`;
        workload.value = workload.value ? `${workload.value}\n${ref}` : ref;
        close();
      };
    }

    function render() {
      if (!container) return;
      injectStyles();
      active = localStorage.getItem(scopeKey()) || localStorage.getItem(SUBTAB_KEY) || 'agentic';
      if (!root) {
        container.innerHTML = `<div class="rpws"><div class="rpws-body"><div class="rpws-empty">
          <div class="rpws-ico">${ICON.artifact}</div><h3>No project open</h3>
          <p>Open a project (or focus a terminal in one) to use its Workspace.</p></div></div></div>`;
        return;
      }
      container.innerHTML = `<div class="rpws">
        <div class="rpws-head"><div class="rpws-scope">Workspace · <strong>${escapeText(basename(root))}</strong></div></div>
        <div class="rpws-nav">${SUBTABS.map(([k, label]) => `<button data-sub="${k}"${k === active ? ' class="active"' : ''}>${label}</button>`).join('')}</div>
        <div class="rpws-body">${agenticPage()}${loopsPage()}</div>
      </div>`;
      applyActive();
      if (active === 'agentic') loadAgentic();
      if (active === 'loops') loadPlaybooks();
      const refresh = container.querySelector('[data-act="refresh"]');
      if (refresh) refresh.onclick = () => loadAgentic();
      container.querySelectorAll('.rpws-nav button').forEach((b) => {
        b.onclick = () => {
          active = b.dataset.sub;
          localStorage.setItem(SUBTAB_KEY, active);
          localStorage.setItem(scopeKey(), active);
          container.querySelectorAll('.rpws-nav button').forEach((x) => x.classList.toggle('active', x.dataset.sub === active));
          applyActive();
          if (active === 'agentic') loadAgentic();
          if (active === 'loops') loadPlaybooks();
        };
      });
    }

    function applyActive() {
      container.querySelectorAll('.rpws-page').forEach((p) => p.classList.toggle('active', p.dataset.page === active));
    }

    return {
      mount(el, initialRoot) { container = el; root = initialRoot; render(); },
      setRoot(newRoot) { if (newRoot !== root) { root = newRoot; render(); } },
      destroy() { container = null; },
    };
  }

  register('workspace', createWorkspaceView());
})();
