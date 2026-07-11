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
    chat: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" width="16" height="16"><path d="M2.5 3.5h11v7h-6l-3 3v-3h-2z"/></svg>',
    search: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" width="16" height="16"><circle cx="7" cy="7" r="4.5"/><line x1="10.5" y1="10.5" x2="14" y2="14"/></svg>',
    git: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" width="16" height="16"><circle cx="4.5" cy="3.5" r="1.8"/><circle cx="4.5" cy="12.5" r="1.8"/><circle cx="11.5" cy="6" r="1.8"/><path d="M4.5 5.3v5.4"/><path d="M11.5 7.8c0 2.5-3 2.5-5 3.2"/></svg>',
    tasks: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" width="16" height="16"><path d="M2.5 4l1.2 1.2L6 2.9"/><path d="M2.5 9.5l1.2 1.2L6 8.4"/><line x1="8" y1="4.2" x2="14" y2="4.2"/><line x1="8" y1="9.7" x2="14" y2="9.7"/><line x1="2.5" y1="13.5" x2="14" y2="13.5"/></svg>',
    librarian: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" width="16" height="16"><path d="M3 3.5h10v6.8H7.4L4 13.2v-2.9H3z"/><path d="M5 5.8h6"/><path d="M5 8h4"/></svg>',
    plus: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" width="16" height="16"><path d="M8 3v10"/><path d="M3 8h10"/></svg>',
  };
  const LIBRARIAN_VIEW = { key: 'librarian', title: 'Librarian Conversations' };
  const VIEW_ORDER = [
    { key: 'files', title: 'Files' },
    { key: 'chat', title: 'Chat' },
    { key: 'search', title: 'Search' },
    { key: 'git', title: 'Git' },
    { key: 'tasks', title: 'Tasks' },
  ];

  const STYLES = `
.rpane-host { position:relative; display:flex; flex-direction:column; height:100%; min-height:0; min-width:0; background:var(--bg-secondary); border-left:1px solid var(--border); font-family:var(--font-sans, sans-serif); }
.rpane-resize { position:absolute; z-index:20; top:0; bottom:0; left:-4px; width:8px; cursor:col-resize; touch-action:none; }
.rpane-resize::after { content:''; position:absolute; top:0; bottom:0; left:3px; width:1px; background:transparent; transition:background .12s; }
.rpane-resize:hover::after, .rpane-host.rpane-resizing .rpane-resize::after { background:var(--accent, #4f8cff); }
.rpane-bar { display:flex; align-items:center; gap:2px; flex:0 0 36px; height:36px; padding:0 8px; border-bottom:1px solid var(--border); }
.rpane-tab { display:flex; align-items:center; justify-content:center; width:28px; height:28px; border:none; border-radius:var(--radius-md, 6px); background:transparent; color:var(--text-secondary); cursor:pointer; padding:0; }
.rpane-tab:hover { background:var(--bg-tertiary); color:var(--text-primary); }
.rpane-tab.rpane-active { background:var(--bg-tertiary); color:var(--accent); }
.rpane-bar-separator { flex:0 0 1px; width:1px; height:18px; margin:0 4px; background:var(--border); }
.rpane-title { margin-left:auto; font-size:11px; color:var(--text-secondary); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; max-width:45%; }
.rpane-content { flex:1 1 0%; min-height:0; position:relative; display:flex; flex-direction:column; }
.rpane-view { flex:1 1 0%; min-height:0; overflow-y:auto; display:none; flex-direction:column; }
.rpane-view.rpane-view-active { display:flex; }
.rpane-empty { padding:16px 12px; font-size:12px; color:var(--text-secondary); text-align:center; }
.rpane-chat-shell { display:flex; flex:1 1 auto; flex-direction:column; min-height:0; overflow:hidden; }
.rpane-chat-control { display:flex; flex-direction:column; gap:5px; flex:0 0 auto; padding:7px 9px; border-bottom:1px solid var(--border); }
.rpane-chat-control-row { display:grid; grid-template-columns:42px minmax(0,1fr); align-items:center; gap:7px; width:100%; }
.rpane-chat-control label { color:var(--text-secondary); font-size:10px; font-weight:650; text-transform:uppercase; }
.rpane-chat-agent,.rpane-chat-model { min-width:0; width:100%; height:28px; padding:3px 7px; border:1px solid var(--border); border-radius:6px; background:var(--input-bg,rgba(255,255,255,.05)); color:var(--text-primary); font:inherit; font-size:12px; outline:none; }
.rpane-chat-agent:focus,.rpane-chat-model:focus { border-color:var(--accent,#4f8cff); }
.rpane-chat-history-meta { align-self:flex-end; color:var(--text-secondary); font-size:10px; white-space:nowrap; }
.rpane-chat-body { display:flex; flex:1 1 auto; min-height:0; overflow:hidden; }
.rpane-librarian-panel { flex-direction:column; min-height:0; }
.rpane-librarian-head { display:flex; align-items:center; justify-content:space-between; gap:8px; padding:9px 10px; border-bottom:1px solid var(--border); }
.rpane-librarian-title { min-width:0; font-size:12px; font-weight:600; color:var(--text-primary, #e8eaf0); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
.rpane-librarian-new { flex:0 0 auto; display:flex; align-items:center; justify-content:center; width:24px; height:24px; border:none; border-radius:var(--radius-md, 6px); background:var(--accent, #4f8cff); color:var(--accent-foreground,#fff); cursor:pointer; padding:0; }
.rpane-librarian-new:hover { filter:brightness(1.08); }
.rpane-librarian-new svg { width:14px; height:14px; }
.rpane-librarian-vault { flex:0 0 auto; font-size:10px; color:var(--text-secondary, #8a8f98); border:1px solid var(--border); border-radius:999px; padding:1px 6px; }
.rpane-librarian-list { display:flex; flex-direction:column; min-height:0; }
.rpane-librarian-item { display:flex; flex-direction:column; gap:3px; padding:8px 10px; border-bottom:1px solid var(--border, rgba(255,255,255,.08)); cursor:pointer; background:transparent; }
.rpane-librarian-item:hover { background:var(--hover-bg, rgba(255,255,255,.07)); }
.rpane-librarian-item[data-current="1"] { background:color-mix(in srgb, var(--accent, #4f8cff) 12%, transparent); }
.rpane-librarian-item-title { font-size:12px; color:var(--text-primary, #e8eaf0); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
.rpane-librarian-item-meta { display:flex; gap:7px; font-size:10px; color:var(--text-secondary, #8a8f98); }
.rpane-librarian-preview { font-size:11px; color:var(--text-secondary, #8a8f98); line-height:1.35; white-space:normal; overflow:hidden; display:-webkit-box; -webkit-line-clamp:2; -webkit-box-orient:vertical; }
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
.rpane-todo-addbtn { flex:0 0 auto; width:28px; background:var(--accent, #4f8cff); color:var(--accent-foreground,#fff); border:none; border-radius:6px; font-size:16px; line-height:1; cursor:pointer; }
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

  // ---- Persistent general / agent review chat -------------------------
  function createChatView() {
    const selectionKey = 'xnaut-right-pane:model-selection:v1';
    let savedSelection = {};
    try { savedSelection = JSON.parse(localStorage.getItem(selectionKey) || '{}') || {}; } catch (_) { savedSelection = {}; }
    let container = null;
    let entry = null;
    let generation = 0;
    let options = { title: 'Chat', chatKey: 'right-pane:chat' };
    let profiles = [];
    let selectedProfileKey = '';
    let selectedModel = String(savedSelection.model || '');
    let selectedProvider = String(savedSelection.provider || '');
    let availableModels = [];
    let globalModel = '';
    let globalProvider = '';
    let mcpTools = [];
    let contextKey = '';
    let selectionHydrated = false;

    async function persistModelSelection() {
      if (selectedProvider && selectedModel) {
        localStorage.setItem(selectionKey, JSON.stringify({ provider: selectedProvider, model: selectedModel }));
      } else {
        localStorage.removeItem(selectionKey);
      }
      const current = await invoke('settings_get');
      current.agent_chat_selection = { provider: selectedProvider, model: selectedModel };
      await invoke('settings_set', { settings: current });
    }

    function profileKey(profile) {
      return String(profile?.rel || profile?.id || profile?.name || '');
    }

    function profilePrompt(profile) {
      if (!profile) return '';
      return [
        `You are ${profile.name || 'the selected Agent'}, the xNAUT ${profile.role || 'project Agent'}.`,
        profile.body ? `Profile:\n${profile.body}` : '',
        (profile.skills || []).length ? `Skills:\n- ${profile.skills.join('\n- ')}` : '',
        (profile.tools || []).length ? `Tools:\n- ${profile.tools.join('\n- ')}` : '',
        (profile.constraints || []).length ? `Constraints:\n- ${profile.constraints.join('\n- ')}` : '',
        (profile.outputs || []).length ? `Expected outputs:\n- ${profile.outputs.join('\n- ')}` : '',
      ].filter(Boolean).join('\n\n');
    }

    function chooseProfile() {
      if (selectedProfileKey && profiles.some((profile) => profileKey(profile) === selectedProfileKey)) return;
      const preferred = String(options.preferredAgentRole || '').toLowerCase();
      const explicit = String(options.agentProfileId || '');
      const match = profiles.find((profile) => profileKey(profile) === explicit)
        || profiles.find((profile) => preferred && [profile.name, profile.role, profile.id].some((value) => String(value || '').toLowerCase() === preferred))
        || profiles.find((profile) => profileKey(profile) === localStorage.getItem('xnaut-agents:default-agent'));
      selectedProfileKey = match ? profileKey(match) : '';
    }

    function effectiveOptions(profile) {
      const baseKey = options.chatKeyBase || options.chatKey || 'right-pane:chat';
      const key = profile ? (profile.id || profile.name || profileKey(profile)) : 'assistant';
      const runtime = profile?.runtime || {};
      const assignedModel = runtime.provider && runtime.provider !== 'global' ? String(runtime.model || '').trim() : globalModel;
      const configuredProviders = new Set([globalProvider, ...availableModels.map((item) => String(item?.provider || ''))]);
      const assignedProvider = runtime.provider && runtime.provider !== 'global' && configuredProviders.has(String(runtime.provider)) ? String(runtime.provider) : globalProvider;
      const mcpPrompt = mcpTools.length ? [
        'Local Excalidraw drawing tools are available through MCP. Call one tool at a time using ONLY JSON:',
        '{"action":"mcp_call","server":"excalidraw","tool":"TOOL_NAME","arguments":{}}',
        'Available tools:',
        ...mcpTools.map((tool) => `- ${tool.name}: ${tool.description || ''}\n  input: ${JSON.stringify(tool.inputSchema || {})}`),
      ].join('\n') : '';
      const buildsLoops = profile?.id === 'loopbuilder';
      const loopPrompt = buildsLoops ? [
        'When the user has described enough detail, create a draft Agent Loop using ONLY one JSON object:',
        '{"action":"loop_create","name":"Loop name","description":"Purpose","project":null,"nodes":[{"id":"start","kind":"trigger","name":"Start","next":"work"},{"id":"work","kind":"agent","name":"Do work","next":"review"},{"id":"review","kind":"decision","name":"Approved?","branches":{"yes":"done","no":"retry"}},{"id":"retry","kind":"retry","name":"Refine","next":"work"},{"id":"done","kind":"output","name":"Complete"}]}',
        'Allowed kinds: trigger, agent, action, decision, human_approval, transform, retry, parallel, subflow, output.',
        'Use next for one route and branches for named routes. Every cycle must pass through a retry node. Include human_approval before privileged or irreversible actions.',
        'Do not include Markdown fences or prose around the JSON. The system compiles, validates, and saves the draft; it does not activate it.',
      ].join('\n') : '';
      const workspaceContext = window.xnautGetAgentWorkspaceContext?.();
      const profileWrites = !profile || (profile.access?.write || []).some((scope) => ['vault', 'assigned_files', 'source_code', 'repo'].includes(scope));
      const vaultPrompt = workspaceContext ? `You may read documents from the active ${workspaceContext.vault || 'work'} Vault.${profileWrites ? ` When the user asks you to draft, improve, or edit the active document, apply the result with ONLY {"action":"vault_write","rel":"${workspaceContext.rel}","content":"COMPLETE DOCUMENT"}. You may write only that exact active path.` : ''} When the user asks you to inspect a referenced document, reply first with ONLY {"action":"vault_read","rel":"relative/path.md"}. Use paths relative to the Vault and never include the "work:" prefix.` : '';
      const workspaceVaultTools = workspaceContext ? {
        vault: () => workspaceContext.vault || 'work',
        entry: null,
        readOnly: !profileWrites,
        writeRel: profileWrites ? () => window.xnautGetAgentWorkspaceContext?.()?.rel || '' : null,
        onWrite: workspaceContext.onWrite,
      } : null;
      const vaultTools = options.vaultTools
        ? Object.assign({}, options.vaultTools, workspaceContext?.onWrite ? { onWrite: workspaceContext.onWrite } : {})
        : workspaceVaultTools;
      const prompt = [profilePrompt(profile), options.systemPromptAppend || '', vaultPrompt, mcpPrompt, loopPrompt].filter(Boolean).join('\n\n');
      return Object.assign({}, options, {
        title: profile ? `${profile.name || 'Agent'} · ${options.title || 'Chat'}` : (options.title || 'Chat'),
        chatKey: profile ? `${baseKey}:${key}` : baseKey,
        systemPromptAppend: prompt,
        modelOverride: selectedModel || String(options.modelOverride || '').trim() || assignedModel,
        providerOverride: selectedProvider || (assignedProvider !== globalProvider ? assignedProvider : ''),
        vaultTools,
        mcpTools: mcpTools.length ? { server: 'excalidraw', tools: mcpTools } : null,
        loopTools: buildsLoops,
        embedded: true,
      });
    }

    async function remount() {
      if (!container || typeof window.xnautCreateChatPane !== 'function') return;
      const current = ++generation;
      if (entry && window.xnautDestroyChatPane) {
        await window.xnautDestroyChatPane(entry.label).catch(() => {});
        entry = null;
      }
      let settings = null;
      try {
        if (window.xnautSyncChatSettingsFromAiSettings) {
          await window.xnautSyncChatSettingsFromAiSettings().catch(() => false);
        }
        const loaded = await Promise.all([invoke('agent_profiles_seed').catch(() => invoke('agent_profiles_list').catch(() => [])), invoke('settings_get').catch(() => null)]);
        profiles = (loaded[0] || []).filter((profile) => profile && profile.status !== 'disabled');
        settings = loaded[1];
        if (!selectionHydrated) {
          const durable = settings?.agent_chat_selection || {};
          if (durable.provider && durable.model) {
            selectedProvider = String(durable.provider);
            selectedModel = String(durable.model);
          } else if (selectedProvider && selectedModel) {
            await persistModelSelection();
          }
          selectionHydrated = true;
        }
        globalModel = String(settings?.llm?.model || '').trim();
        globalProvider = String(settings?.llm?.provider || '').trim();
        availableModels = await invoke('chat_list_provider_models').catch(() => []);
        const configuredProviders = new Set([globalProvider, ...(settings?.llm_providers || []).filter((item) => item?.enabled).map((item) => String(item.name || ''))]);
        if (selectedProvider && !configuredProviders.has(selectedProvider)) {
          selectedProvider = '';
          selectedModel = '';
          await persistModelSelection();
        }
        const drawingTools = new Set(['read_me', 'create_view', 'list_scenes', 'create_scene', 'get_scene', 'search_scene_content', 'get_scene_content', 'read_excalidraw_format', 'edit_scene_content']);
        mcpTools = ((await invoke('mcp_list_tools', { server: 'excalidraw' }).catch(() => [])) || []).filter((tool) => drawingTools.has(tool?.name));
      } catch (_) { profiles = []; availableModels = []; }
      if (current !== generation || !container) return;
      chooseProfile();
      const profile = profiles.find((item) => profileKey(item) === selectedProfileKey) || null;
      const runtime = profile?.runtime || {};
      const assignedModel = runtime.provider && runtime.provider !== 'global' ? String(runtime.model || '').trim() : globalModel;
      const configuredProviders = new Set([globalProvider, ...availableModels.map((item) => String(item?.provider || ''))]);
      const assignedProvider = runtime.provider && runtime.provider !== 'global' && configuredProviders.has(String(runtime.provider)) ? String(runtime.provider) : globalProvider;
      const activeModel = selectedModel || String(options.modelOverride || '').trim() || assignedModel;
      const activeProvider = selectedProvider || assignedProvider;
      const modelChoices = [];
      const seenModels = new Set();
      const addModel = (provider, model) => {
        if (!model) return;
        const key = `${provider}\t${model}`;
        if (seenModels.has(key)) return;
        seenModels.add(key);
        modelChoices.push({ provider, model });
      };
      addModel(activeProvider, activeModel);
      addModel(globalProvider, globalModel);
      availableModels.forEach((item) => addModel(String(item?.provider || ''), String(item?.model || '')));
      const chatOptions = effectiveOptions(profile);
      const historyCount = typeof window.xnautGetChatHistory === 'function' ? (window.xnautGetChatHistory(chatOptions.chatKey) || []).length : 0;
      container.innerHTML = `<div class="rpane-chat-shell"><div class="rpane-chat-control"><div class="rpane-chat-control-row"><label for="rpane-chat-agent">Agent</label><select id="rpane-chat-agent" class="rpane-chat-agent"><option value="">Assistant</option>${profiles.map((item) => `<option value="${escapeText(profileKey(item))}"${profileKey(item) === selectedProfileKey ? ' selected' : ''}>${escapeText(item.name || item.id || item.rel)}</option>`).join('')}</select></div><div class="rpane-chat-control-row"><label for="rpane-chat-model">Model</label><select id="rpane-chat-model" class="rpane-chat-model">${modelChoices.length ? modelChoices.map((item) => { const value = `${item.provider}\t${item.model}`; return `<option value="${escapeText(value)}"${item.model === activeModel && item.provider === activeProvider ? ' selected' : ''}>${escapeText(item.provider ? `${item.provider} · ${item.model}` : item.model)}</option>`; }).join('') : '<option value="">Global default</option>'}</select></div>${historyCount ? `<span class="rpane-chat-history-meta">${historyCount} messages</span>` : ''}</div><div class="rpane-chat-body"></div></div>`;
      const select = container.querySelector('.rpane-chat-agent');
      select.onchange = () => { selectedProfileKey = select.value; remount().catch((e) => { if (container) container.innerHTML = `<div class="rpane-empty">${escapeText(String(e))}</div>`; }); };
      const modelSelect = container.querySelector('.rpane-chat-model');
      modelSelect.onchange = async () => { const parts = modelSelect.value.split('\t'); selectedProvider = parts.shift() || ''; selectedModel = parts.join('\t'); await persistModelSelection(); remount().catch((e) => { if (container) container.innerHTML = `<div class="rpane-empty">${escapeText(String(e))}</div>`; }); };
      const body = container.querySelector('.rpane-chat-body');
      const created = await window.xnautCreateChatPane('right-pane-chat', body, chatOptions);
      if (current !== generation) {
        if (created && window.xnautDestroyChatPane) window.xnautDestroyChatPane(created.label).catch(() => {});
        return;
      }
      entry = created;
    }

    return {
      mount(el) { container = el; remount().catch((e) => { container.innerHTML = `<div class="rpane-empty">${escapeText(String(e))}</div>`; }); },
      setRoot() {},
      open(nextOptions) {
        options = Object.assign({ title: 'Chat', chatKey: 'right-pane:chat' }, nextOptions || {});
        const nextContext = options.chatKeyBase || options.chatKey || 'right-pane:chat';
        if (nextContext !== contextKey) selectedProfileKey = '';
        contextKey = nextContext;
        if (container) remount().catch((e) => { container.innerHTML = `<div class="rpane-empty">${escapeText(String(e))}</div>`; });
      },
      destroy() {
        generation += 1;
        if (entry && window.xnautDestroyChatPane) window.xnautDestroyChatPane(entry.label).catch(() => {});
        entry = null;
        container = null;
      },
    };
  }
  const chatView = createChatView();
  registerView('chat', chatView);

  // ---- Vault Librarian conversation history ---------------------------
  const VAULT_CONV_PREFIX = 'xnaut-vault-conversations:';
  const CHAT_HIST_PREFIX = 'xnaut-chat-history:';

  function activeVaultName() {
    return localStorage.getItem('xnaut-vault:last') || 'work';
  }

  function librarianChatKey(vault) {
    return 'vault:' + (vault || activeVaultName());
  }

  function isToolOnlyAssistantMessage(m) {
    if (!m || m.role !== 'assistant') return false;
    const text = String(m.content || '').trim();
    return /^\{[\s\S]*"action"\s*:\s*"vault_[^"]+"[\s\S]*\}$/.test(text)
      || /^```(?:json)?\s*\n?\{[\s\S]*"action"\s*:\s*"vault_[^"]+"[\s\S]*\}\s*```$/i.test(text);
  }

  function visibleLibrarianMessages(history) {
    return (Array.isArray(history) ? history : [])
      .filter((m) => (m.role === 'user' || m.role === 'assistant') && !isToolOnlyAssistantMessage(m));
  }

  function readCurrentLibrarianHistory(vault) {
    const key = librarianChatKey(vault);
    if (window.xnautGetChatHistory) return window.xnautGetChatHistory(key) || [];
    try { return JSON.parse(localStorage.getItem(CHAT_HIST_PREFIX + key) || '[]') || []; } catch (_) { return []; }
  }

  function readArchivedLibrarianConversations(vault) {
    try {
      const parsed = JSON.parse(localStorage.getItem(VAULT_CONV_PREFIX + (vault || activeVaultName())) || '[]');
      return Array.isArray(parsed) ? parsed : [];
    } catch (_) {
      return [];
    }
  }

  function writeArchivedLibrarianConversations(vault, conversations) {
    localStorage.setItem(VAULT_CONV_PREFIX + (vault || activeVaultName()), JSON.stringify((conversations || []).slice(0, 50)));
    document.dispatchEvent(new CustomEvent('xnaut:librarian-conversations-changed', {
      detail: { vault: vault || activeVaultName() },
    }));
  }

  function conversationTitle(messages) {
    const firstUser = visibleLibrarianMessages(messages).find((m) => m.role === 'user');
    const text = String(firstUser && (firstUser.display || firstUser.content) || 'Librarian conversation').replace(/\s+/g, ' ').trim();
    return text.length > 54 ? text.slice(0, 51) + '...' : text;
  }

  function conversationPreview(messages) {
    const last = [...visibleLibrarianMessages(messages)].reverse().find((m) => m.role === 'assistant' || m.role === 'user');
    const text = String(last && (last.display || last.content) || '').replace(/\s+/g, ' ').trim();
    return text.length > 130 ? text.slice(0, 127) + '...' : text;
  }

  function archiveCurrentLibrarianConversation() {
    const vault = activeVaultName();
    const messages = readCurrentLibrarianHistory(vault);
    const visible = visibleLibrarianMessages(messages);
    if (!visible.length) return null;
    const archived = readArchivedLibrarianConversations(vault);
    const fingerprint = JSON.stringify(visible.map((m) => [m.role, m.content, m.display || '']));
    const existing = archived.find((c) => c && c.fingerprint === fingerprint);
    if (existing) return existing;
    const now = new Date().toISOString();
    const item = {
      id: 'conv-' + Date.now().toString(36),
      vault,
      title: conversationTitle(messages),
      preview: conversationPreview(messages),
      createdAt: now,
      updatedAt: now,
      count: visible.length,
      fingerprint,
      messages,
    };
    archived.unshift(item);
    writeArchivedLibrarianConversations(vault, archived);
    return item;
  }

  function restoreLibrarianConversation(id) {
    const vault = activeVaultName();
    archiveCurrentLibrarianConversation();
    const item = readArchivedLibrarianConversations(vault).find((c) => c && c.id === id);
    if (!item) return;
    if (window.xnautSetChatHistory) window.xnautSetChatHistory(librarianChatKey(vault), item.messages || []);
    else {
      localStorage.setItem(CHAT_HIST_PREFIX + librarianChatKey(vault), JSON.stringify(item.messages || []));
      document.dispatchEvent(new CustomEvent('xnaut:chat-history-changed', {
        detail: { chatKey: librarianChatKey(vault), history: item.messages || [] },
      }));
    }
  }

  function startNewLibrarianConversation() {
    const vault = activeVaultName();
    archiveCurrentLibrarianConversation();
    if (window.xnautClearChatHistory) window.xnautClearChatHistory(librarianChatKey(vault));
    else {
      localStorage.removeItem(CHAT_HIST_PREFIX + librarianChatKey(vault));
      document.dispatchEvent(new CustomEvent('xnaut:chat-history-changed', {
        detail: { chatKey: librarianChatKey(vault), history: [] },
      }));
    }
    document.dispatchEvent(new CustomEvent('xnaut:librarian-conversations-changed', { detail: { vault } }));
  }

  function createLibrarianView() {
    let container = null;

    function render() {
      if (!container) return;
      const vault = activeVaultName();
      const current = visibleLibrarianMessages(readCurrentLibrarianHistory(vault));
      const archived = readArchivedLibrarianConversations(vault);
      container.classList.add('rpane-librarian-panel');
      container.innerHTML = `
        <div class="rpane-librarian-head">
          <span class="rpane-librarian-title">Librarian conversations</span>
          <button class="rpane-librarian-new" title="New Librarian conversation" aria-label="New Librarian conversation">${ICONS.plus}</button>
          <span class="rpane-librarian-vault">${escapeText(vault)}</span>
        </div>
        <div class="rpane-librarian-list"></div>`;
      const list = container.querySelector('.rpane-librarian-list');
      container.querySelector('.rpane-librarian-new').onclick = () => {
        startNewLibrarianConversation();
        render();
      };
      if (current.length) {
        const row = document.createElement('div');
        row.className = 'rpane-librarian-item';
        row.dataset.current = '1';
        row.innerHTML = `
          <span class="rpane-librarian-item-title">Current - ${escapeText(conversationTitle(current))}</span>
          <span class="rpane-librarian-preview">${escapeText(conversationPreview(current))}</span>
          <span class="rpane-librarian-item-meta"><span>${current.length} messages</span><span>active</span></span>`;
        list.appendChild(row);
      }
      archived.forEach((conv) => {
        const row = document.createElement('div');
        row.className = 'rpane-librarian-item';
        row.dataset.convId = conv.id;
        row.innerHTML = `
          <span class="rpane-librarian-item-title">${escapeText(conv.title || 'Librarian conversation')}</span>
          <span class="rpane-librarian-preview">${escapeText(conv.preview || '')}</span>
          <span class="rpane-librarian-item-meta"><span>${Number(conv.count || 0)} messages</span><span>${escapeText((conv.updatedAt || '').slice(0, 16).replace('T', ' '))}</span></span>`;
        row.onclick = () => {
          restoreLibrarianConversation(conv.id);
          render();
        };
        list.appendChild(row);
      });
      if (!current.length && !archived.length) {
        list.innerHTML = '<div class="rpane-empty">No Librarian conversations yet.</div>';
      }
    }

    const rerender = () => render();
    return {
      mount(el) {
        container = el;
        render();
        document.addEventListener('xnaut:chat-history-changed', rerender);
        document.addEventListener('xnaut:librarian-conversations-changed', rerender);
      },
      setRoot() { render(); },
      destroy() {
        document.removeEventListener('xnaut:chat-history-changed', rerender);
        document.removeEventListener('xnaut:librarian-conversations-changed', rerender);
        container = null;
      },
    };
  }
  registerView(LIBRARIAN_VIEW.key, createLibrarianView());

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
    const savedWidth = Number(localStorage.getItem('xnaut-right-pane-width'));
    if (Number.isFinite(savedWidth) && savedWidth >= 260) hostElement.style.width = `${savedWidth}px`;
    hostElement.innerHTML = `
      <div class="rpane-resize" title="Resize pane" aria-hidden="true"></div>
      <div class="rpane-bar">
        ${VIEW_ORDER.map((v) => `<button class="rpane-tab" data-rpane-view="${v.key}" title="${v.title}" aria-label="${v.title}">${ICONS[v.key]}</button>`).join('')}
        <span class="rpane-bar-separator"></span>
        <button class="rpane-tab rpane-librarian-history" data-rpane-view="${LIBRARIAN_VIEW.key}" title="${LIBRARIAN_VIEW.title}" aria-label="${LIBRARIAN_VIEW.title}">${ICONS.librarian}</button>
        <span class="rpane-title" title=""></span>
      </div>
      <div class="rpane-content"></div>
    `;
    const content = hostElement.querySelector('.rpane-content');
    const titleEl = hostElement.querySelector('.rpane-title');

    const viewSlots = new Map();
    for (const v of VIEW_ORDER.concat([LIBRARIAN_VIEW])) {
      const el = document.createElement('div');
      el.className = 'rpane-view';
      el.dataset.rpaneSlot = v.key;
      content.appendChild(el);
      viewSlots.set(v.key, { el, mounted: false, root: null });
    }

    mountedState = { host: hostElement, root: null, activeKey: 'files', viewSlots, titleEl };

    const resizeHandle = hostElement.querySelector('.rpane-resize');
    let resizing = false;
    function resizeMove(e) {
      if (!resizing) return;
      const max = Math.max(360, Math.min(960, Math.floor(window.innerWidth * 0.72)));
      const width = Math.max(260, Math.min(max, window.innerWidth - e.clientX));
      hostElement.style.width = `${width}px`;
      localStorage.setItem('xnaut-right-pane-width', String(width));
      window.dispatchEvent(new Event('resize'));
    }
    function resizeEnd() {
      if (!resizing) return;
      resizing = false;
      hostElement.classList.remove('rpane-resizing');
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
      window.removeEventListener('pointermove', resizeMove);
      window.removeEventListener('pointerup', resizeEnd);
    }
    resizeHandle.addEventListener('pointerdown', (e) => {
      e.preventDefault();
      resizing = true;
      hostElement.classList.add('rpane-resizing');
      document.body.style.cursor = 'col-resize';
      document.body.style.userSelect = 'none';
      window.addEventListener('pointermove', resizeMove);
      window.addEventListener('pointerup', resizeEnd);
    });

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
        if (!b.dataset.rpaneView) return;
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

    function showLibrarianConversations() {
      setActive(LIBRARIAN_VIEW.key);
    }

    function openChat(opts) {
      chatView.open(opts || {});
      setActive('chat');
    }

    function destroyHost() {
      if (!mountedState) return;
      resizeEnd();
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
      showLibrarianConversations,
      openChat,
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
  window.xnautRightPaneShowLibrarianConversations = () => {
    if (!mountedState || !lastController || typeof lastController.showLibrarianConversations !== 'function') return false;
    lastController.showLibrarianConversations();
    return true;
  };
  window.xnautRightPaneOpenChat = (opts) => {
    if (!mountedState || !lastController || typeof lastController.openChat !== 'function') return false;
    lastController.openChat(opts || {});
    return true;
  };
})();
