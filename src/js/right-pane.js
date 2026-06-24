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
.rpane-todo-head { display:flex; align-items:center; gap:8px; padding:8px 10px 4px; }
.rpane-todo-project { flex:1 1 auto; min-width:0; font-size:12px; font-weight:600; color:var(--text-primary, #e8eaf0); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
.rpane-plan-btn { flex:0 0 auto; background:var(--agent-thinking, #4dffd0); color:#07120f; border:none; border-radius:6px; font:inherit; font-size:11px; font-weight:600; padding:3px 9px; cursor:pointer; }
.rpane-plan-btn:hover { filter:brightness(1.08); }
.rpane-todo-add { display:flex; gap:6px; padding:4px 10px 8px; border-bottom:1px solid var(--border); }
.rpane-todo-input { flex:1 1 auto; min-width:0; background:var(--input-bg, rgba(255,255,255,.05)); border:1px solid var(--border, rgba(255,255,255,.14)); border-radius:6px; color:inherit; padding:5px 8px; font:inherit; font-size:12px; outline:none; }
.rpane-todo-input:focus { border-color:var(--accent, #4f8cff); }
.rpane-todo-addbtn { flex:0 0 auto; width:28px; background:var(--accent, #4f8cff); color:#fff; border:none; border-radius:6px; font-size:16px; line-height:1; cursor:pointer; }
.rpane-todos { display:flex; flex-direction:column; }
.rpane-todo-row { display:flex; align-items:center; gap:8px; padding:5px 10px; border-bottom:1px solid var(--border, rgba(255,255,255,.05)); }
.rpane-todo-row input { flex:0 0 auto; accent-color:var(--agent-thinking, #4dffd0); }
.rpane-todo-text { flex:1 1 auto; min-width:0; font-size:12px; color:var(--text-primary, #ddd); word-break:break-word; }
.rpane-todo-done .rpane-todo-text { text-decoration:line-through; color:var(--text-secondary, #888); }
.rpane-todo-del { flex:0 0 auto; background:transparent; border:none; color:var(--text-secondary, #888); cursor:pointer; font-size:15px; line-height:1; padding:0 4px; }
.rpane-todo-del:hover { color:#f85149; }
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

  // ---- Per-project task list (project_todos store, keyed by task id) -----
  function createTasksView() {
    let container = null;
    let root = null;
    let taskId = null;

    function paint(todos, taskName) {
      container.innerHTML = `
        <div class="rpane-todo-head">
          <span class="rpane-todo-project" title="${escapeText(root || '')}">${escapeText(taskName || 'Project tasks')}</span>
          <button class="rpane-plan-btn" title="Open Plan Mode for this project">Plan Mode</button>
        </div>
        <div class="rpane-todo-add">
          <input class="rpane-todo-input" type="text" placeholder="Add a task / reminder…" spellcheck="false">
          <button class="rpane-todo-addbtn" title="Add">+</button>
        </div>
        <div class="rpane-todos"></div>`;
      const listEl = container.querySelector('.rpane-todos');
      const input = container.querySelector('.rpane-todo-input');

      const repaintList = (items) => {
        if (!items || !items.length) { listEl.innerHTML = '<div class="rpane-empty">No tasks yet — add one above.</div>'; return; }
        listEl.innerHTML = '';
        items.forEach((t) => {
          const row = document.createElement('div');
          row.className = 'rpane-todo-row' + (t.done ? ' rpane-todo-done' : '');
          const cb = document.createElement('input'); cb.type = 'checkbox'; cb.checked = !!t.done;
          const span = document.createElement('span'); span.className = 'rpane-todo-text'; span.textContent = t.text;
          const del = document.createElement('button'); del.className = 'rpane-todo-del'; del.title = 'Delete'; del.textContent = '×';
          cb.onchange = async () => { try { repaintList(await invoke('project_todos_toggle', { taskId, todoId: t.id })); } catch (e) { console.error(e); } };
          del.onclick = async () => { try { repaintList(await invoke('project_todos_remove', { taskId, todoId: t.id })); } catch (e) { console.error(e); } };
          row.append(cb, span, del);
          listEl.appendChild(row);
        });
      };
      repaintList(todos);

      const add = async () => {
        const text = input.value.trim();
        if (!text) return;
        input.value = '';
        try { repaintList(await invoke('project_todos_add', { taskId, text })); } catch (e) { console.error(e); }
      };
      container.querySelector('.rpane-todo-addbtn').onclick = add;
      input.addEventListener('keydown', (e) => { if (e.key === 'Enter') { e.preventDefault(); add(); } });
      container.querySelector('.rpane-plan-btn').onclick = () => {
        if (window.xnautAttachPlanTab) window.xnautAttachPlanTab({ projectContext: { path: root, client_company: taskName } });
      };
    }

    async function render() {
      if (!container) return;
      container.innerHTML = '<div class="rpane-empty">Loading…</div>';
      const r = root ? String(root).replace(/\/+$/, '') : '';
      if (!r) { container.innerHTML = '<div class="rpane-empty">Open a project to track its tasks.</div>'; return; }
      let tasks = [];
      try { tasks = await invoke('tasks_list'); } catch (_) { tasks = []; }
      const task = (Array.isArray(tasks) ? tasks : []).find(
        (t) => t && t.kind === 'project' && t.path && String(t.path).replace(/\/+$/, '') === r,
      );
      if (!task) {
        container.innerHTML = '<div class="rpane-empty">This folder isn\'t a project yet.<br>Right-click it in Files → “Open as project”, then add tasks here.</div>';
        taskId = null;
        return;
      }
      taskId = task.id;
      let todos = [];
      try { todos = await invoke('project_todos_list', { taskId }); } catch (_) { todos = []; }
      paint(todos, task.name);
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
