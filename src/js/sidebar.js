// Left sidebar — v1.6 Orca-style navigation rail.
//
// Architecture mirrors markdown-pane.js: IIFE module, window.xnaut* exports,
// inline-SVG icon buttons, scoped <style> injected once, defensive Tauri
// access. app.js owns mounting (calls xnautMountSidebar) and panel routing
// (provides window.xnautSidebarNavigate); this module owns rendering and
// sidebar-local state (active nav row, pinned projects, context menu).
(function () {
  'use strict';

  const invoke = (...a) => window.__TAURI__.core.invoke(...a);

  const PIN_KEY = 'xnaut-pinned-projects';

  function escapeText(s) {
    return String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
  }

  function navigate(key, payload) {
    if (typeof window.xnautSidebarNavigate === 'function') {
      try { window.xnautSidebarNavigate(key, payload); } catch (e) { console.error('[sidebar] navigate failed:', e); }
    } else {
      console.warn('[sidebar] xnautSidebarNavigate not wired yet (key:', key, ')');
    }
  }

  // ---------- pin state ----------
  function loadPins() {
    try {
      const v = JSON.parse(localStorage.getItem(PIN_KEY) || '[]');
      return Array.isArray(v) ? v : [];
    } catch (_) { return []; }
  }
  function savePins(pins) {
    try { localStorage.setItem(PIN_KEY, JSON.stringify(pins)); } catch (_) { /* quota — ignore */ }
  }

  // ---------- icons ----------
  const SVG_ATTRS = 'viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"';
  const ICONS = {
    control: `<svg ${SVG_ATTRS}><path d="M2.5 5.5h11v7h-11z"/><path d="M5 5.5V3h6v2.5M5 9h2M9 9h2"/></svg>`,
    agents: `<svg ${SVG_ATTRS}><circle cx="8" cy="5" r="2.5"/><path d="M3.5 13c.5-2.7 2-4 4.5-4s4 1.3 4.5 4"/></svg>`,
    observatory: `<svg ${SVG_ATTRS}><circle cx="8" cy="8" r="5.5"/><circle cx="8" cy="8" r="2"/><path d="M8 2.5V1M8 15v-1.5M2.5 8H1M15 8h-1.5"/></svg>`,
    tasks: `<svg ${SVG_ATTRS}><path d="M3 4.5l1.5 1.5L7 3.5"/><line x1="9" y1="4.5" x2="13" y2="4.5"/><path d="M3 10.5l1.5 1.5L7 9.5"/><line x1="9" y1="10.5" x2="13" y2="10.5"/></svg>`,
    automations: `<svg ${SVG_ATTRS}><path d="M8.5 2L4 9h3.5L7 14l5-7H8.5l.5-5z"/></svg>`,
    pm: `<svg ${SVG_ATTRS}><rect x="2.5" y="5" width="11" height="8" rx="1.5"/><path d="M6 5V3.5h4V5"/></svg>`,
    vault: `<svg ${SVG_ATTRS}><path d="M3 3.5h7.5a2 2 0 0 1 2 2V13H5a2 2 0 0 1-2-2V3.5z"/><path d="M5.5 3.5V13"/></svg>`,
    search: `<svg ${SVG_ATTRS}><circle cx="7" cy="7" r="4"/><line x1="10" y1="10" x2="13.5" y2="13.5"/></svg>`,
    plus: `<svg ${SVG_ATTRS}><line x1="8" y1="3" x2="8" y2="13"/><line x1="3" y1="8" x2="13" y2="8"/></svg>`,
    refresh: `<svg ${SVG_ATTRS}><path d="M13 8a5 5 0 1 1-1.5-3.5"/><path d="M13 2v3h-3"/></svg>`,
  };

  const NAV_ITEMS = [
    { key: 'control-center', label: 'Control Center', icon: 'control' },
    { key: 'agents', label: 'Agent Space' },
    { key: 'observatory', label: 'Observatory' },
    { key: 'tasks', label: 'Tasks' },
    { key: 'automations', label: 'Automations' },
    { key: 'pm', label: 'Projects' },
    { key: 'vault', label: 'Vault' },
    { key: 'search', label: 'Search' },
  ];

  // ---------- styles ----------
  function injectStyles() {
    if (document.getElementById('sidebar-styles')) return;
    const style = document.createElement('style');
    style.id = 'sidebar-styles';
    style.textContent = `
      .sbar-root { display: flex; flex-direction: column; height: 100%; min-height: 0; overflow: hidden;
        background: var(--editor-surface, #1b1b1f); color: var(--text-primary, #ddd);
        border-right: 1px solid var(--border-color, #333); font-size: 13px; user-select: none; }
      .sbar-nav { display: flex; flex-direction: column; padding: 8px 6px 4px; gap: 1px; }
      .sbar-nav-row { display: flex; align-items: center; gap: 8px; padding: 6px 8px; border-radius: 6px;
        cursor: pointer; color: var(--text-secondary, #aaa); }
      .sbar-nav-row:hover { background: var(--hover-bg, rgba(255,255,255,0.06)); }
      .sbar-nav-row.sbar-active { background: var(--active-bg, rgba(255,255,255,0.1)); color: var(--text-primary, #fff); }
      .sbar-nav-row svg, .sbar-icon-btn svg { width: 15px; height: 15px; flex: 0 0 auto; }
      .sbar-section-head { display: flex; align-items: center; justify-content: space-between;
        padding: 10px 14px 4px 14px; font-size: 11px; font-weight: 600; letter-spacing: 0.06em;
        text-transform: uppercase; color: var(--text-muted, #777); }
      .sbar-section-head.sbar-collapsible { cursor: pointer; }
      .sbar-section-head.sbar-collapsible:hover { color: var(--text-secondary, #9aa0aa); }
      .sbar-head-label { display: flex; align-items: center; gap: 4px; }
      .sbar-caret { display: inline-block; width: 9px; font-size: 9px; line-height: 1; opacity: .7; transition: transform .15s ease; }
      .sbar-section-head.sbar-collapsed .sbar-caret { transform: rotate(-90deg); }
      .sbar-icon-btn { display: flex; align-items: center; justify-content: center; width: 22px; height: 22px;
        border: none; border-radius: 5px; background: transparent; color: var(--text-secondary, #aaa); cursor: pointer; padding: 0; }
      .sbar-icon-btn:hover { background: var(--hover-bg, rgba(255,255,255,0.08)); color: var(--text-primary, #fff); }
      .sbar-projects { flex: 1 1 0%; min-height: 0; overflow-y: auto; padding: 2px 6px 8px; }
      .sbar-agents { flex:0 1 auto; max-height:42%; min-height:0; overflow-y:auto; padding:2px 6px 6px; }
      .sbar-agent { position:relative; display:flex; align-items:center; gap:8px; padding:6px 8px; border-radius:6px; cursor:pointer; }
      .sbar-agent:hover,.sbar-thread:hover { background:var(--hover-bg,rgba(255,255,255,.06)); }
      .sbar-agent-avatar { display:grid; place-items:center; width:25px; height:25px; flex:0 0 auto; border-radius:7px;
        color:#fff; background:var(--agent-accent,#666); font-size:9px; font-weight:750; }
      .sbar-agent-copy { flex:1 1 auto; min-width:0; } .sbar-agent-name { overflow:hidden; color:var(--text-primary,#e7e7eb); font-size:12px; text-overflow:ellipsis; white-space:nowrap; }
      .sbar-agent-meta { display:flex; align-items:center; gap:5px; margin-top:1px; color:var(--text-muted,#74747e); font-size:10px; }
      .sbar-agent-status { width:6px; height:6px; border-radius:50%; background:#676771; }.sbar-agent-status.working { background:#4da3ff; }.sbar-agent-status.permission,.sbar-agent-status.blocked { background:#ff5f56; }
      .sbar-agent-more { display:grid; place-items:center; width:20px; height:20px; border:0; border-radius:5px; color:var(--text-muted,#74747e); background:transparent; cursor:pointer; opacity:0; }
      .sbar-agent:hover .sbar-agent-more,.sbar-agent-more:focus { opacity:1; }.sbar-agent-more:hover { color:var(--text-primary,#eee); background:rgba(255,255,255,.08); }
      .sbar-threads { margin:0 0 4px 33px; border-left:1px solid var(--border-color,#303038); }.sbar-thread { padding:4px 8px; overflow:hidden; color:var(--text-muted,#83838d); font-size:10px; text-overflow:ellipsis; white-space:nowrap; cursor:pointer; }
      .sbar-thread-new { color:var(--text-secondary,#a0a0aa); }
      .sbar-sub-label { padding: 6px 8px 2px; font-size: 10px; letter-spacing: 0.05em; text-transform: uppercase;
        color: var(--text-muted, #666); }
      .sbar-row { display: flex; align-items: flex-start; gap: 8px; padding: 6px 8px; border-radius: 6px; cursor: pointer; }
      .sbar-row:hover { background: var(--hover-bg, rgba(255,255,255,0.06)); }
      .sbar-row-active { background: var(--active-bg, rgba(255,255,255,0.1)); box-shadow: inset 2px 0 0 var(--agent-thinking, #4dffd0); }
      .sbar-dot { flex: 0 0 auto; width: 7px; height: 7px; margin-top: 5px; border-radius: 50%;
        background: var(--dot-off, #555); }
      .sbar-dot.sbar-on { background: var(--dot-on, #3fb950); }
      .sbar-row-main { flex: 1 1 auto; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
      .sbar-row-top { display: flex; align-items: center; gap: 6px; min-width: 0; }
      .sbar-name { flex: 1 1 auto; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
      .sbar-chip { flex: 0 0 auto; font-size: 10px; padding: 1px 6px; border-radius: 8px;
        background: var(--chip-bg, rgba(255,255,255,0.08)); color: var(--text-secondary, #999); }
      .sbar-branch { font-size: 11px; color: var(--text-muted, #777); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
      /* Separated by a dot only when there is a branch to separate it FROM. */
      .sbar-ago { color: var(--text-muted, #666); }
      .sbar-branch-name:not(:empty) + .sbar-ago:not(:empty)::before { content: ' · '; }
      .sbar-empty { padding: 10px 8px; color: var(--text-muted, #666); font-size: 12px; }
      .sbar-sess { background: rgba(120,180,255,.12); color: #7fb2ff; border-color: transparent; }
      /* State as a hairline around the row. One pixel on purpose: it should be
         readable in peripheral vision without competing with the text. Only the
         states worth reacting to get one — idle projects stay unmarked, or the
         whole rail turns into noise you learn to ignore. */
      .sbar-row[data-state] { box-shadow: inset 0 0 0 1px var(--sbar-state, transparent); }
      /* Needs attention: the row outlines itself in yellow and pulses. A pulse
         reads as "still waiting" — it repeats without implying progress, which
         is what a travelling snake implies. Same 1px weight as the other states. */
      .sbar-row { position: relative; }
      .sbar-row[data-state="attention"] { --sbar-state: transparent; }
      .sbar-row[data-state="attention"]::after {
        content: ''; position: absolute; inset: 0; border-radius: 6px;
        pointer-events: none; box-sizing: border-box;
        box-shadow: inset 0 0 0 1px #ffd166;
        animation: sbar-pulse 1.6s ease-in-out infinite; }
      @keyframes sbar-pulse { 0%, 100% { opacity: 1; } 50% { opacity: .28; } }
      @media (prefers-reduced-motion: reduce) {
        .sbar-row[data-state="attention"]::after { animation: none; }
      }
      .sbar-row[data-state="working"]   { --sbar-state: rgba(245,184,64,.75); }
      .sbar-row[data-state="waiting"]   { --sbar-state: rgba(245,184,64,.40); }
      .sbar-row[data-state="done"]      { --sbar-state: rgba(63,185,80,.55); }
      .sbar-row[data-state="live"]      { --sbar-state: rgba(140,146,158,.28); }
      .sbar-row[data-state="exited"]    { --sbar-state: rgba(140,146,158,.22); }
      /* A conic-gradient sweeps by ANGLE, so it rotates like a pie no matter what
         shape you mask it into — that is why the last version still read as a
         circle. This moves an actual block around the four edges instead: eight
         positions, stepped, so it hops corner to corner along a square path. */
      .sbar-dot.sbar-run { position: relative; width: 12px; height: 12px; margin-top: 3px;
        border-radius: 2px; background: transparent;
        box-shadow: inset 0 0 0 1px rgba(77,163,255,.22); animation: none; }
      .sbar-dot.sbar-run::before, .sbar-dot.sbar-run::after {
        content: ''; position: absolute; width: 3px; height: 3px; border-radius: 1px;
        background: #4da3ff; animation: sbar-square 1.2s steps(1) infinite; }
      /* the tail: same path, one step behind, dimmer */
      .sbar-dot.sbar-run::after { background: rgba(77,163,255,.35); animation-delay: -0.15s; }
      @keyframes sbar-square {
        0%    { top: 0;     left: 0; }
        12.5% { top: 0;     left: 4.5px; }
        25%   { top: 0;     left: 9px; }
        37.5% { top: 4.5px; left: 9px; }
        50%   { top: 9px;   left: 9px; }
        62.5% { top: 9px;   left: 4.5px; }
        75%   { top: 9px;   left: 0; }
        87.5% { top: 4.5px; left: 0; }
        100%  { top: 0;     left: 0; }
      }
      /* Alive but nothing known to be happening — present, quiet, no motion. */
      .sbar-dot.sbar-live { border-radius: 2px; background: #8c929e; }
      /* Needs a human. The only state allowed to be loud. */
      .sbar-dot.sbar-attention { border-radius: 2px; background: #ff5f56;
        box-shadow: 0 0 0 3px rgba(255,95,86,.18); }
      /* Resurrectable but not running — a hollow square, no motion. */
      .sbar-dot.sbar-exited { width: 10px; height: 10px; margin-top: 4px; border-radius: 2px;
        background: transparent; box-shadow: inset 0 0 0 1.5px #5c626c; }
      @media (prefers-reduced-motion: reduce) {
        .sbar-dot.sbar-run { animation: none; background-image: none; background: #4da3ff; }
      }
      .sbar-hidden-toggle { padding: 7px 8px; margin-top: 2px; color: var(--text-muted, #666);
        font-size: 11px; cursor: pointer; border-radius: 6px; user-select: none; }
      .sbar-hidden-toggle:hover { background: rgba(255,255,255,.05); color: var(--text-secondary, #a0a5af); }
      .sbar-usage { flex: 0 0 auto; display: flex; align-items: center; gap: 6px; padding: 8px 10px;
        border-top: 1px solid var(--border-color, #333); }
      .sbar-usage-rows { flex: 1 1 auto; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
      .sbar-usage-row { font-size: 11px; color: var(--text-secondary, #999); overflow: hidden;
        text-overflow: ellipsis; white-space: nowrap; }
      .sbar-usage-row.sbar-muted { color: var(--text-muted, #666); }
      .sbar-menu { position: fixed; z-index: 10000; min-width: 160px; padding: 4px;
        background: var(--menu-bg, #222227); border: 1px solid var(--border-color, #3a3a40);
        border-radius: 8px; box-shadow: 0 8px 24px rgba(0,0,0,0.45); font-size: 13px; }
      .sbar-menu-item { padding: 6px 10px; border-radius: 5px; cursor: pointer; color: var(--text-primary, #ddd); }
      .sbar-menu-item:hover { background: var(--hover-bg, rgba(255,255,255,0.08)); }
      .sbar-menu-item.sbar-danger { color: var(--danger, #e5534b); }
    `;
    document.head.appendChild(style);
  }

  // ---------- context menu ----------
  let menuEl = null;
  function closeMenu() {
    if (menuEl && menuEl.parentNode) menuEl.parentNode.removeChild(menuEl);
    menuEl = null;
  }
  function openMenu(x, y, items) {
    closeMenu();
    menuEl = document.createElement('div');
    menuEl.className = 'sbar-menu';
    for (const it of items) {
      const row = document.createElement('div');
      row.className = 'sbar-menu-item' + (it.danger ? ' sbar-danger' : '');
      row.textContent = it.label;
      row.addEventListener('click', (e) => { e.stopPropagation(); closeMenu(); it.action(); });
      menuEl.appendChild(row);
    }
    document.body.appendChild(menuEl);
    // Keep on-screen.
    const r = menuEl.getBoundingClientRect();
    menuEl.style.left = Math.min(x, window.innerWidth - r.width - 8) + 'px';
    menuEl.style.top = Math.min(y, window.innerHeight - r.height - 8) + 'px';
  }
  function onDocMouseDown(e) {
    if (menuEl && !menuEl.contains(e.target)) closeMenu();
  }

  // Shared so the Projects panel gets the same right-click menu instead of a
  // second implementation that looks almost but not quite the same.
  window.xnautContextMenu = (x, y, items) => openMenu(x, y, items);

  // ---------- usage parsing ----------
  function asPct(v) {
    const n = Number(v);
    return Number.isFinite(n) ? Math.round(n) : null;
  }
  function pick(o, keys) {
    for (const k of keys) if (o && o[k] != null) return o[k];
    return null;
  }
  function normalizeUsage(data) {
    let items = [];
    if (Array.isArray(data)) {
      items = data;
    } else if (data && typeof data === 'object') {
      if (Array.isArray(data.entries)) items = data.entries;
      else if (Array.isArray(data.plans)) items = data.plans;
      else {
        items = Object.keys(data)
          .filter((k) => data[k] && typeof data[k] === 'object' && !Array.isArray(data[k]))
          .map((k) => Object.assign({ label: k }, data[k]));
      }
    }
    const rows = [];
    for (const it of items) {
      if (!it || typeof it !== 'object') continue;
      const label = pick(it, ['label', 'name', 'provider', 'plan']) || '?';
      const p5 = asPct(pick(it, ['pct5h', 'pct_5h', 'five_hour_pct', 'fiveHourPct', 'session_pct', 'pctSession', 'percent_5h']));
      const pw = asPct(pick(it, ['pctWk', 'pct_wk', 'pct_week', 'week_pct', 'weekly_pct', 'weekPct', 'percent_week']));
      if (p5 === null && pw === null) continue;
      rows.push({ label: String(label), p5, pw });
      if (rows.length >= 2) break;
    }
    return rows;
  }

  // ---------- controller ----------
  let current = null; // last-mounted controller internals

  function mountSidebar(host) {
    if (!host) throw new Error('xnautMountSidebar: host element required');
    if (current) current.destroy(); // calling twice re-renders
    injectStyles();

    const state = { activeNav: 'control-center', destroyed: false, profiles: [], agentSessions: [] };
    host.innerHTML = '';

    const root = document.createElement('div');
    root.className = 'sbar-root';

    // Nav rows.
    const nav = document.createElement('div');
    nav.className = 'sbar-nav';
    const navEls = {};
    for (const item of NAV_ITEMS) {
      const row = document.createElement('div');
      row.className = 'sbar-nav-row';
      row.innerHTML = `${ICONS[item.icon || item.key]}<span>${escapeText(item.label)}</span>`;
      row.addEventListener('click', () => {
        state.activeNav = item.key;
        for (const k of Object.keys(navEls)) navEls[k].classList.toggle('sbar-active', k === state.activeNav);
        navigate(item.key);
      });
      navEls[item.key] = row;
      nav.appendChild(row);
    }
    navEls[state.activeNav].classList.add('sbar-active');
    root.appendChild(nav);

    async function syncVaultNavigation() {
      const vaultRow = navEls.vault;
      if (!vaultRow || state.destroyed) return;
      try {
        const settings = await invoke('settings_get');
        if (!settings?.project_management?.enabled) {
          vaultRow.hidden = false;
          return;
        }
        const projects = await invoke('pm_project_list');
        vaultRow.hidden = Array.isArray(projects) && projects.length > 0;
      } catch (_) {
        vaultRow.hidden = false;
      }
    }

    // Active-project highlight (Orca/CMUX): called by app.js setActiveProject.
    // id null → Home/global (restore nav highlight, clear project highlight).
    window.xnautSidebarSetActiveProject = (id) => {
      state.activeProjectId = id || null;
      list.querySelectorAll('.sbar-row').forEach((r) => {
        r.classList.toggle('sbar-row-active', !!id && r.dataset.taskId === id);
      });
      for (const k of Object.keys(navEls)) {
        navEls[k].classList.toggle('sbar-active', !id && k === state.activeNav);
      }
    };

    // Agents are the library. The single + remains here and owns all global
    // creation entries, with New Agent first as specified.
    const agentsHead = document.createElement('div');
    agentsHead.className = 'sbar-section-head sbar-collapsible';
    agentsHead.innerHTML = `<span class="sbar-head-label"><span class="sbar-caret">▾</span><span>Agents</span></span>`;
    const addBtn = document.createElement('button');
    addBtn.className = 'sbar-icon-btn';
    addBtn.title = 'Create';
    addBtn.setAttribute('aria-label', 'Create');
    addBtn.innerHTML = ICONS.plus;
    addBtn.addEventListener('click', (event) => {
      event.stopPropagation();
      const rect = addBtn.getBoundingClientRect();
      openMenu(rect.left, rect.bottom + 4, [
        { label:'New Agent', action:() => navigate('new-agent') },
        { label:'Open Agent Space', action:() => navigate('agents') },
        { label:'New Project', action:() => navigate('new-project') },
      ]);
    });
    agentsHead.appendChild(addBtn);
    root.appendChild(agentsHead);
    const agentsList = document.createElement('div');
    agentsList.className = 'sbar-agents';
    root.appendChild(agentsList);
    const AGENTS_COLLAPSE_KEY = 'xnaut-agents-collapsed';
    let agentsCollapsed = localStorage.getItem(AGENTS_COLLAPSE_KEY) === '1';
    const applyAgentsCollapsed = () => {
      agentsList.style.display = agentsCollapsed ? 'none' : '';
      agentsHead.classList.toggle('sbar-collapsed', agentsCollapsed);
    };
    applyAgentsCollapsed();
    agentsHead.addEventListener('click', () => {
      agentsCollapsed = !agentsCollapsed;
      localStorage.setItem(AGENTS_COLLAPSE_KEY, agentsCollapsed ? '1' : '0');
      applyAgentsCollapsed();
    });

    // Projects header (collapsible).
    const head = document.createElement('div');
    head.className = 'sbar-section-head sbar-collapsible';
    head.innerHTML = `<span class="sbar-head-label"><span class="sbar-caret">▾</span><span>Projects</span></span>`;
    root.appendChild(head);

    // Scrolling project list.
    const list = document.createElement('div');
    list.className = 'sbar-projects';
    root.appendChild(list);

    // Collapse the Projects list (persisted, toggled by clicking the header).
    const PROJECTS_COLLAPSE_KEY = 'xnaut-projects-collapsed';
    const applyProjectsCollapsed = (c) => {
      list.style.display = c ? 'none' : '';
      head.classList.toggle('sbar-collapsed', c);
    };
    let projectsCollapsed = localStorage.getItem(PROJECTS_COLLAPSE_KEY) === '1';
    applyProjectsCollapsed(projectsCollapsed);
    head.addEventListener('click', () => {
      projectsCollapsed = !projectsCollapsed;
      localStorage.setItem(PROJECTS_COLLAPSE_KEY, projectsCollapsed ? '1' : '0');
      applyProjectsCollapsed(projectsCollapsed);
    });

    // Usage strip.
    const usage = document.createElement('div');
    usage.className = 'sbar-usage';
    const usageRows = document.createElement('div');
    usageRows.className = 'sbar-usage-rows';
    const usageBtn = document.createElement('button');
    usageBtn.className = 'sbar-icon-btn';
    usageBtn.title = 'Refresh usage';
    usageBtn.setAttribute('aria-label', 'Refresh plan usage');
    usageBtn.innerHTML = ICONS.refresh;
    usageBtn.addEventListener('click', () => loadUsage());
    usage.appendChild(usageRows);
    usage.appendChild(usageBtn);
    root.appendChild(usage);

    host.appendChild(root);
    document.addEventListener('mousedown', onDocMouseDown);

    // Derived on its own so the 3-second status poll can update a row in place
    // instead of rebuilding it. It used to call renderProjects(), which wiped
    // and re-created every row three times a minute — that is what made the
    // project list flash, and it restarted the snake animation each time.
    function ago(ms) {
      if (!ms) return '';
      const mins = Math.max(0, Math.round((Date.now() - ms) / 60000));
      if (mins < 60) return mins <= 1 ? 'just now' : mins + 'm ago';
      const hrs = Math.round(mins / 60);
      if (hrs < 48) return hrs + 'h ago';
      return Math.round(hrs / 24) + 'd ago';
    }

    function agentInitials(profile) {
      return String(profile.display_name || '?').split(/\s+/).filter(Boolean).slice(0,2).map((part) => part[0].toUpperCase()).join('');
    }

    function latestAgentSession(handle) {
      return (state.agentSessions || []).filter((session) => session.agent_id === handle)
        .sort((left, right) => Number(right.last_output_at_ms || right.started_at_ms || 0) - Number(left.last_output_at_ms || left.started_at_ms || 0))[0] || null;
    }

    function agentMenu(event, profile) {
      event.preventDefault(); event.stopPropagation();
      openMenu(event.clientX, event.clientY, [
        { label:'Edit / Settings', action:() => navigate('agent-settings', profile) },
        { label:'Duplicate', action:async () => {
          const newHandle = prompt(`Duplicate @${profile.handle} as:`, `${profile.handle}-copy`);
          if (!newHandle) return;
          try { await invoke('agent_profile_duplicate', { handle:profile.handle, newHandle, displayName:`${profile.display_name} Copy` }); refreshAgents(); }
          catch (error) { console.error('[sidebar] duplicate agent failed:', error); }
        } },
        { label:'Assign project', action:async () => {
          const project = prompt('Default project path:', profile.default_project || '');
          if (project == null) return;
          try { await invoke('agent_profile_update', { handle:profile.handle, profile:{ ...profile, default_project:project.trim() || null } }); refreshAgents(); }
          catch (error) { console.error('[sidebar] assign project failed:', error); }
        } },
        ...(profile.handle === 'nautbot' ? [] : [{ label:'Delete…', danger:true, action:async () => {
          if (!confirm(`Delete ${profile.display_name} (@${profile.handle})?`)) return;
          try { await invoke('agent_profile_delete', { handle:profile.handle, rel:null }); refreshAgents(); }
          catch (error) { console.error('[sidebar] delete agent failed:', error); }
        } }]),
      ]);
    }

    function renderAgents() {
      agentsList.innerHTML = '';
      if (!state.profiles.length) {
        agentsList.innerHTML = '<div class="sbar-empty">No agents yet</div>';
        return;
      }
      state.profiles.forEach((profile) => {
        const session = latestAgentSession(profile.handle);
        const status = session && session.status || 'idle';
        const row = document.createElement('div');
        row.className = 'sbar-agent'; row.dataset.agentHandle = profile.handle;
        row.innerHTML = `<span class="sbar-agent-avatar" style="--agent-accent:${escapeText(profile.accent_color || '#666')}">${escapeText(agentInitials(profile))}</span><span class="sbar-agent-copy"><span class="sbar-agent-name">${escapeText(profile.display_name)}</span><span class="sbar-agent-meta"><span class="sbar-agent-status ${escapeText(status)}"></span><span>@${escapeText(profile.handle)}</span><span>· ${escapeText(status === 'idle' ? 'Ready' : status)}</span></span></span><button class="sbar-agent-more" aria-label="Agent actions" title="Agent actions">•••</button>`;
        row.onclick = () => navigate('agent-thread', { handle:profile.handle });
        row.oncontextmenu = (event) => agentMenu(event, profile);
        row.querySelector('.sbar-agent-more').onclick = (event) => agentMenu(event, profile);
        agentsList.appendChild(row);
        const threads = window.xnautAgentThreadsFor ? window.xnautAgentThreadsFor(profile.handle) : [];
        const threadHost = document.createElement('div'); threadHost.className = 'sbar-threads';
        threads.slice(0,3).forEach((thread) => {
          const item = document.createElement('div'); item.className = 'sbar-thread'; item.textContent = thread.title || 'Untitled thread';
          item.onclick = () => navigate('agent-thread', { handle:profile.handle, threadId:thread.id }); threadHost.appendChild(item);
        });
        const create = document.createElement('div'); create.className = 'sbar-thread sbar-thread-new'; create.textContent = '+ New thread';
        create.onclick = () => navigate('agent-thread', { handle:profile.handle, newThread:true }); threadHost.appendChild(create);
        agentsList.appendChild(threadHost);
      });
    }

    async function refreshAgents() {
      try {
        const [profiles, sessions] = await Promise.all([invoke('agent_profile_list'), invoke('agent_sessions_list').catch(() => [])]);
        if (state.destroyed) return;
        state.profiles = Array.isArray(profiles) ? profiles : [];
        state.agentSessions = Array.isArray(sessions) ? sessions : [];
        renderAgents();
      } catch (error) {
        console.error('[sidebar] agent_profile_list failed:', error); state.profiles = []; renderAgents();
      }
    }

    function dotStateFor(task) {
      // Dot lights when the project has open tabs in this session.
      const sessions = sessionsFor(task);
      const running = sessions.filter((s) => !s.exited);
      const exited = sessions.filter((s) => s.exited);
      // The dot used to mean "has tabs open in this window", which you already
      // know. A running session is the thing worth seeing at a glance.
      const live = running.length > 0
        || !!(window.xnautProjectHasTabs && window.xnautProjectHasTabs(task.id));
      // Motion has to mean something. The snake runs only when an agent hook
      // actually reports Working; a session that merely exists gets a steady
      // dot. Zellij cannot tell us the difference — its resurrection cache is
      // rewritten about once a second whether the agent is thinking or idle —
      // so absent a real status we say "alive", not "busy".
      const agentState = window.xnautProjectAgentStatus
        ? window.xnautProjectAgentStatus(task.id)
        : null;
      let dotClass = '';
      let rowState = '';
      if (agentState === 'permission' || agentState === 'blocked') { dotClass = ' sbar-attention'; rowState = 'attention'; }
      else if (agentState === 'working') { dotClass = ' sbar-run'; rowState = 'working'; }
      else if (agentState === 'waiting') { dotClass = ' sbar-live'; rowState = 'waiting'; }
      else if (agentState === 'done') { dotClass = ' sbar-live'; rowState = 'done'; }
      else if (running.length) { dotClass = ' sbar-live'; rowState = 'live'; }
      else if (exited.length) { dotClass = ' sbar-exited'; rowState = 'exited'; }
      else if (live) dotClass = ' sbar-on';
      const title = running.length ? 'session running' : (exited.length ? 'session can be resurrected' : '');
      return { dotClass, rowState, title, sessions };
    }

    function buildRow(task) {
      const row = document.createElement('div');
      row.className = 'sbar-row';
      row.dataset.taskId = task.id;
      const { dotClass, rowState, title, sessions } = dotStateFor(task);
      if (rowState) row.dataset.state = rowState;
      const badge = task.kind === 'task' ? 'task' : (task.project_type || '');
      const agents = sessions
        .map((s) => (/^([a-z]{2,4})-/.exec(String(s.name || '')) || [])[1])
        .filter(Boolean);
      row.innerHTML = `
        <span class="sbar-dot${dotClass}" title="${title}"></span>
        <div class="sbar-row-main">
          <div class="sbar-row-top">
            <span class="sbar-name" title="${escapeText(task.path || '')}">${escapeText(task.name || task.id)}</span>
            ${agents.map((a) => `<span class="sbar-chip sbar-sess" title="${escapeText(a)} session — click to attach">${escapeText(a)}</span>`).join('')}
            ${badge ? `<span class="sbar-chip">${escapeText(badge)}</span>` : ''}
          </div>
          <div class="sbar-branch" hidden><span class="sbar-branch-name"></span><span class="sbar-ago"></span></div>
        </div>
      `;
      row.addEventListener('click', (e) => {
        // Open the running session, not a new shell in the same directory —
        // that was only ever useful before the Observatory existed.
        const sessions = sessionsFor(task);
        if (!sessions.length) return navigate('open-task', task);
        if (sessions.length === 1) {
          return navigate('open-task', { ...task, zellij_session: sessions[0].name });
        }
        // A project can have one session per agent (cl-Bucky and cx-Bucky) —
        // ask rather than guess which one is meant.
        openMenu(e.clientX, e.clientY, sessions.map((s) => ({
          label: s.name,
          action: () => navigate('open-task', { ...task, zellij_session: s.name }),
        })).concat([{
          label: 'New terminal here',
          action: () => navigate('open-task', task),
        }]));
      });
      row.addEventListener('contextmenu', (e) => {
        e.preventDefault();
        e.stopPropagation();
        const pins = loadPins();
        const pinned = pins.includes(task.id);
        const items = [{
          label: pinned ? 'Unpin' : 'Pin',
          action: () => {
            savePins(pinned ? pins.filter((p) => p !== task.id) : pins.concat([task.id]));
            renderProjects(state.tasks || []);
          },
        }];
        items.push({
          label: window.xnautHiddenProjects.isHidden('sidebar', task.id) ? 'Unhide' : 'Hide',
          action: () => {
            window.xnautHiddenProjects.toggle('sidebar', task.id);
            renderProjects(state.tasks || []);
          },
        });
        if (task.kind === 'task') {
          items.push({ label: 'Promote to Project', action: () => navigate('promote-task', task) });
        }
        items.push({
          label: 'Remove from list',
          danger: true,
          action: () => {
            // task_remove only drops the registry entry (folder stays on disk;
            // re-add via "Open as project"). Native confirm() is a no-op in this
            // WebKit, so we just proceed.
            invoke('task_remove', { id: task.id })
              .then(() => refresh())
              .catch((err) => console.error('[sidebar] task_remove failed:', err));
          },
        });
        openMenu(e.clientX, e.clientY, items);
      });
      // Branch — best effort, fills in async.
      if (task.path) {
        invoke('get_git_info', { path: task.path }).then((info) => {
          const branch = info && (info.branch || info.current_branch || null);
          if (!branch || !row.isConnected) return;
          const el = row.querySelector('.sbar-branch');
          el.querySelector('.sbar-branch-name').textContent = String(branch);
          el.hidden = false;
        }).catch(() => { /* not a git repo / command failed — show nothing */ });
      }
      return row;
    }

    function renderProjects(tasks) {
      state.tasks = tasks;
      list.innerHTML = '';
      const pins = loadPins();
      // Hidden projects stay in the registry and on disk — they are only kept
      // out of the list, so a demo does not show client work. state.showHidden
      // reveals them temporarily so they can be unhidden again.
      const hiddenIds = window.xnautHiddenProjects.list('sidebar');
      const visible = state.showHidden
        ? tasks
        : tasks.filter((t) => !hiddenIds.includes(String(t.id)));
      const hiddenCount = tasks.length - visible.length;
      const pinned = visible.filter((t) => pins.includes(t.id));
      const rest = visible.filter((t) => !pins.includes(t.id));
      if (!tasks.length) {
        const empty = document.createElement('div');
        empty.className = 'sbar-empty';
        empty.textContent = 'No projects yet';
        list.appendChild(empty);
        return;
      }
      if (pinned.length) {
        const lbl = document.createElement('div');
        lbl.className = 'sbar-sub-label';
        lbl.textContent = 'Pinned';
        list.appendChild(lbl);
        for (const t of pinned) list.appendChild(buildRow(t));
      }
      for (const t of rest) list.appendChild(buildRow(t));
      // The only way back: without this, hiding is a one-way door.
      if (hiddenCount > 0 || state.showHidden) {
        const toggle = document.createElement('div');
        toggle.className = 'sbar-hidden-toggle';
        toggle.textContent = state.showHidden
          ? 'Hide hidden again'
          : `${hiddenCount} hidden — show`;
        toggle.title = 'Right-click a revealed project and choose Hide to unhide it';
        toggle.addEventListener('click', () => {
          state.showHidden = !state.showHidden;
          renderProjects(state.tasks || []);
        });
        list.appendChild(toggle);
      }
      // Re-apply the active-project highlight after rebuilding rows.
      if (window.xnautSidebarSetActiveProject) window.xnautSidebarSetActiveProject(state.activeProjectId || null);
      // Async: one git call per project, so it must not hold up the render.
      fillActivity(visible);
    }

    // Last activity, for every row in ONE call. The obvious source — zellij's
    // last_active_ms — is useless here: it comes from the mtime of the
    // resurrection cache, which zellij rewrites about once a second whether the
    // agent is thinking or asleep, so every project would read "just now".
    // The last COMMIT is a thing that actually happened.
    async function fillActivity(tasks) {
      const rows = tasks.filter((t) => t.path);
      if (!rows.length) return;
      let times = [];
      try { times = await invoke('projects_activity', { paths: rows.map((t) => t.path) }) || []; }
      catch (_) { return; }
      rows.forEach((task, i) => {
        const row = list.querySelector(`.sbar-row[data-task-id="${String(task.id).replace(/"/g, '\\"')}"]`);
        if (!row) return;
        const text = ago(times[i]);
        if (!text) return;
        const line = row.querySelector('.sbar-branch');
        const el = row.querySelector('.sbar-ago');
        if (!line || !el) return;
        el.textContent = text;
        el.title = 'last commit';
        line.hidden = false; // may be the only thing on the line, if there is no branch
      });
    }

    // app.js polls agent_sessions_list and calls this when a status changes.
    // Without it the dots only updated on a full refresh, so an agent could go
    // from working to needing you and the rail would not move.
    // In place, and only where something actually changed: re-assigning an
    // unchanged className restarts the CSS animation, so the snake would stutter
    // back to its first step on every poll.
    window.xnautSidebarRefreshDots = () => {
      if (!state.tasks) return;
      for (const task of state.tasks) {
        const row = list.querySelector(`.sbar-row[data-task-id="${String(task.id).replace(/"/g, '\\"')}"]`);
        if (!row) continue;
        const { dotClass, rowState, title } = dotStateFor(task);
        const dot = row.querySelector('.sbar-dot');
        if (dot) {
          const next = 'sbar-dot' + dotClass;
          if (dot.className !== next) dot.className = next;
          if (dot.title !== title) dot.title = title;
        }
        if ((row.dataset.state || '') !== rowState) {
          if (rowState) row.dataset.state = rowState;
          else delete row.dataset.state;
        }
      }
    };

    async function refresh() {
      if (state.destroyed) return;
      syncVaultNavigation();
      refreshAgents();
      // Live Zellij sessions, so a project row can open the session that is
      // already running instead of a fresh shell in the same folder.
      try {
        const zs = await invoke('zellij_sessions_info');
        state.sessions = Array.isArray(zs) ? zs : [];
      } catch (_) {
        state.sessions = [];
      }
      try {
        const tasks = await invoke('tasks_list');
        if (state.destroyed) return;
        renderProjects(Array.isArray(tasks) ? tasks : []);
      } catch (e) {
        console.error('[sidebar] tasks_list failed:', e);
        renderProjects([]);
      }
    }

    // Sessions are named <agent>-<project> by the shell wrappers: cl-Bucky and
    // cx-Bucky both belong to Bucky. Zellij truncates long names (cl-nautflow-
    // incident-loo), so the project side is matched as a prefix.
    function sessionsFor(task) {
      const name = String(task.name || task.id || '');
      if (!name) return [];
      return (state.sessions || []).filter((s) => {
        const m = /^([a-z]{2,4})-(.+)$/.exec(String(s.name || ''));
        if (!m) return false;
        const proj = m[2];
        return name === proj || name.startsWith(proj) || proj.startsWith(name);
      });
    }

    async function loadUsage() {
      const fail = () => {
        usageRows.innerHTML = '<div class="sbar-usage-row sbar-muted">usage: n/a</div>';
      };
      try {
        const home = await invoke('get_home_directory');
        if (state.destroyed || typeof home !== 'string' || !home) return fail();
        const raw = await invoke('read_file', { path: home.replace(/\/+$/, '') + '/.flowai/usage.json' });
        if (state.destroyed) return;
        const data = typeof raw === 'string' ? JSON.parse(raw) : raw;
        const rows = normalizeUsage(data);
        if (!rows.length) return fail();
        usageRows.innerHTML = rows.map((r) => {
          const parts = [];
          if (r.p5 !== null) parts.push(`${r.p5}% 5h`);
          if (r.pw !== null) parts.push(`${r.pw}% wk`);
          return `<div class="sbar-usage-row">${escapeText(r.label)} ${parts.join(' · ')}</div>`;
        }).join('');
      } catch (e) {
        if (!state.destroyed) fail();
      }
    }

    function destroy() {
      if (state.destroyed) return;
      state.destroyed = true;
      closeMenu();
      document.removeEventListener('mousedown', onDocMouseDown);
      window.removeEventListener('xnaut:agent-profiles-changed', refreshAgents);
      window.removeEventListener('xnaut:agent-threads-changed', renderAgents);
      if (root.parentNode) root.parentNode.removeChild(root);
      if (current && current.destroy === destroy) current = null;
    }

    refresh();
    loadUsage();
    window.addEventListener('xnaut:agent-profiles-changed', refreshAgents);
    window.addEventListener('xnaut:agent-threads-changed', renderAgents);

    const controller = { refresh, destroy };
    current = controller;
    return controller;
  }

  // Public API.
  window.xnautMountSidebar = mountSidebar;
  window.xnautSidebarRefresh = function () {
    if (current) current.refresh();
  };
})();
