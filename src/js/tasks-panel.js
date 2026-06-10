// Tasks panel — v1.6 forge-native issue/PR browser (Forgejo / GitHub / GitLab).
//
// Architecture mirrors markdown-pane.js: IIFE + 'use strict', window.xnaut*
// exports, pure-DOM pane that lives inside the parent webview. Data comes
// from the Rust side via invoke('forge_hosts') / invoke('forge_list_issues').
// Each row's "Start →" hands off to window.xnautOpenWorktreeModal
// (worktree-modal.js).
(function () {
  'use strict';

  // Lazy — the script may be parsed before the Tauri bridge is injected.
  const invoke = (...a) => window.__TAURI__.core.invoke(...a);

  // label -> { kind, label, pane }
  const panes = new Map();
  let labelCounter = 0;
  function nextLabel() {
    labelCounter += 1;
    return `tasks-${Date.now().toString(36)}-${labelCounter}`;
  }

  function escapeText(s) {
    return String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
  }

  // "47 days ago" from an ISO timestamp.
  function relativeTime(iso) {
    const t = Date.parse(iso);
    if (!Number.isFinite(t)) return '';
    const sec = Math.max(0, Math.floor((Date.now() - t) / 1000));
    const units = [['year', 31536000], ['month', 2592000], ['day', 86400], ['hour', 3600], ['minute', 60]];
    for (const [name, div] of units) {
      if (sec >= div) {
        const n = Math.floor(sec / div);
        return `${n} ${name}${n === 1 ? '' : 's'} ago`;
      }
    }
    return 'just now';
  }

  function openUrl(url) {
    if (!url) return;
    if (typeof window.xnautOpenUrl === 'function') window.xnautOpenUrl(url);
    else window.open(url);
  }

  const FORGE_ICONS = {
    forgejo: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="4.5" cy="12.5" r="1.7"/><circle cx="11.5" cy="3.5" r="1.7"/><circle cx="11.5" cy="9.5" r="1.7"/><path d="M4.5 10.8V8.5a3 3 0 0 1 3-3h2.2M4.5 10.8a5.3 5.3 0 0 1 5.2-1.3"/></svg>',
    github: '<svg viewBox="0 0 16 16" fill="currentColor"><path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27s1.36.09 2 .27c1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.01 8.01 0 0 0 16 8c0-4.42-3.58-8-8-8Z"/></svg>',
    gitlab: '<svg viewBox="0 0 16 16" fill="currentColor"><path d="m15.4 9.2-.8-2.5-1.6-4.9a.28.28 0 0 0-.52 0L10.9 6.7H5.1L3.52 1.8a.28.28 0 0 0-.52 0L1.4 6.7l-.8 2.5a.55.55 0 0 0 .2.62L8 14.7l7.2-4.88a.55.55 0 0 0 .2-.62Z"/></svg>',
  };
  const REFRESH_ICON = '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M13.5 8a5.5 5.5 0 1 1-1.6-3.9"/><path d="M13.5 1.5v3h-3"/></svg>';

  function injectStyles() {
    if (document.getElementById('tasks-panel-styles')) return;
    const st = document.createElement('style');
    st.id = 'tasks-panel-styles';
    st.textContent = `
.taskp-pane { display:flex; flex-direction:column; flex:1 1 0%; width:100%; height:100%; min-width:0; min-height:0; overflow:hidden; background:var(--editor-surface, #1b1d23); border-radius:var(--radius-md, 6px); color:var(--text, #d7dae0); font-size:13px; }
.taskp-toolbar { display:flex; flex-direction:column; gap:6px; padding:8px 10px; border-bottom:1px solid var(--border, rgba(255,255,255,.08)); flex:0 0 auto; }
.taskp-row { display:flex; align-items:center; gap:8px; min-width:0; }
.taskp-forges { display:flex; border:1px solid var(--border, rgba(255,255,255,.12)); border-radius:var(--radius-md, 6px); overflow:hidden; flex:0 0 auto; }
.taskp-forge { display:flex; align-items:center; gap:5px; padding:4px 9px; background:transparent; border:none; color:var(--text-muted, #8a8f98); cursor:pointer; font-size:12px; }
.taskp-forge svg { width:13px; height:13px; }
.taskp-forge + .taskp-forge { border-left:1px solid var(--border, rgba(255,255,255,.12)); }
.taskp-forge.active { background:var(--accent-bg, rgba(79,140,255,.18)); color:var(--text, #e8eaf0); }
.taskp-repo { flex:1 1 auto; min-width:80px; background:var(--input-bg, rgba(255,255,255,.05)); border:1px solid var(--border, rgba(255,255,255,.12)); border-radius:var(--radius-md, 6px); color:inherit; padding:4px 8px; font:inherit; outline:none; }
.taskp-repo:focus { border-color:var(--accent, #4f8cff); }
.taskp-refresh { flex:0 0 auto; background:transparent; border:1px solid var(--border, rgba(255,255,255,.12)); border-radius:var(--radius-md, 6px); color:var(--text-muted, #8a8f98); cursor:pointer; padding:4px 6px; display:flex; }
.taskp-refresh svg { width:13px; height:13px; }
.taskp-refresh:hover { color:var(--text, #e8eaf0); }
.taskp-tabs { display:flex; gap:2px; flex:0 0 auto; }
.taskp-tab { background:transparent; border:none; color:var(--text-muted, #8a8f98); padding:3px 9px; cursor:pointer; font:inherit; border-radius:var(--radius-md, 6px); }
.taskp-tab.active { color:var(--text, #e8eaf0); background:var(--accent-bg, rgba(79,140,255,.18)); }
.taskp-pills { display:flex; gap:4px; flex:0 0 auto; }
.taskp-pill { background:transparent; border:1px solid var(--border, rgba(255,255,255,.12)); border-radius:999px; color:var(--text-muted, #8a8f98); padding:2px 10px; font-size:12px; cursor:pointer; }
.taskp-pill.active { color:var(--text, #e8eaf0); border-color:var(--accent, #4f8cff); }
.taskp-username { width:120px; flex:0 0 auto; background:var(--input-bg, rgba(255,255,255,.05)); border:1px solid var(--border, rgba(255,255,255,.12)); border-radius:var(--radius-md, 6px); color:inherit; padding:3px 7px; font-size:12px; outline:none; }
.taskp-filter { flex:1 1 auto; min-width:60px; background:var(--input-bg, rgba(255,255,255,.05)); border:1px solid var(--border, rgba(255,255,255,.12)); border-radius:var(--radius-md, 6px); color:inherit; padding:3px 8px; font-size:12px; outline:none; }
.taskp-body { flex:1 1 0%; min-height:0; overflow:auto; }
.taskp-table { width:100%; border-collapse:collapse; }
.taskp-table th { text-align:left; font-size:11px; font-weight:600; text-transform:uppercase; letter-spacing:.04em; color:var(--text-muted, #8a8f98); padding:6px 10px; border-bottom:1px solid var(--border, rgba(255,255,255,.08)); position:sticky; top:0; background:var(--editor-surface, #1b1d23); }
.taskp-table td { padding:7px 10px; border-bottom:1px solid var(--border, rgba(255,255,255,.06)); vertical-align:top; }
.taskp-id { color:var(--text-muted, #8a8f98); white-space:nowrap; font-variant-numeric:tabular-nums; }
.taskp-title-link { cursor:pointer; color:var(--text, #e8eaf0); }
.taskp-title-link:hover { color:var(--accent, #4f8cff); text-decoration:underline; }
.taskp-meta { font-size:11px; color:var(--text-muted, #8a8f98); padding-top:2px; }
.taskp-chips { display:flex; flex-wrap:wrap; gap:4px; }
.taskp-chip { font-size:11px; padding:1px 7px; border-radius:999px; border:1px solid var(--border, rgba(255,255,255,.16)); color:var(--text-muted, #aab2bd); white-space:nowrap; }
.taskp-state { font-size:11px; padding:1px 8px; border-radius:999px; white-space:nowrap; }
.taskp-state-open { color:#3fb950; border:1px solid rgba(63,185,80,.4); }
.taskp-state-other { color:#a371f7; border:1px solid rgba(163,113,247,.4); }
.taskp-start { background:var(--accent-bg, rgba(79,140,255,.18)); border:1px solid var(--accent, #4f8cff); color:var(--text, #e8eaf0); border-radius:var(--radius-md, 6px); padding:3px 9px; font-size:12px; cursor:pointer; white-space:nowrap; }
.taskp-start:hover { background:var(--accent, #4f8cff); color:#fff; }
.taskp-msg { color:var(--text-muted, #8a8f98); padding:18px 10px; text-align:center; }
.taskp-msg.taskp-err { color:#f85149; text-align:left; white-space:pre-wrap; }
`;
    document.head.appendChild(st);
  }

  async function createTasksPanel(tabId, parentContainer, opts) {
    opts = opts || {};
    injectStyles();
    const label = nextLabel();

    const pane = document.createElement('div');
    pane.className = 'taskp-pane';
    pane.dataset.tasksLabel = label;
    pane.innerHTML = `
      <div class="taskp-toolbar">
        <div class="taskp-row">
          <div class="taskp-forges"></div>
          <input class="taskp-repo" type="text" placeholder="repo name" spellcheck="false">
          <button class="taskp-refresh" title="Refresh" aria-label="Refresh">${REFRESH_ICON}</button>
        </div>
        <div class="taskp-row">
          <div class="taskp-tabs">
            <button class="taskp-tab active" data-kind="issues">Issues</button>
            <button class="taskp-tab" data-kind="prs">PRs</button>
          </div>
          <div class="taskp-pills">
            <button class="taskp-pill active" data-pill="all">All</button>
            <button class="taskp-pill" data-pill="open">Open</button>
            <button class="taskp-pill" data-pill="mine">Mine</button>
          </div>
          <input class="taskp-username" type="text" placeholder="your username" spellcheck="false" hidden>
          <input class="taskp-filter" type="text" placeholder="filter by title…" spellcheck="false">
        </div>
      </div>
      <div class="taskp-body">
        <table class="taskp-table">
          <thead><tr><th>ID</th><th>Title</th><th>Labels</th><th>State</th><th></th></tr></thead>
          <tbody></tbody>
        </table>
      </div>`;
    parentContainer.appendChild(pane);

    const el = (sel) => pane.querySelector(sel);
    const forgesEl = el('.taskp-forges');
    const repoInput = el('.taskp-repo');
    const usernameInput = el('.taskp-username');
    const filterInput = el('.taskp-filter');
    const tbody = el('tbody');

    const state = {
      hosts: [], forgeIndex: 0, kind: 'issues', pill: 'all',
      items: [], visible: [], reqSeq: 0,
    };

    usernameInput.value = localStorage.getItem('xnaut-forge-username') || '';

    function messageRow(text, isErr) {
      tbody.innerHTML = `<tr><td colspan="5"><div class="taskp-msg${isErr ? ' taskp-err' : ''}">${escapeText(text)}</div></td></tr>`;
    }

    function renderRows() {
      const q = filterInput.value.trim().toLowerCase();
      const me = (localStorage.getItem('xnaut-forge-username') || '').trim();
      state.visible = state.items.filter((it) => {
        if (state.pill === 'open' && String(it.state).toLowerCase() !== 'open') return false;
        if (state.pill === 'mine' && it.author !== me) return false;
        if (q && !String(it.title || '').toLowerCase().includes(q)) return false;
        return true;
      });
      if (!state.visible.length) { messageRow('No items match.'); return; }
      tbody.innerHTML = state.visible.map((it, i) => {
        const stateCls = String(it.state).toLowerCase() === 'open' ? 'taskp-state-open' : 'taskp-state-other';
        const chips = (it.labels || []).map((l) => `<span class="taskp-chip">${escapeText(l)}</span>`).join('');
        return `<tr>
          <td class="taskp-id">#${escapeText(it.number)}</td>
          <td><div class="taskp-title-link" data-idx="${i}">${escapeText(it.title)}</div>
              <div class="taskp-meta">${escapeText(it.author || '')} · ${escapeText(relativeTime(it.updated_at))}</div></td>
          <td><div class="taskp-chips">${chips}</div></td>
          <td><span class="taskp-state ${stateCls}">${escapeText(it.state)}</span></td>
          <td><button class="taskp-start" data-idx="${i}">Start →</button></td>
        </tr>`;
      }).join('');
    }

    async function refresh() {
      const repo = repoInput.value.trim();
      if (!repo) { state.items = []; messageRow('Enter a repo name to load issues.'); return; }
      const seq = ++state.reqSeq;
      messageRow('Loading…');
      try {
        const items = await invoke('forge_list_issues', { forgeIndex: state.forgeIndex, repo, kind: state.kind });
        if (seq !== state.reqSeq) return; // superseded
        state.items = items || [];
        renderRows();
      } catch (e) {
        if (seq !== state.reqSeq) return;
        messageRow(String(e), true);
      }
    }

    function selectForge(idx) {
      state.forgeIndex = idx;
      forgesEl.querySelectorAll('.taskp-forge').forEach((b, i) => b.classList.toggle('active', i === idx));
      repoInput.value = localStorage.getItem(`xnaut-tasks-repo-${idx}`) || '';
      refresh();
    }

    function renderForges() {
      forgesEl.innerHTML = state.hosts.map((h, i) =>
        `<button class="taskp-forge" data-idx="${i}" title="${escapeText(h.base_url || '')}">${FORGE_ICONS[h.kind] || FORGE_ICONS.forgejo}<span>${escapeText(h.owner || h.kind)}</span></button>`
      ).join('');
      forgesEl.querySelectorAll('.taskp-forge').forEach((b) => {
        b.onclick = () => selectForge(parseInt(b.dataset.idx, 10));
      });
    }

    // ── wiring ──────────────────────────────────────────────────────────
    repoInput.addEventListener('change', () => {
      localStorage.setItem(`xnaut-tasks-repo-${state.forgeIndex}`, repoInput.value.trim());
      refresh();
    });
    repoInput.addEventListener('keydown', (e) => { if (e.key === 'Enter') repoInput.blur(); });
    el('.taskp-refresh').onclick = refresh;

    pane.querySelectorAll('.taskp-tab').forEach((b) => {
      b.onclick = () => {
        state.kind = b.dataset.kind;
        pane.querySelectorAll('.taskp-tab').forEach((x) => x.classList.toggle('active', x === b));
        refresh();
      };
    });
    pane.querySelectorAll('.taskp-pill').forEach((b) => {
      b.onclick = () => {
        state.pill = b.dataset.pill;
        pane.querySelectorAll('.taskp-pill').forEach((x) => x.classList.toggle('active', x === b));
        usernameInput.hidden = state.pill !== 'mine';
        renderRows();
      };
    });
    usernameInput.addEventListener('input', () => {
      localStorage.setItem('xnaut-forge-username', usernameInput.value.trim());
      renderRows();
    });
    filterInput.addEventListener('input', renderRows);

    tbody.addEventListener('click', (e) => {
      const title = e.target.closest('.taskp-title-link');
      if (title) { const it = state.visible[+title.dataset.idx]; if (it) openUrl(it.html_url); return; }
      const start = e.target.closest('.taskp-start');
      if (start) {
        const it = state.visible[+start.dataset.idx];
        if (!it) return;
        if (typeof window.xnautOpenWorktreeModal !== 'function') {
          console.warn('[tasks-panel] xnautOpenWorktreeModal not loaded (worktree-modal.js missing?)');
          return;
        }
        window.xnautOpenWorktreeModal({ forgeIndex: state.forgeIndex, repo: repoInput.value.trim(), issue: it });
      }
    });

    // ── initial load ────────────────────────────────────────────────────
    try {
      state.hosts = (await invoke('forge_hosts')) || [];
      if (!state.hosts.length) {
        messageRow('No forges configured. Add one in Settings → Forges.');
      } else {
        renderForges();
        selectForge(0);
      }
    } catch (e) {
      messageRow(String(e), true);
    }

    const entry = { kind: 'tasks', label, pane };
    panes.set(label, entry);
    return entry;
  }

  function destroyTasksPanel(label) {
    const entry = panes.get(label);
    if (!entry) return;
    if (entry.pane && entry.pane.parentNode) entry.pane.parentNode.removeChild(entry.pane);
    panes.delete(label);
  }

  // Public API hooks (mirror the markdown-pane shape).
  window.xnautCreateTasksPanel = createTasksPanel;
  window.xnautDestroyTasksPanel = destroyTasksPanel;
})();
