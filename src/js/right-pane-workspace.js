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
  // Read a captured item in place: markdown/text docs open in the in-app rendered
  // viewer; html/pdf/etc and artifact URLs open with the OS default (browser renders).
  function openItem(it) {
    const t = it && it.target;
    if (!t) return;
    if (it.kind !== 'artifact' && /\.(md|markdown|txt)$/i.test(t) && typeof window.xnautOpenMarkdownFile === 'function') {
      window.xnautOpenMarkdownFile(t);
      return;
    }
    openTarget(t);
  }

  function escapeText(s) {
    return String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
  }
  const basename = (p) => String(p || '').replace(/\/+$/, '').split('/').pop() || '';

  const SUBTABS = [
    ['agentic', 'History'],
    ['loops', 'Loops'],
  ];
  const SUBTAB_KEY = 'xnaut-workspace-subtab';

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
`;
    document.head.appendChild(s);
  }

  function createWorkspaceView() {
    let container = null;
    let root = null;
    let active = localStorage.getItem(SUBTAB_KEY) || 'agentic';
    let sessionId = '';        // '' = newest; else a specific session to backscan
    let sessions = [];

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
      // Phase 3 wires this to loops_run_start → GitVM sandbox.
      return `<div class="rpws-page" data-page="loops">
        <div class="rpws-empty">
          <div class="rpws-ico">${ICON.loop}</div>
          <h3>No loop runs yet</h3>
          <p>Call a loop to run this workspace's task in a GitVM sandbox — the agent self-tests, screen-records, and iterates until done, then posts results back to Agentic.</p>
          <span class="rpws-phase">Execute · Phase 3</span>
        </div>
        <div class="rpws-actions">
          <button class="rpws-btn rpws-btn-primary" data-act="execute" disabled title="Coming in Phase 3">Execute in sandbox</button>
        </div>
      </div>`;
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
