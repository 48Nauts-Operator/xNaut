// Right pane — v1.6 project-scoped multi-view sidebar (host shell).
//
// Architecture: a single mounted host with a top icon bar (Files / Search /
// Git / Tasks) and a view container below. Views register themselves via
// window.xnautRightPaneRegisterView(key, view) where view is
// { mount(container, root), setRoot(root), destroy() }. Scripts may load in
// any order: late registrations are mounted on demand, and views that load
// *before* this file queue themselves on window.__xnautRightPaneQueue.
(function () {
  'use strict';

  const invoke = (...a) => window.__TAURI__.core.invoke(...a);

  function escapeText(s) {
    return String(s).replace(/[&<>"']/g, (c) => ({ '&':'&amp;', '<':'&lt;', '>':'&gt;', '"':'&quot;', "'":'&#39;' }[c]));
  }

  function basename(p) {
    if (!p) return '';
    const trimmed = String(p).replace(/\/+$/, '');
    const i = trimmed.lastIndexOf('/');
    return i === -1 ? trimmed : trimmed.slice(i + 1);
  }

  const ICONS = {
    files: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" width="16" height="16"><path d="M4 1.5h5l3 3V14a.5.5 0 0 1-.5.5h-7.5A.5.5 0 0 1 3.5 14V2a.5.5 0 0 1 .5-.5z"/><path d="M9 1.5v3h3"/></svg>',
    search: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" width="16" height="16"><circle cx="7" cy="7" r="4.5"/><line x1="10.5" y1="10.5" x2="14" y2="14"/></svg>',
    git: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" width="16" height="16"><circle cx="4.5" cy="3.5" r="1.8"/><circle cx="4.5" cy="12.5" r="1.8"/><circle cx="11.5" cy="6" r="1.8"/><path d="M4.5 5.3v5.4"/><path d="M11.5 7.8c0 2.5-3 2.5-5 3.2"/></svg>',
    tasks: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" width="16" height="16"><path d="M2.5 4l1.2 1.2L6 2.9"/><path d="M2.5 9.5l1.2 1.2L6 8.4"/><line x1="8" y1="4.2" x2="14" y2="4.2"/><line x1="8" y1="9.7" x2="14" y2="9.7"/><line x1="2.5" y1="13.5" x2="14" y2="13.5"/></svg>',
  };
  const VIEW_ORDER = [
    { key: 'files', title: 'Files' },
    { key: 'search', title: 'Search' },
    { key: 'git', title: 'Git' },
    { key: 'tasks', title: 'Tasks' },
  ];

  const STYLES = `
.rpane-host { display:flex; flex-direction:column; height:100%; min-height:0; min-width:0; background:var(--bg-secondary); border-left:1px solid var(--border); font-family:var(--font-sans, sans-serif); }
.rpane-bar { display:flex; align-items:center; gap:2px; flex:0 0 36px; height:36px; padding:0 8px; border-bottom:1px solid var(--border); }
.rpane-tab { display:flex; align-items:center; justify-content:center; width:28px; height:28px; border:none; border-radius:var(--radius-md, 6px); background:transparent; color:var(--text-secondary); cursor:pointer; padding:0; }
.rpane-tab:hover { background:var(--bg-tertiary); color:var(--text-primary); }
.rpane-tab.rpane-active { background:var(--bg-tertiary); color:var(--accent); }
.rpane-title { margin-left:auto; font-size:11px; color:var(--text-secondary); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; max-width:45%; }
.rpane-content { flex:1 1 0%; min-height:0; position:relative; display:flex; flex-direction:column; }
.rpane-view { flex:1 1 0%; min-height:0; overflow-y:auto; display:none; flex-direction:column; }
.rpane-view.rpane-view-active { display:flex; }
.rpane-empty { padding:16px 12px; font-size:12px; color:var(--text-secondary); text-align:center; }
.rpane-task-row { display:flex; align-items:center; gap:6px; padding:6px 10px; font-size:12px; color:var(--text-primary); border-bottom:1px solid var(--border); min-width:0; }
.rpane-task-name { flex:1 1 auto; min-width:0; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
.rpane-task-kind { flex:0 0 auto; font-size:10px; padding:1px 6px; border-radius:999px; background:var(--bg-tertiary); color:var(--text-secondary); border:1px solid var(--border); }
.rpane-task-session { flex:0 0 auto; font-size:10px; font-family:var(--font-mono, monospace); color:var(--text-secondary); }
.rpane-rootmenu { position:absolute; z-index:50; min-width:200px; background:var(--editor-surface, #1b1d23); border:1px solid var(--border, rgba(255,255,255,.14)); border-radius:8px; box-shadow:0 10px 30px rgba(0,0,0,.45); padding:4px; }
.rpane-rootmenu-item { display:flex; flex-direction:column; gap:1px; padding:6px 10px; border-radius:6px; cursor:pointer; }
.rpane-rootmenu-item:hover { background:var(--hover-bg, rgba(255,255,255,.07)); }
.rpane-rootmenu-label { font-size:12px; color:var(--text-primary, #e8eaf0); }
.rpane-rootmenu-path { font-size:10px; color:var(--text-secondary, #8a8f98); font-family:var(--font-mono, monospace); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
`;

  function ensureStyles() {
    if (document.getElementById('right-pane-styles')) return;
    const el = document.createElement('style');
    el.id = 'right-pane-styles';
    el.textContent = STYLES;
    document.head.appendChild(el);
  }

  // ---- View registry --------------------------------------------------
  const registry = new Map(); // key -> view object
  let mountedState = null;    // set while a host is mounted

  function registerView(key, view) {
    if (!key || !view || typeof view.mount !== 'function') {
      console.warn('[right-pane] invalid view registration:', key);
      return;
    }
    registry.set(key, view);
    // Late registration: if the host is up and this view's tab is active,
    // mount it immediately.
    if (mountedState && mountedState.activeKey === key) {
      mountActiveView();
    }
  }
  window.xnautRightPaneRegisterView = registerView;

  // Drain registrations queued by views that loaded before this file.
  (function drainQueue() {
    const q = window.__xnautRightPaneQueue;
    if (Array.isArray(q)) {
      for (const item of q) {
        if (item && item.key && item.view) registerView(item.key, item.view);
      }
    }
    // Replace the queue with a live shim so anything pushing later still works.
    window.__xnautRightPaneQueue = { push: (item) => { if (item && item.key && item.view) registerView(item.key, item.view); } };
  })();

  // ---- Built-in tasks view --------------------------------------------
  function createTasksView() {
    let container = null;
    let root = null;

    async function render() {
      if (!container) return;
      container.innerHTML = '<div class="rpane-empty">Loading tasks…</div>';
      let tasks;
      try {
        tasks = await invoke('tasks_list');
      } catch (e) {
        container.innerHTML = `<div class="rpane-empty">Failed to load tasks: ${escapeText(String(e))}</div>`;
        return;
      }
      if (!Array.isArray(tasks)) tasks = [];
      const r = root ? String(root).replace(/\/+$/, '') : '';
      const scoped = tasks.filter((t) => {
        const p = t && t.path ? String(t.path) : '';
        if (!r || !p) return false;
        return p === r || p.startsWith(r + '/');
      });
      if (!scoped.length) {
        container.innerHTML = '<div class="rpane-empty">No tasks for this project</div>';
        return;
      }
      container.innerHTML = scoped.map((t) => `
        <div class="rpane-task-row" title="${escapeText(t.path || '')}">
          <span class="rpane-task-name">${escapeText(t.name || t.id || 'task')}</span>
          <span class="rpane-task-kind">${escapeText(t.kind || '?')}</span>
          ${t.zellij_session ? `<span class="rpane-task-session">${escapeText(t.zellij_session)}</span>` : ''}
        </div>`).join('');
    }

    return {
      mount(el, initialRoot) { container = el; root = initialRoot; render(); },
      setRoot(newRoot) { root = newRoot; render(); },
      destroy() { container = null; },
    };
  }
  registerView('tasks', createTasksView());

  // ---- Host mount ------------------------------------------------------
  function mountActiveView() {
    const s = mountedState;
    if (!s) return;
    const key = s.activeKey;
    const view = registry.get(key);
    const slot = s.viewSlots.get(key);
    if (!view || !slot) return;
    if (!slot.mounted) {
      slot.mounted = true;
      slot.root = s.root;
      try { view.mount(slot.el, s.root); } catch (e) { console.error(`[right-pane] mount of "${key}" failed`, e); }
    } else if (slot.root !== s.root) {
      slot.root = s.root;
      if (typeof view.setRoot === 'function') {
        try { view.setRoot(s.root); } catch (e) { console.error(`[right-pane] setRoot of "${key}" failed`, e); }
      }
    }
  }

  function mountRightPane(hostElement) {
    if (!hostElement) throw new Error('xnautMountRightPane: hostElement required');
    ensureStyles();
    if (mountedState) destroyHost(); // single instance

    hostElement.classList.add('rpane-host');
    hostElement.innerHTML = `
      <div class="rpane-bar">
        ${VIEW_ORDER.map((v) => `<button class="rpane-tab" data-rpane-view="${v.key}" title="${v.title}" aria-label="${v.title}">${ICONS[v.key]}</button>`).join('')}
        <span class="rpane-title" title=""></span>
      </div>
      <div class="rpane-content"></div>
    `;
    const content = hostElement.querySelector('.rpane-content');
    const titleEl = hostElement.querySelector('.rpane-title');

    const viewSlots = new Map();
    for (const v of VIEW_ORDER) {
      const el = document.createElement('div');
      el.className = 'rpane-view';
      el.dataset.rpaneSlot = v.key;
      content.appendChild(el);
      viewSlots.set(v.key, { el, mounted: false, root: null });
    }

    mountedState = { host: hostElement, root: null, activeKey: 'files', viewSlots, titleEl };

    function setActive(key) {
      mountedState.activeKey = key;
      hostElement.querySelectorAll('.rpane-tab').forEach((b) => b.classList.toggle('rpane-active', b.dataset.rpaneView === key));
      viewSlots.forEach((slot, k) => slot.el.classList.toggle('rpane-view-active', k === key));
      const slot = viewSlots.get(key);
      if (slot && !slot.el.children.length && !registry.has(key)) {
        slot.el.innerHTML = '<div class="rpane-empty">View not loaded</div>';
      }
      mountActiveView();
    }

    hostElement.querySelectorAll('.rpane-tab').forEach((b) => {
      b.onclick = () => {
        setActive(b.dataset.rpaneView);
        // The Files icon doubles as a root picker: Home / Project Root / current project.
        if (b.dataset.rpaneView === 'files') toggleRootMenu(b);
      };
    });
    setActive('files');

    let rootMenuEl = null;
    function closeRootMenu() {
      if (rootMenuEl) { rootMenuEl.remove(); rootMenuEl = null; document.removeEventListener('mousedown', onDocDown, true); }
    }
    function onDocDown(e) { if (rootMenuEl && !rootMenuEl.contains(e.target)) closeRootMenu(); }

    async function toggleRootMenu(anchor) {
      if (rootMenuEl) { closeRootMenu(); return; }
      // Resolve roots: Home, Project Root (from settings), and the active project.
      let home = '~';
      let projectRoot = '';
      try { home = await invoke('get_home_directory', {}); } catch (_) {}
      try { const s = await invoke('settings_get'); projectRoot = (s && s.project_root) || ''; } catch (_) {}
      const items = [{ label: 'Home', path: home }];
      if (projectRoot) items.push({ label: 'Project Root', path: projectRoot });
      if (mountedState.root && mountedState.root !== home && mountedState.root !== projectRoot) {
        items.push({ label: 'Current Project', path: mountedState.root });
      }

      const menu = document.createElement('div');
      menu.className = 'rpane-rootmenu';
      menu.innerHTML = items.map((it, i) =>
        `<div class="rpane-rootmenu-item" data-i="${i}"><span class="rpane-rootmenu-label">${escapeText(it.label)}</span><span class="rpane-rootmenu-path">${escapeText(it.path)}</span></div>`
      ).join('');
      const barRect = hostElement.querySelector('.rpane-bar').getBoundingClientRect();
      const aRect = anchor.getBoundingClientRect();
      const hostRect = hostElement.getBoundingClientRect();
      menu.style.left = (aRect.left - hostRect.left) + 'px';
      menu.style.top = (barRect.bottom - hostRect.top + 2) + 'px';
      menu.querySelectorAll('.rpane-rootmenu-item').forEach((row) => {
        row.onclick = () => { const it = items[+row.dataset.i]; closeRootMenu(); setRoot(it.path); };
      });
      hostElement.appendChild(menu);
      rootMenuEl = menu;
      setTimeout(() => document.addEventListener('mousedown', onDocDown, true), 0);
    }

    function setRoot(path) {
      mountedState.root = path || null;
      const name = basename(path);
      titleEl.textContent = name;
      titleEl.title = path || '';
      mountActiveView();
    }

    function destroyHost() {
      if (!mountedState) return;
      mountedState.viewSlots.forEach((slot, key) => {
        if (!slot.mounted) return;
        const view = registry.get(key);
        if (view && typeof view.destroy === 'function') {
          try { view.destroy(); } catch (e) { console.error(`[right-pane] destroy of "${key}" failed`, e); }
        }
      });
      mountedState.host.innerHTML = '';
      mountedState.host.classList.remove('rpane-host');
      mountedState = null;
    }

    return {
      setRoot,
      getRoot: () => (mountedState ? mountedState.root : null),
      destroy: destroyHost,
    };
  }

  let lastController = null;
  window.xnautMountRightPane = (hostElement) => {
    lastController = mountRightPane(hostElement);
    return lastController;
  };
  window.xnautRightPaneSetRoot = (path) => {
    if (mountedState && lastController) lastController.setRoot(path);
    // no-op if unmounted
  };
})();
