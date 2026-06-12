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
    tasks: `<svg ${SVG_ATTRS}><path d="M3 4.5l1.5 1.5L7 3.5"/><line x1="9" y1="4.5" x2="13" y2="4.5"/><path d="M3 10.5l1.5 1.5L7 9.5"/><line x1="9" y1="10.5" x2="13" y2="10.5"/></svg>`,
    automations: `<svg ${SVG_ATTRS}><path d="M8.5 2L4 9h3.5L7 14l5-7H8.5l.5-5z"/></svg>`,
    search: `<svg ${SVG_ATTRS}><circle cx="7" cy="7" r="4"/><line x1="10" y1="10" x2="13.5" y2="13.5"/></svg>`,
    plus: `<svg ${SVG_ATTRS}><line x1="8" y1="3" x2="8" y2="13"/><line x1="3" y1="8" x2="13" y2="8"/></svg>`,
    refresh: `<svg ${SVG_ATTRS}><path d="M13 8a5 5 0 1 1-1.5-3.5"/><path d="M13 2v3h-3"/></svg>`,
  };

  const NAV_ITEMS = [
    { key: 'tasks', label: 'Tasks' },
    { key: 'automations', label: 'Automations' },
    { key: 'pm', label: 'PM' },
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
      .sbar-icon-btn { display: flex; align-items: center; justify-content: center; width: 22px; height: 22px;
        border: none; border-radius: 5px; background: transparent; color: var(--text-secondary, #aaa); cursor: pointer; padding: 0; }
      .sbar-icon-btn:hover { background: var(--hover-bg, rgba(255,255,255,0.08)); color: var(--text-primary, #fff); }
      .sbar-projects { flex: 1 1 0%; min-height: 0; overflow-y: auto; padding: 2px 6px 8px; }
      .sbar-sub-label { padding: 6px 8px 2px; font-size: 10px; letter-spacing: 0.05em; text-transform: uppercase;
        color: var(--text-muted, #666); }
      .sbar-row { display: flex; align-items: flex-start; gap: 8px; padding: 6px 8px; border-radius: 6px; cursor: pointer; }
      .sbar-row:hover { background: var(--hover-bg, rgba(255,255,255,0.06)); }
      .sbar-dot { flex: 0 0 auto; width: 7px; height: 7px; margin-top: 5px; border-radius: 50%;
        background: var(--dot-off, #555); }
      .sbar-dot.sbar-on { background: var(--dot-on, #3fb950); }
      .sbar-row-main { flex: 1 1 auto; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
      .sbar-row-top { display: flex; align-items: center; gap: 6px; min-width: 0; }
      .sbar-name { flex: 1 1 auto; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
      .sbar-chip { flex: 0 0 auto; font-size: 10px; padding: 1px 6px; border-radius: 8px;
        background: var(--chip-bg, rgba(255,255,255,0.08)); color: var(--text-secondary, #999); }
      .sbar-branch { font-size: 11px; color: var(--text-muted, #777); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
      .sbar-empty { padding: 10px 8px; color: var(--text-muted, #666); font-size: 12px; }
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

    const state = { activeNav: 'tasks', destroyed: false };
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
      row.innerHTML = `${ICONS[item.key]}<span>${escapeText(item.label)}</span>`;
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

    // Projects header.
    const head = document.createElement('div');
    head.className = 'sbar-section-head';
    head.innerHTML = `<span>Projects</span>`;
    const addBtn = document.createElement('button');
    addBtn.className = 'sbar-icon-btn';
    addBtn.title = 'Add project';
    addBtn.setAttribute('aria-label', 'Add project');
    addBtn.innerHTML = ICONS.plus;
    addBtn.addEventListener('click', () => navigate('new-project'));
    head.appendChild(addBtn);
    root.appendChild(head);

    // Scrolling project list.
    const list = document.createElement('div');
    list.className = 'sbar-projects';
    root.appendChild(list);

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

    function buildRow(task) {
      const row = document.createElement('div');
      row.className = 'sbar-row';
      row.dataset.taskId = task.id;
      const live = typeof task.zellij_session === 'string' && task.zellij_session.length > 0;
      const badge = task.kind === 'task' ? 'task' : (task.project_type || '');
      row.innerHTML = `
        <span class="sbar-dot${live ? ' sbar-on' : ''}"></span>
        <div class="sbar-row-main">
          <div class="sbar-row-top">
            <span class="sbar-name" title="${escapeText(task.path || '')}">${escapeText(task.name || task.id)}</span>
            ${badge ? `<span class="sbar-chip">${escapeText(badge)}</span>` : ''}
          </div>
          <div class="sbar-branch" hidden></div>
        </div>
      `;
      row.addEventListener('click', () => navigate('open-task', task));
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
        if (task.kind === 'task') {
          items.push({ label: 'Promote to Project', action: () => navigate('promote-task', task) });
        }
        items.push({
          label: 'Remove from list',
          danger: true,
          action: () => {
            if (!confirm(`Remove "${task.name || task.id}" from the list?`)) return;
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
          el.textContent = String(branch);
          el.hidden = false;
        }).catch(() => { /* not a git repo / command failed — show nothing */ });
      }
      return row;
    }

    function renderProjects(tasks) {
      state.tasks = tasks;
      list.innerHTML = '';
      const pins = loadPins();
      const pinned = tasks.filter((t) => pins.includes(t.id));
      const rest = tasks.filter((t) => !pins.includes(t.id));
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
    }

    async function refresh() {
      if (state.destroyed) return;
      try {
        const tasks = await invoke('tasks_list');
        if (state.destroyed) return;
        renderProjects(Array.isArray(tasks) ? tasks : []);
      } catch (e) {
        console.error('[sidebar] tasks_list failed:', e);
        renderProjects([]);
      }
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
      if (root.parentNode) root.parentNode.removeChild(root);
      if (current && current.destroy === destroy) current = null;
    }

    refresh();
    loadUsage();

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
