// Skill library (XNAUT-158) — a sub-surface of Agent Space.
//
// Skills are markdown: a folder with a SKILL.md carrying YAML frontmatter.
// What you add here is exactly what appears in an agent's Capabilities tab,
// so the library is the single place a skill enters the system.
//
// Roots are read in precedence order by the backend (project, user, Claude's
// own ~/.claude/skills, bundled). Only user skills are editable here; the
// rest are shown with their source so nobody edits a file an update will
// overwrite.
(function () {
  'use strict';

  const invoke = (...args) => window.__TAURI__.core.invoke(...args);
  const esc = (value) => String(value == null ? '' : value).replace(/[&<>"']/g, (c) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  }[c]));

  // window.prompt/confirm can resolve to null in this webview without ever
  // rendering, which makes a button look dead. Agent Space exports in-app
  // replacements; fall back only if it somehow has not loaded.
  const ask = (message, value, label) => (window.xnautPromptDialog
    ? window.xnautPromptDialog(message, value, label)
    : Promise.resolve(prompt(message, value)));
  const sure = (message, label) => (window.xnautConfirmDialog
    ? window.xnautConfirmDialog(message, label)
    : Promise.resolve(confirm(message)));

  const SOURCE_LABEL = {
    project: 'project', user: 'yours', claude: 'claude code', codex: 'codex', bundled: 'built in',
  };
  // Grouped so a library of fifty is readable: starred first, then yours,
  // then each harness under its own heading.
  const GROUP_ORDER = ['user', 'project', 'claude', 'codex', 'bundled'];
  const GROUP_TITLE = {
    user: 'Yours', project: 'This project', claude: 'Claude Code', codex: 'Codex', bundled: 'Built in',
  };

  function ensureStyles() {
    if (document.getElementById('skills-panel-styles')) return;
    const style = document.createElement('style');
    style.id = 'skills-panel-styles';
    style.textContent = `
      .skl { --skl-accent:#f5b840; display:flex; flex:1 1 auto; min-width:0; min-height:0; overflow:hidden;
        color:var(--text-primary,#e0e0e0); background:var(--bg-primary,#0a0a0f);
        font-family:var(--font-ui,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif); }
      .skl * { box-sizing:border-box; }
      .skl-list { display:flex; flex:0 0 300px; flex-direction:column; min-height:0; overflow-y:auto;
        border-right:1px solid var(--border,#2a2a2f); background:var(--bg-secondary,#141419); }
      .skl-head { display:flex; align-items:center; justify-content:space-between; gap:8px; padding:14px 14px 10px; }
      .skl-title { font-size:13px; font-weight:700; letter-spacing:.1em; text-transform:uppercase; color:var(--text-secondary,#a0a0a0); }
      .skl-btn { padding:5px 11px; border:1px solid var(--border,#2a2a2f); border-radius:7px; background:transparent;
        color:var(--text-secondary,#a0a0a0); font:inherit; font-size:11px; cursor:pointer; }
      .skl-btn:hover { color:var(--text-primary,#e0e0e0); }
      .skl-btn.primary { background:var(--skl-accent); border-color:var(--skl-accent); color:#0a0a0f; font-weight:600; }
      .skl-btn.danger { color:#ef4444; }
      .skl-row { display:flex; flex-direction:column; gap:3px; padding:10px 14px; border-top:1px solid #1c1c22; cursor:pointer; }
      .skl-row:hover { background:rgba(255,255,255,.02); }
      .skl-row.on { background:#191713; border-left:2px solid var(--skl-accent); }
      .skl-row-top { display:flex; align-items:center; gap:8px; }
      .skl-name { font-size:13px; font-weight:600; color:var(--text-primary,#e0e0e0); }
      .skl-src { font-size:9px; font-weight:700; letter-spacing:.04em; text-transform:uppercase; padding:1px 6px;
        border:1px solid var(--border,#2a2a2f); border-radius:999px; color:var(--text-secondary,#7a7a84); }
      .skl-group { padding:12px 14px 4px; font-family:var(--font-mono,monospace); font-size:9px; letter-spacing:.12em;
        text-transform:uppercase; color:#6a6a74; }
      .skl-star { border:0; background:transparent; color:var(--skl-accent); font-size:13px; cursor:pointer; padding:0; line-height:1; }
      .skl-star:not(.on) { color:#4a4a52; }
      .skl-desc { font-size:11px; color:#7a7a84; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
      .skl-detail { display:flex; flex:1 1 auto; flex-direction:column; min-width:0; min-height:0; padding:18px 22px; gap:12px; }
      .skl-detail-head { display:flex; align-items:center; gap:10px; }
      .skl-detail-name { font-size:19px; font-weight:620; color:var(--text-primary,#e0e0e0); }
      .skl-path { font-family:var(--font-mono,monospace); font-size:10px; color:#5a5a62; }
      .skl-editor { flex:1 1 auto; min-height:0; width:100%; padding:12px 14px; resize:none;
        border:1px solid var(--border,#2a2a2f); border-radius:9px; background:var(--bg-secondary,#141419);
        color:var(--text-primary,#e0e0e0); font-family:var(--font-mono,monospace); font-size:12px; line-height:1.55; }
      .skl-editor[readonly] { color:var(--text-secondary,#a0a0a0); }
      .skl-actions { display:flex; align-items:center; gap:8px; }
      .skl-note { font-size:11px; color:#7a7a84; }
      .skl-empty { margin:auto; padding:30px; text-align:center; color:var(--text-secondary,#a0a0a0); font-size:13px; }
    `;
    document.head.appendChild(style);
  }

  function createSkillsPanel(tabId, parent) {
    ensureStyles();
    const pane = document.createElement('section');
    pane.className = 'skl';
    parent.appendChild(pane);

    let skills = [];
    let selected = null;
    let body = '';

    async function load(keepName) {
      try {
        skills = (await invoke('skill_catalog', { project: null })) || [];
      } catch (error) {
        console.error('[skills] catalog failed:', error);
        skills = [];
      }
      const want = keepName || (selected && selected.name);
      selected = skills.find((skill) => skill.name === want) || skills[0] || null;
      body = '';
      if (selected) {
        try { body = await invoke('skill_read', { path: selected.path }); }
        catch (_) { body = ''; }
      }
      render();
    }

    function rowMarkup(skill) {
      return `<div class="skl-row ${selected && skill.name === selected.name ? 'on' : ''}" data-skill="${esc(skill.name)}">
        <span class="skl-row-top">
          <button class="skl-star ${skill.favourite ? 'on' : ''}" data-star="${esc(skill.name)}" title="${skill.favourite ? 'Unstar' : 'Star'}">${skill.favourite ? '★' : '☆'}</button>
          <span class="skl-name">${esc(skill.name)}</span>
          <span class="skl-src">${esc(SOURCE_LABEL[skill.source] || skill.source)}</span></span>
        <span class="skl-desc">${esc(skill.description || 'No description in the frontmatter.')}</span>
      </div>`;
    }

    function listMarkup() {
      const starred = skills.filter((skill) => skill.favourite);
      const chunks = [];
      if (starred.length) {
        chunks.push(`<div class="skl-group">Starred</div>${starred.map(rowMarkup).join('')}`);
      }
      for (const source of GROUP_ORDER) {
        const group = skills.filter((skill) => skill.source === source && !skill.favourite);
        if (!group.length) continue;
        chunks.push(`<div class="skl-group">${GROUP_TITLE[source]} · ${group.length}</div>${group.map(rowMarkup).join('')}`);
      }
      return chunks.join('');
    }

    function render() {
      const detail = selected ? `<div class="skl-detail">
          <div class="skl-detail-head">
            <span class="skl-detail-name">${esc(selected.name)}</span>
            <span class="skl-src">${esc(SOURCE_LABEL[selected.source] || selected.source)}</span>
            <span style="flex:1"></span>
            ${selected.editable ? '<button class="skl-btn danger" data-delete>Delete</button>' : ''}
          </div>
          <div class="skl-path">${esc(selected.path)}</div>
          <textarea class="skl-editor" data-editor ${selected.editable ? '' : 'readonly'}>${esc(body)}</textarea>
          <div class="skl-actions">
            ${selected.editable
              ? '<button class="skl-btn primary" data-save>Save</button><span class="skl-note">Agents load a skill on demand; enable it per agent in Capabilities.</span>'
              : `<span class="skl-note">Read-only: this one comes from ${esc(SOURCE_LABEL[selected.source] || selected.source)}. Duplicate it to make it yours.</span><button class="skl-btn" data-duplicate>Duplicate to mine</button>`}
          </div>
        </div>`
        : '<div class="skl-empty">No skills yet. Create one, or import a folder that has a SKILL.md.</div>';

      pane.innerHTML = `<div class="skl-list">
          <div class="skl-head"><span class="skl-title">Skills</span>
            <span style="display:flex;gap:6px"><button class="skl-btn" data-import>Import</button>
            <button class="skl-btn primary" data-new>New</button></span></div>
          ${listMarkup()}
        </div>${detail}`;
      wire();
    }

    function wire() {
      pane.querySelectorAll('[data-skill]').forEach((row) => {
        row.onclick = () => load(row.dataset.skill);
      });
      pane.querySelectorAll('[data-star]').forEach((star) => {
        star.onclick = async (event) => {
          event.stopPropagation();
          const name = star.dataset.star;
          const current = skills.find((skill) => skill.name === name);
          try { await invoke('skill_favourite', { name, favourite: !(current && current.favourite) }); }
          catch (error) { console.error('[skills] star failed:', error); }
          await load(name);
        };
      });
      const create = pane.querySelector('[data-new]');
      if (create) create.onclick = async () => {
        const name = await ask('Skill name (letters, numbers, - and _)', '', 'Create');
        if (!name) return;
        try { const made = await invoke('skill_write', { name, contents: null }); await load(made.name); }
        catch (error) { alert(String(error)); }
      };
      const importer = pane.querySelector('[data-import]');
      if (importer) importer.onclick = async () => {
        const source = await ask('Path to a skill folder (with SKILL.md) or a markdown file', '', 'Import');
        if (!source) return;
        try { const made = await invoke('skill_import', { sourcePath: source, name: null }); await load(made.name); }
        catch (error) { alert(String(error)); }
      };
      const save = pane.querySelector('[data-save]');
      if (save) save.onclick = async () => {
        const editor = pane.querySelector('[data-editor]');
        try { await invoke('skill_write', { name: selected.name, contents: editor.value }); await load(selected.name); }
        catch (error) { alert(String(error)); }
      };
      const duplicate = pane.querySelector('[data-duplicate]');
      if (duplicate) duplicate.onclick = async () => {
        try { const made = await invoke('skill_import', { sourcePath: selected.path, name: selected.name }); await load(made.name); }
        catch (error) { alert(String(error)); }
      };
      const remove = pane.querySelector('[data-delete]');
      if (remove) remove.onclick = async () => {
        if (!await sure(`Delete the skill "${esc(selected.name)}"?`, 'Delete')) return;
        try { await invoke('skill_delete', { name: selected.name }); selected = null; await load(); }
        catch (error) { alert(String(error)); }
      };
    }

    load();
    return { kind: 'skills', label: `skills-${tabId}`, pane, dispose() {} };
  }

  window.xnautCreateSkillsPanel = createSkillsPanel;
  window.xnautOpenSkills = () => {
    if (window.xnautHomeContext) window.xnautHomeContext();
    return window.xnautAttachSingletonPanelTab('Skills', 'xnautCreateSkillsPanel', {});
  };
})();
