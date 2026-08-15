// Plugin library (XNAUT-147) — a sub-surface of Agent Space, next to Skills.
//
// A plugin is an MCP server. Enabling one here is what makes it reach the
// coding harness at launch: claude gets a --mcp-config, codex gets -c
// overrides. Nothing is written into anyone else's config file.
//
// The catalog ships as starting points, not promises: package names and hosted
// endpoints move, so every field stays editable and the blocker line says
// exactly what is missing rather than failing inside an agent run.
(function () {
  'use strict';

  const invoke = (...args) => window.__TAURI__.core.invoke(...args);
  const esc = (value) => String(value == null ? '' : value).replace(/[&<>"']/g, (c) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  }[c]));

  const sure = (message, label) => (window.xnautConfirmDialog
    ? window.xnautConfirmDialog(message, label)
    : Promise.resolve(confirm(message)));
  const ask = (message, value, label) => (window.xnautPromptDialog
    ? window.xnautPromptDialog(message, value, label)
    : Promise.resolve(prompt(message, value)));

  const CATEGORY_ORDER = ['Docs & search', 'Knowledge', 'Work tracking', 'Comms', 'Dev', 'Design', 'Business'];

  function ensureStyles() {
    if (document.getElementById('plugins-panel-styles')) return;
    const style = document.createElement('style');
    style.id = 'plugins-panel-styles';
    style.textContent = `
      .plg { --plg-accent:#f5b840; display:flex; flex:1 1 auto; min-width:0; min-height:0; overflow:hidden;
        color:var(--text-primary,#e0e0e0); background:var(--bg-primary,#0a0a0f);
        font-family:var(--font-ui,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif); }
      .plg * { box-sizing:border-box; }
      .plg-list { display:flex; flex:0 0 310px; flex-direction:column; min-height:0; overflow-y:auto;
        border-right:1px solid var(--border,#2a2a2f); background:var(--bg-secondary,#141419); }
      .plg-head { display:flex; align-items:center; justify-content:space-between; gap:8px; padding:14px 14px 10px; }
      .plg-title { font-size:13px; font-weight:700; letter-spacing:.1em; text-transform:uppercase; color:var(--text-secondary,#a0a0a0); }
      .plg-btn { padding:5px 11px; border:1px solid var(--border,#2a2a2f); border-radius:7px; background:transparent;
        color:var(--text-secondary,#a0a0a0); font:inherit; font-size:11px; cursor:pointer; }
      .plg-btn:hover { color:var(--text-primary,#e0e0e0); }
      .plg-btn.primary { background:var(--plg-accent); border-color:var(--plg-accent); color:#0a0a0f; font-weight:600; }
      .plg-btn.danger { color:#ef4444; }
      .plg-group { padding:12px 14px 4px; font-family:var(--font-mono,monospace); font-size:9px; letter-spacing:.12em;
        text-transform:uppercase; color:#6a6a74; }
      .plg-row { display:flex; flex-direction:column; gap:3px; padding:10px 14px; border-top:1px solid #1c1c22; cursor:pointer; }
      .plg-row:hover { background:rgba(255,255,255,.02); }
      .plg-row.on { background:#191713; border-left:2px solid var(--plg-accent); }
      .plg-row-top { display:flex; align-items:center; gap:8px; }
      .plg-name { font-size:13px; font-weight:600; color:var(--text-primary,#e0e0e0); }
      .plg-desc { font-size:11px; color:#7a7a84; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
      .plg-dot { width:6px; height:6px; flex:0 0 auto; border-radius:50%; background:#3a3a42; }
      .plg-dot.on { background:#4ade80; } .plg-dot.blocked { background:#f5b840; }
      .plg-detail { display:flex; flex:1 1 auto; flex-direction:column; min-width:0; min-height:0; padding:18px 22px; gap:13px; overflow-y:auto; }
      .plg-detail-head { display:flex; align-items:center; gap:10px; }
      .plg-detail-name { font-size:19px; font-weight:620; color:var(--text-primary,#e0e0e0); }
      .plg-detail-desc { color:var(--text-secondary,#a0a0a0); font-size:12px; line-height:1.6; }
      .plg-note { padding:9px 11px; border:1px solid #3a3220; border-radius:8px; background:rgba(245,184,64,.06);
        color:#d8c79a; font-size:11px; line-height:1.55; }
      .plg-blocked { color:#f5b840; font-size:11px; }
      .plg-field { display:flex; flex-direction:column; gap:5px; }
      .plg-label { color:#7a7a84; font-size:10px; font-weight:700; letter-spacing:.08em; text-transform:uppercase; }
      .plg-input { width:100%; padding:8px 10px; border:1px solid var(--border,#2a2a2f); border-radius:7px;
        color:var(--text-primary,#e0e0e0); background:var(--bg-secondary,#141419);
        font-family:var(--font-mono,monospace); font-size:12px; }
      .plg-actions { display:flex; align-items:center; gap:8px; padding-top:4px; }
      .plg-empty { margin:auto; padding:30px; text-align:center; color:var(--text-secondary,#a0a0a0); font-size:13px; }
      .plg-link { color:var(--plg-accent); font-size:11px; text-decoration:none; }
    `;
    document.head.appendChild(style);
  }

  function createPluginsPanel(tabId, parent) {
    ensureStyles();
    const pane = document.createElement('section');
    pane.className = 'plg';
    parent.appendChild(pane);

    let plugins = [];
    let selected = null;

    async function load(keepId) {
      try {
        plugins = (await invoke('plugin_catalog')) || [];
      } catch (error) {
        console.error('[plugins] catalog failed:', error);
        plugins = [];
      }
      const want = keepId || (selected && selected.id);
      selected = plugins.find((plugin) => plugin.id === want) || plugins[0] || null;
      render();
    }

    // Mirrors the Rust blocker() so the reason is visible BEFORE the toggle is
    // touched. The backend still refuses; this is the explanation, not the gate.
    function blocker(plugin) {
      if (plugin.transport === 'http' && !String(plugin.url || '').trim()) return 'needs its URL';
      if (plugin.transport === 'stdio' && !String(plugin.command || '').trim()) return 'needs a command';
      for (const key of plugin.required_env || []) {
        if (!String((plugin.env || {})[key] || '').trim()) return `needs ${key}`;
      }
      return null;
    }

    function rowMarkup(plugin) {
      const state = plugin.enabled ? 'on' : (blocker(plugin) ? 'blocked' : '');
      return `<div class="plg-row ${selected && plugin.id === selected.id ? 'on' : ''}" data-plugin="${esc(plugin.id)}">
        <span class="plg-row-top"><span class="plg-dot ${state}"></span><span class="plg-name">${esc(plugin.name)}</span></span>
        <span class="plg-desc">${esc(plugin.description || plugin.id)}</span></div>`;
    }

    function listMarkup() {
      const groups = [];
      const seen = new Set();
      for (const category of CATEGORY_ORDER.concat(
        plugins.map((plugin) => plugin.category).filter((category) => category && !CATEGORY_ORDER.includes(category)),
      )) {
        if (seen.has(category)) continue;
        seen.add(category);
        const group = plugins.filter((plugin) => (plugin.category || 'Other') === category);
        if (!group.length) continue;
        groups.push(`<div class="plg-group">${esc(category)} · ${group.length}</div>${group.map(rowMarkup).join('')}`);
      }
      const rest = plugins.filter((plugin) => !seen.has(plugin.category || 'Other'));
      if (rest.length) groups.push(`<div class="plg-group">Other · ${rest.length}</div>${rest.map(rowMarkup).join('')}`);
      return groups.join('');
    }

    function fieldMarkup(label, key, value, placeholder) {
      return `<div class="plg-field"><span class="plg-label">${esc(label)}</span>
        <input class="plg-input" data-field="${esc(key)}" value="${esc(value || '')}" placeholder="${esc(placeholder || '')}" spellcheck="false"></div>`;
    }

    function detailMarkup() {
      if (!selected) return '<div class="plg-empty">No plugins in the library.</div>';
      const reason = blocker(selected);
      const transport = selected.transport === 'http'
        ? fieldMarkup('Endpoint', 'url', selected.url, 'https://…/mcp')
          + Object.keys(selected.headers || {}).map((key) => fieldMarkup(`Header · ${key}`, `header:${key}`, selected.headers[key], 'Bearer …')).join('')
        : fieldMarkup('Command', 'command', selected.command, 'npx')
          + fieldMarkup('Arguments', 'args', (selected.args || []).join(' '), '-y package-name');
      const env = Object.keys(selected.env || {}).sort().map((key) => fieldMarkup(
        `${key}${(selected.required_env || []).includes(key) ? ' · required' : ''}`, `env:${key}`, selected.env[key], '',
      )).join('');
      return `<div class="plg-detail">
        <div class="plg-detail-head"><span class="plg-detail-name">${esc(selected.name)}</span>
          <span class="plg-dot ${selected.enabled ? 'on' : (reason ? 'blocked' : '')}"></span>
          <span style="flex:1"></span>
          ${selected.docs_url ? `<a class="plg-link" href="${esc(selected.docs_url)}" target="_blank" rel="noreferrer">docs</a>` : ''}
          ${selected.seeded ? '' : '<button class="plg-btn danger" data-delete>Delete</button>'}</div>
        <div class="plg-detail-desc">${esc(selected.description)}</div>
        ${selected.note ? `<div class="plg-note">${esc(selected.note)}</div>` : ''}
        ${transport}
        ${env}
        <div class="plg-actions">
          <button class="plg-btn primary" data-save>Save</button>
          <button class="plg-btn" data-toggle>${selected.enabled ? 'Disable' : 'Enable'}</button>
          ${reason && !selected.enabled ? `<span class="plg-blocked">${esc(reason)}</span>` : ''}
          <span style="flex:1"></span>
          <button class="plg-btn" data-env-add>Add variable</button>
        </div>
        <div class="plg-detail-desc">Enabled plugins are handed to the coding harness at launch: claude via <code>--mcp-config</code>, codex via <code>-c mcp_servers…</code>. Chat turns do not use plugins.</div>
      </div>`;
    }

    function collect() {
      const next = JSON.parse(JSON.stringify(selected));
      pane.querySelectorAll('[data-field]').forEach((input) => {
        const key = input.dataset.field;
        const value = input.value;
        if (key === 'args') next.args = value.trim() ? value.trim().split(/\s+/) : [];
        else if (key.startsWith('env:')) next.env[key.slice(4)] = value;
        else if (key.startsWith('header:')) next.headers[key.slice(7)] = value;
        else next[key] = value;
      });
      return next;
    }

    async function save(mutate) {
      const next = mutate ? mutate(collect()) : collect();
      try {
        const saved = await invoke('plugin_save', { plugin: next });
        await load(saved.id);
      } catch (error) {
        alert(String(error));
      }
    }

    function render() {
      pane.innerHTML = `<div class="plg-list">
          <div class="plg-head"><span class="plg-title">Plugins</span>
            <button class="plg-btn primary" data-new>Add</button></div>
          ${listMarkup()}
        </div>${detailMarkup()}`;
      wire();
    }

    function wire() {
      pane.querySelectorAll('[data-plugin]').forEach((row) => {
        row.onclick = () => load(row.dataset.plugin);
      });
      const saveButton = pane.querySelector('[data-save]');
      if (saveButton) saveButton.onclick = () => save();
      const toggle = pane.querySelector('[data-toggle]');
      if (toggle) toggle.onclick = () => save((next) => ({ ...next, enabled: !selected.enabled }));
      const addVariable = pane.querySelector('[data-env-add]');
      if (addVariable) addVariable.onclick = async () => {
        const key = await ask('Environment variable name', '', 'Add');
        if (!key || !key.trim()) return;
        await save((next) => ({ ...next, env: { ...next.env, [key.trim()]: '' } }));
      };
      const remove = pane.querySelector('[data-delete]');
      if (remove) remove.onclick = async () => {
        if (!await sure(`Remove "${selected.name}" from the library?`, 'Remove')) return;
        try { await invoke('plugin_delete', { id: selected.id }); selected = null; await load(); }
        catch (error) { alert(String(error)); }
      };
      const create = pane.querySelector('[data-new]');
      if (create) create.onclick = async () => {
        const name = await ask('Name of the MCP server to add', '', 'Add');
        if (!name || !name.trim()) return;
        const id = name.trim().toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');
        try {
          const saved = await invoke('plugin_save', { plugin: {
            id, name: name.trim(), description: 'Added here. Fill in the command or endpoint below.',
            transport: 'stdio', command: 'npx', args: [], url: '', headers: {},
            category: 'Other', note: '', env: {}, required_env: [], enabled: false, docs_url: '', seeded: false,
          } });
          await load(saved.id);
        } catch (error) { alert(String(error)); }
      };
    }

    load();
    return { kind: 'plugins', label: `plugins-${tabId}`, pane, dispose() {} };
  }

  window.xnautCreatePluginsPanel = createPluginsPanel;
  window.xnautOpenPlugins = () => {
    if (window.xnautHomeContext) window.xnautHomeContext();
    return window.xnautAttachSingletonPanelTab('Plugins', 'xnautCreatePluginsPanel', {});
  };
})();
