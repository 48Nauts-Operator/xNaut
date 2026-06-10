// Tasks Mode v1.6 — integration glue between the new panels (chat, sidebar,
// tasks browser, automations, right pane) and app.js's tab/PTY system.
// All cross-panel window.xnaut* hooks that the panels guard for live here.
(function () {
  'use strict';

  const invoke = (...a) => window.__TAURI__.core.invoke(...a);

  function shellEscape(p) {
    return "'" + String(p).replace(/'/g, "'\\''") + "'";
  }

  // ── Open a LaunchSpec (from scaffold commands / worktree modal / chat) ──
  // Runs spec.command in a fresh command PTY and attaches it as a tab.
  window.xnautOpenLaunchSpec = async function (spec) {
    if (!spec || !spec.command) return;
    const result = await invoke('create_command_session', {
      config: {
        program: 'sh',
        args: ['-c', spec.command],
        workingDir: spec.cwd || null,
      },
    });
    const label = (spec.task && spec.task.name) || 'Task';
    window.xnautAttachAgentTab(result.session_id, label);
    if (spec.cwd) window.xnautRightPaneSetRoot && window.xnautRightPaneSetRoot(spec.cwd);
    window.xnautSidebarRefresh && window.xnautSidebarRefresh();
  };

  // ── Panel tabs (generic attach is provided by app.js) ──
  window.xnautAttachChatTab = (opts) =>
    window.xnautAttachPanelTab('Chat', 'xnautCreateChatPane', opts || {});
  window.xnautAttachTasksTab = (opts) =>
    window.xnautAttachPanelTab('Forge Tasks', 'xnautCreateTasksPanel', opts || {});
  window.xnautAttachAutomationsTab = (opts) =>
    window.xnautAttachPanelTab('Automations', 'xnautCreateAutomationsPanel', opts || {});

  // ── Sidebar navigation dispatch ──
  window.xnautSidebarNavigate = function (key, arg) {
    switch (key) {
      case 'tasks':
        window.xnautAttachTasksTab();
        break;
      case 'automations':
        window.xnautAttachAutomationsTab();
        break;
      case 'search':
        setRightPaneVisible(true);
        break;
      case 'new-project':
        // Chat is the scaffold entry point.
        window.xnautAttachChatTab();
        break;
      case 'open-task':
        openTask(arg).catch((e) => console.error('open-task failed:', e));
        break;
      case 'promote-task':
        promoteTask(arg).catch((e) => console.error('promote-task failed:', e));
        break;
      default:
        console.warn('[tasks-mode] unknown sidebar nav key:', key);
    }
  };

  async function openTask(task) {
    if (!task) return;
    if (task.zellij_session) {
      const result = await invoke('create_command_session', {
        config: {
          program: 'sh',
          args: ['-c', `zellij attach ${shellEscape(task.zellij_session)}`],
          workingDir: task.path || null,
        },
      });
      window.xnautAttachAgentTab(result.session_id, task.name);
    } else if (typeof window.createNewTab === 'function') {
      window.createNewTab();
    }
    if (task.path) window.xnautRightPaneSetRoot && window.xnautRightPaneSetRoot(task.path);
  }

  async function promoteTask(task) {
    if (!task) return;
    const settings = await invoke('settings_get');
    const labels = settings.categories.map((c) => c.label).join(', ');
    const category = prompt(`Promote "${task.name}" to project.\nCategory (${labels}):`, 'Development');
    if (!category) return;
    try {
      await invoke('scaffold_promote_task', { taskId: task.id, categoryLabel: category, forgeIndex: 0 });
      window.xnautSidebarRefresh && window.xnautSidebarRefresh();
      alert(`Promoted "${task.name}" — repo created on the default forge.`);
    } catch (e) {
      alert(`Promote failed: ${e}`);
    }
  }

  // ── File-click hooks (right-pane files/search views) ──

  // Folder click: write the shell-escaped path into the focused PTY (no newline).
  window.xnautInjectPathIntoTerminal = async function (path) {
    const tab = (window.tabs || tabsRef()).find((t) => t.id === (window.activeTabId || activeTabIdRef()));
    if (!tab) return;
    const focused = tab.terminals[tab.focusedPaneIndex || 0];
    if (!focused || !focused.sessionId) return;
    await invoke('write_to_terminal', { sessionId: focused.sessionId, data: shellEscape(path) });
  };

  // app.js declares `tabs`/`activeTabId` with let/const (global lexical scope,
  // not window properties) — indirect eval reads them without tripping strict mode.
  function tabsRef() {
    try { return (0, eval)('tabs') || []; } catch (_) { return []; }
  }
  function activeTabIdRef() {
    try { return (0, eval)('activeTabId'); } catch (_) { return null; }
  }

  // File click: open in the user's editor inside a new PTY tab.
  window.xnautOpenInEditor = async function (path) {
    let editor = '';
    try {
      const s = await invoke('settings_get');
      editor = (s.editor || '').trim();
    } catch (_) { /* settings unreadable → env fallback */ }
    const cmd = editor
      ? `${editor} ${shellEscape(path)}`
      : `\${EDITOR:-vi} ${shellEscape(path)}`;
    const result = await invoke('create_command_session', {
      config: { program: 'sh', args: ['-c', cmd], workingDir: null },
    });
    window.xnautAttachAgentTab(result.session_id, path.split('/').pop());
  };

  // Markdown click: render through marked into the TipTap markdown pane.
  window.xnautOpenMarkdownFile = async function (path) {
    const body = await invoke('read_file', { path });
    const text = typeof body === 'string' ? body : (body && body.content) || '';
    const html = typeof marked !== 'undefined'
      ? (marked.parse ? marked.parse(text) : marked(text))
      : `<pre>${text.replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c]))}</pre>`;
    await window.xnautAttachMarkdownTab({ filename: path.split('/').pop(), content: html, filePath: path });
  };

  // Open external URLs through the Tauri shell (window.open is unreliable in webviews).
  window.xnautOpenUrl = function (url) {
    try {
      if (window.__TAURI__?.shell?.open) window.__TAURI__.shell.open(url);
      else invoke('plugin:shell|open', { path: url });
    } catch (_) {
      window.open(url, '_blank');
    }
  };

  // ── Automation fired (scheduler emits automation://fire) ──
  // v1: launch the automation's agent in its project path with the prompt.
  // workspace="new_run" / session_mode nuances land in v1.6.1.
  window.xnautAutomationFired = async function (automation) {
    try {
      const resp = await invoke('agent_launch', {
        req: {
          agent_id: automation.agent_id,
          worktree_path: automation.project_path,
          prompt: automation.prompt,
          cols: null,
          rows: null,
        },
      });
      window.xnautAttachAgentTab(resp.session_id, `⚙ ${automation.name}`);
    } catch (e) {
      console.error('[tasks-mode] automation launch failed:', e);
    }
  };

  // ── Sidebar + right pane hosts ──

  function setSidebarVisible(show) {
    const host = document.getElementById('xnaut-sidebar-host');
    if (!host) return;
    host.style.display = show ? 'flex' : 'none';
    localStorage.setItem('xnaut-sidebar-visible', show ? '1' : '0');
    if (show && !host.dataset.mounted) {
      window.xnautMountSidebar && window.xnautMountSidebar(host);
      host.dataset.mounted = '1';
    }
  }

  function setRightPaneVisible(show) {
    const host = document.getElementById('xnaut-right-pane-host');
    if (!host) return;
    host.style.display = show ? 'flex' : 'none';
    localStorage.setItem('xnaut-right-pane-visible', show ? '1' : '0');
    if (show && !host.dataset.mounted) {
      window.xnautMountRightPane && window.xnautMountRightPane(host);
      host.dataset.mounted = '1';
      invoke('get_home_directory')
        .then((h) => window.xnautRightPaneSetRoot && window.xnautRightPaneSetRoot(h))
        .catch(() => {});
    }
  }

  window.xnautToggleSidebar = function () {
    const host = document.getElementById('xnaut-sidebar-host');
    setSidebarVisible(host && host.style.display === 'none');
  };
  window.xnautToggleRightPane = function () {
    const host = document.getElementById('xnaut-right-pane-host');
    setRightPaneVisible(host && host.style.display === 'none');
  };

  // ── Settings → Tasks Mode section (appended into #settings-panel) ──

  async function ensureSettingsSection() {
    const panel = document.getElementById('settings-panel');
    if (!panel || document.getElementById('tasksmode-settings')) return;
    const body = panel.querySelector('.settings-panel-body') || panel;

    let s;
    try { s = await invoke('settings_get'); } catch (e) {
      console.error('[tasks-mode] settings_get failed:', e);
      return;
    }

    const section = document.createElement('div');
    section.id = 'tasksmode-settings';
    section.innerHTML = `
      <h3 style="margin:16px 0 8px;">Tasks Mode (v1.6)</h3>
      <div class="tm-grid">
        <label>LLM endpoint</label><input id="tm-llm-endpoint" type="text" value="">
        <label>LLM model</label><input id="tm-llm-model" type="text" value="">
        <label>LLM API key</label><input id="tm-llm-key" type="password" value="" placeholder="(optional)">
        <label>Engram (Brain)</label>
        <span><input id="tm-engram-on" type="checkbox"> enabled <span id="tm-engram-dot" style="margin-left:6px;"></span></span>
        <label>Engram API URL</label><input id="tm-engram-url" type="text" value="" placeholder="http://stargate...:8085">
        <label>Editor</label><input id="tm-editor" type="text" value="" placeholder="(empty = $EDITOR)">
        <label>Project root</label><input id="tm-root" type="text" value="">
      </div>
      <div style="margin-top:8px;">
        <div style="font-size:12px; opacity:.7; margin-bottom:4px;">Forge hosts (first = default)</div>
        <div id="tm-forges"></div>
        <button id="tm-add-forge" class="btn" style="font-size:12px; margin-top:4px;">+ Add forge</button>
      </div>
      <div style="margin-top:10px;">
        <button id="tm-save" class="btn btn-primary" style="width:100%;">Save Tasks Mode settings</button>
        <div id="tm-status" style="font-size:12px; margin-top:4px; opacity:.8;"></div>
      </div>
      <style>
        #tasksmode-settings .tm-grid { display:grid; grid-template-columns: 120px 1fr; gap:6px 8px; align-items:center; }
        #tasksmode-settings .tm-grid label { font-size:12px; opacity:.8; }
        #tasksmode-settings input[type=text], #tasksmode-settings input[type=password], #tasksmode-settings select {
          background: var(--editor-surface, #1e1e1e); border: 1px solid var(--border-color, #333);
          color: var(--text-primary, #ddd); border-radius: 4px; padding: 4px 6px; font-size: 12px; width: 100%;
        }
        #tasksmode-settings .tm-forge-row { display:grid; grid-template-columns: 90px 1fr 110px 1fr 24px; gap:4px; margin-bottom:4px; }
      </style>
    `;
    body.appendChild(section);

    const $ = (id) => document.getElementById(id);
    $('tm-llm-endpoint').value = s.llm.endpoint || '';
    $('tm-llm-model').value = s.llm.model || '';
    $('tm-llm-key').value = s.llm.api_key || '';
    $('tm-engram-on').checked = !!s.engram.enabled;
    $('tm-engram-url').value = s.engram.url || '';
    $('tm-editor').value = s.editor || '';
    $('tm-root').value = s.project_root || '';

    const forgesEl = $('tm-forges');
    function forgeRow(f) {
      const row = document.createElement('div');
      row.className = 'tm-forge-row';
      row.innerHTML = `
        <select class="tm-f-kind">
          <option value="forgejo">Forgejo</option><option value="github">GitHub</option><option value="gitlab">GitLab</option>
        </select>
        <input class="tm-f-url" type="text" placeholder="base URL">
        <input class="tm-f-owner" type="text" placeholder="owner/org">
        <input class="tm-f-token" type="password" placeholder="token (optional)">
        <button class="tm-f-del btn-icon" title="Remove">×</button>
      `;
      row.querySelector('.tm-f-kind').value = f.kind || 'forgejo';
      row.querySelector('.tm-f-url').value = f.base_url || '';
      row.querySelector('.tm-f-owner').value = f.owner || '';
      row.querySelector('.tm-f-token').value = f.token || '';
      row.querySelector('.tm-f-del').onclick = () => row.remove();
      forgesEl.appendChild(row);
    }
    (s.forges || []).forEach(forgeRow);
    $('tm-add-forge').onclick = () => forgeRow({});

    // Engram reachability dot.
    invoke('engram_status').then((st) => {
      $('tm-engram-dot').textContent = st.enabled ? (st.reachable ? '🧠 connected' : '🧠 unreachable') : '';
    }).catch(() => {});

    $('tm-save').onclick = async () => {
      const updated = {
        ...s,
        project_root: $('tm-root').value.trim(),
        llm: {
          ...s.llm,
          endpoint: $('tm-llm-endpoint').value.trim(),
          model: $('tm-llm-model').value.trim(),
          api_key: $('tm-llm-key').value.trim() || null,
        },
        engram: { enabled: $('tm-engram-on').checked, url: $('tm-engram-url').value.trim() },
        editor: $('tm-editor').value.trim(),
        forges: Array.from(forgesEl.querySelectorAll('.tm-forge-row')).map((row) => ({
          kind: row.querySelector('.tm-f-kind').value,
          base_url: row.querySelector('.tm-f-url').value.trim(),
          owner: row.querySelector('.tm-f-owner').value.trim(),
          token: row.querySelector('.tm-f-token').value.trim() || null,
        })),
      };
      try {
        await invoke('settings_set', { settings: updated });
        s = updated;
        $('tm-status').textContent = '✓ saved';
        setTimeout(() => { $('tm-status').textContent = ''; }, 2500);
      } catch (e) {
        $('tm-status').textContent = `save failed: ${e}`;
      }
    };
  }

  // Append our section whenever the settings panel opens.
  const origToggleSettings = window.toggleSettingsPanel;
  if (typeof origToggleSettings === 'function') {
    window.toggleSettingsPanel = function (...args) {
      const r = origToggleSettings.apply(this, args);
      ensureSettingsSection().catch((e) => console.error(e));
      return r;
    };
  }

  // ── Boot wiring ──
  function wire() {
    const sbBtn = document.getElementById('btn-toggle-tasks-sidebar');
    if (sbBtn) sbBtn.onclick = () => window.xnautToggleSidebar();
    const rpBtn = document.getElementById('btn-toggle-right-pane');
    if (rpBtn) rpBtn.onclick = () => window.xnautToggleRightPane();
    const chatBtn = document.getElementById('btn-new-chat');
    if (chatBtn) chatBtn.onclick = () => window.xnautAttachChatTab();

    // Restore visibility prefs (slight delay so __TAURI__ + app.js globals exist).
    setTimeout(() => {
      if (localStorage.getItem('xnaut-sidebar-visible') === '1') setSidebarVisible(true);
      if (localStorage.getItem('xnaut-right-pane-visible') === '1') setRightPaneVisible(true);
    }, 400);
  }
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', wire);
  } else {
    wire();
  }
})();
