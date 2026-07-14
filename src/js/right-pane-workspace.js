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

  function escapeText(s) {
    return String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
  }
  const basename = (p) => String(p || '').replace(/\/+$/, '').split('/').pop() || '';

  const SUBTABS = [
    ['agentic', 'Agentic'],
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
.rpws-scope { font-size:11px; color:var(--text-secondary,#9aa0aa); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
.rpws-scope strong { color:var(--text-primary,#e8eaf0); font-weight:650; }
.rpws-nav { display:flex; gap:18px; padding:8px 12px 0; border-bottom:1px solid var(--border,#2a2d34); flex:0 0 auto; }
.rpws-nav button { padding:6px 0 8px; border:0; border-bottom:2px solid transparent; background:transparent; color:var(--text-secondary,#9aa0aa); font:inherit; font-size:12px; cursor:pointer; }
.rpws-nav button.active { border-bottom-color:var(--xnaut-yellow,#f5b840); color:var(--text-primary,#fff); font-weight:650; }
.rpws-body { flex:1 1 auto; min-height:0; overflow-y:auto; }
.rpws-page { display:none; flex-direction:column; height:100%; min-height:0; }
.rpws-page.active { display:flex; }
.rpws-empty { flex:1 1 auto; display:flex; flex-direction:column; align-items:center; justify-content:center; gap:10px; padding:28px 20px; text-align:center; color:var(--text-secondary,#9aa0aa); }
.rpws-empty .rpws-ico { color:var(--text-muted,#6b7078); }
.rpws-empty h3 { margin:2px 0 0; font-size:13px; color:var(--text-primary,#e8eaf0); font-weight:650; }
.rpws-empty p { margin:0; font-size:12px; line-height:1.5; max-width:34ch; }
.rpws-phase { font-size:10px; letter-spacing:.04em; text-transform:uppercase; color:var(--text-muted,#6b7078); border:1px solid var(--border,#2a2d34); border-radius:999px; padding:2px 8px; margin-top:4px; }
.rpws-actions { flex:0 0 auto; display:flex; gap:8px; padding:10px 12px; border-top:1px solid var(--border,#2a2d34); }
.rpws-btn { flex:1 1 auto; min-height:32px; padding:6px 12px; border:1px solid var(--border,#3a3d45); border-radius:7px; background:transparent; color:var(--text-primary,#e8eaf0); font:inherit; font-size:12px; cursor:pointer; }
.rpws-btn:disabled { opacity:.45; cursor:default; }
.rpws-btn-primary { border-color:var(--xnaut-yellow,#f5b840); background:var(--xnaut-yellow,#f5b840); color:#1b1b1b; font-weight:650; }
`;
    document.head.appendChild(s);
  }

  function createWorkspaceView() {
    let container = null;
    let root = null;
    let active = localStorage.getItem(SUBTAB_KEY) || 'agentic';

    function scopeKey() { return root ? `${SUBTAB_KEY}:${root}` : SUBTAB_KEY; }

    function agenticPage() {
      // Phase 2 replaces this empty state with the curated capture feed.
      return `<div class="rpws-page" data-page="agentic">
        <div class="rpws-empty">
          <div class="rpws-ico">${ICON.artifact}</div>
          <h3>Nothing captured yet</h3>
          <p>Important artifacts, links, and decisions from this session will be preserved here — so a served HTML page or a choice you made survives scrollback.</p>
          <span class="rpws-phase">Capture · Phase 2</span>
        </div>
        <div class="rpws-actions">
          <button class="rpws-btn" data-act="load-doc" disabled title="Coming in Phase 2">Load a doc…</button>
          <button class="rpws-btn" data-act="push-terminal" disabled title="Coming in Phase 2">Push to terminal</button>
        </div>
      </div>`;
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
      container.querySelectorAll('.rpws-nav button').forEach((b) => {
        b.onclick = () => {
          active = b.dataset.sub;
          localStorage.setItem(SUBTAB_KEY, active);
          localStorage.setItem(scopeKey(), active);
          container.querySelectorAll('.rpws-nav button').forEach((x) => x.classList.toggle('active', x.dataset.sub === active));
          applyActive();
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
