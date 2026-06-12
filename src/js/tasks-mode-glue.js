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
  window.xnautAttachPmTab = (opts) =>
    window.xnautAttachPanelTab('PM', 'xnautCreatePmPanel', opts || {});

  // ── Sidebar navigation dispatch ──
  window.xnautSidebarNavigate = function (key, arg) {
    switch (key) {
      case 'tasks':
        window.xnautAttachTasksTab();
        break;
      case 'automations':
        window.xnautAttachAutomationsTab();
        break;
      case 'pm':
        window.xnautAttachPmTab();
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

  // ── Settings → Tasks Mode section ──
  // Rendered into the #tasksmode-settings-host div that app.js's settings
  // section map creates — uses the same settings-group / settings-row markup
  // as the AI tab so it inherits the native settings styling.

  window.xnautRenderTasksModeSettings = async function (host) {
    if (!host) return;

    let s;
    try { s = await invoke('settings_get'); } catch (e) {
      host.innerHTML = `<p style="color:var(--text-secondary);">Failed to load settings: ${String(e)}</p>`;
      return;
    }

    host.innerHTML = `
      <h3>Chat LLM</h3>
      <div class="settings-group">
        <div class="settings-row">
          <label><span class="status-dot-sm gray" id="tm-llm-dot"></span>Endpoint</label>
          <input type="url" id="tm-llm-endpoint" placeholder="http://localhost:1234/v1">
          <button class="btn-test" id="tm-test-llm">Test</button>
        </div>
        <div class="settings-row">
          <label>Model</label>
          <input type="text" id="tm-llm-model" placeholder="model name">
        </div>
        <div class="settings-row">
          <label>API key</label>
          <input type="password" id="tm-llm-key" placeholder="(optional — local endpoints don't need one)">
        </div>
        <p style="color:var(--text-secondary); font-size:12px; margin:4px 0 0;">Any OpenAI-compatible endpoint: LM Studio <code>http://localhost:1234/v1</code>, Ollama <code>http://localhost:11434/v1</code>, NautGate <code>http://localhost:8090/v1</code>. Test saves first, then checks <code>/models</code>.</p>
      </div>

      <h3>Engram (Brain)</h3>
      <div class="settings-group">
        <div class="settings-row">
          <label><span class="status-dot-sm gray" id="tm-engram-dot"></span>Enable memory</label>
          <input type="checkbox" id="tm-engram-on" style="width:auto; flex:none;">
        </div>
        <div class="settings-row">
          <label>API URL</label>
          <input type="url" id="tm-engram-url" placeholder="http://stargate.tail138398.ts.net:8085">
        </div>
      </div>

      <h3>Projects</h3>
      <div class="settings-group">
        <div class="settings-row">
          <label>Project root</label>
          <input type="text" id="tm-root">
        </div>
        <div class="settings-row">
          <label>Editor</label>
          <input type="text" id="tm-editor" placeholder="(empty = $EDITOR)">
        </div>
      </div>

      <h3>Forge Hosts</h3>
      <div class="settings-group">
        <p style="color:var(--text-secondary); font-size:12px; margin:0 0 8px;">First entry is the default host for new projects.</p>
        <div id="tm-forges"></div>
        <button class="btn" id="tm-add-forge" style="font-size:12px; margin-top:4px;">+ Add forge</button>
      </div>

      <button id="tm-save" class="btn btn-primary" style="width:100%; margin-top:8px;">Save Tasks Mode Settings</button>
      <div id="tm-status" style="font-size:12px; margin-top:6px; color:var(--text-secondary); text-align:center;"></div>
      <style>
        #tasksmode-settings-host .tm-forge-row { display:grid; grid-template-columns: 100px 1fr 120px 1fr 28px; gap:6px; margin-bottom:6px; align-items:center; }
        #tasksmode-settings-host .tm-forge-row input, #tasksmode-settings-host .tm-forge-row select {
          background: var(--bg-secondary, #1e1e1e); border: 1px solid var(--border, #333);
          color: var(--text-primary, #ddd); border-radius: 6px; padding: 6px 8px; font-size: 12px; width: 100%;
        }
      </style>
    `;

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
      $('tm-engram-dot').className = 'status-dot-sm ' + (st.enabled ? (st.reachable ? 'green' : 'red') : 'gray');
    }).catch(() => {});

    async function saveAll() {
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
      await invoke('settings_set', { settings: updated });
      s = updated;
    }

    $('tm-save').onclick = async (e) => {
      try {
        await saveAll();
        if (window.flashSavedButton) window.flashSavedButton(e.target);
        $('tm-status').textContent = '✓ Settings saved';
        $('tm-status').style.color = '#34d399';
      } catch (err) {
        $('tm-status').textContent = `Save failed: ${err}`;
        $('tm-status').style.color = '#ef4444';
      }
      setTimeout(() => { $('tm-status').textContent = ''; }, 3000);
    };

    // Test = save the form first, then probe {endpoint}/models from the backend.
    // The button itself carries the result: green "Connected ✓" / red "Failed ✗".
    $('tm-test-llm').onclick = async (e) => {
      const btn = e.target;
      const dot = $('tm-llm-dot');
      btn.classList.remove('ok', 'fail');
      btn.textContent = 'Testing…';
      btn.disabled = true;
      dot.className = 'status-dot-sm gray';
      let ok = false;
      let err = null;
      try {
        await saveAll();
        ok = await invoke('chat_check_endpoint');
      } catch (ex) {
        err = ex;
      }
      btn.disabled = false;
      btn.classList.add(ok ? 'ok' : 'fail');
      btn.textContent = ok ? 'Connected ✓' : 'Failed ✗';
      dot.className = 'status-dot-sm ' + (ok ? 'green' : 'red');
      $('tm-status').textContent = err
        ? `Test failed: ${err}`
        : ok
          ? '✓ Saved — endpoint is reachable'
          : '✗ Saved, but the endpoint did not respond (is the server running?)';
      $('tm-status').style.color = ok ? '#34d399' : '#ef4444';
      setTimeout(() => { $('tm-status').textContent = ''; }, 5000);
    };
  };

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
