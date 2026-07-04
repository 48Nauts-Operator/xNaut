// Vault pane - Obsidian-style markdown vault inside xNaut.
// Layout: [ rail: Notes/Tags/Search | librarian chat | divider | note doc ].
// The Rust vault.rs module owns the index; this pane is a thin client.
(function () {
  'use strict';

  const invoke = (...a) => window.__TAURI__.core.invoke(...a);

  const VAULTS = ['work', 'personal'];
  let activePane = null;

  function injectStyles() {
    if (document.getElementById('vault-pane-styles')) return;
    const st = document.createElement('style');
    st.id = 'vault-pane-styles';
    st.textContent = `
.vp-rail { display:flex; flex-direction:column; flex:0 0 250px; min-width:0; min-height:0; overflow:hidden; border-right:1px solid var(--border-color,#333); background:var(--editor-surface,#1b1d23); font-size:13px; }
.vp-rail-head { display:flex; align-items:center; gap:6px; padding:8px; border-bottom:1px solid var(--border-color,#333); }
.vp-rail-head select { flex:1; background:var(--input-bg,rgba(255,255,255,.06)); color:inherit; border:1px solid var(--border-color,#333); border-radius:6px; padding:3px 6px; font:inherit; }
.vp-icon-btn { background:transparent; border:none; color:var(--text-secondary,#aaa); cursor:pointer; font-size:14px; padding:2px 6px; border-radius:5px; }
.vp-icon-btn:hover { background:var(--hover-bg,rgba(255,255,255,.08)); color:#fff; }
.vp-tabs { display:flex; border-bottom:1px solid var(--border-color,#333); }
.vp-tabs button { flex:1; background:transparent; border:none; color:var(--text-muted,#888); font:inherit; font-size:11px; padding:6px 0; cursor:pointer; border-bottom:2px solid transparent; }
.vp-tabs button[data-active="1"] { color:#fff; border-bottom-color:var(--accent,#4f8cff); }
.vp-body { flex:1 1 0%; min-height:0; overflow-y:auto; padding:6px; }
.vp-body details { margin:1px 0; }
.vp-body summary { cursor:pointer; padding:3px 6px; border-radius:5px; color:var(--text-secondary,#aaa); list-style:none; }
.vp-body summary::before { content:'> '; font-size:9px; }
.vp-body details[open] > summary::before { content:'v '; }
.vp-body summary:hover { background:var(--hover-bg,rgba(255,255,255,.06)); }
.vp-tree-children { padding-left:14px; }
.vp-note-row { padding:3px 6px; border-radius:5px; cursor:pointer; color:var(--text,#d7dae0); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
.vp-note-row:hover { background:var(--hover-bg,rgba(255,255,255,.08)); }
.vp-note-row[data-active="1"] { background:var(--active-bg,rgba(79,140,255,.18)); }
.vp-status { padding:4px 8px; font-size:11px; color:var(--text-muted,#777); border-top:1px solid var(--border-color,#333); display:flex; gap:8px; align-items:center; }
.vault-wikilink { color:var(--accent,#4f8cff); cursor:pointer; text-decoration:none; border-bottom:1px dashed var(--accent,#4f8cff); }
.vault-tagchip { color:var(--agent-thinking,#4dffd0); cursor:pointer; }
`;
    document.head.appendChild(st);
  }

  function buildTree(dirs, notes) {
    const rootNode = { dirs: {}, notes: [] };
    const dirNode = (rel) => rel.split('/').reduce((n, seg) => (n.dirs[seg] = n.dirs[seg] || { dirs: {}, notes: [] }), rootNode);
    (dirs || []).forEach((d) => dirNode(d));
    (notes || []).forEach((n) => {
      const i = n.rel.lastIndexOf('/');
      (i < 0 ? rootNode : dirNode(n.rel.slice(0, i))).notes.push(n);
    });
    return rootNode;
  }

  function renderTree(node, el, openNote, activeRel) {
    const names = Object.keys(node.dirs).sort((a, b) => (a === '_inbox' ? -1 : b === '_inbox' ? 1 : a.localeCompare(b)));
    names.forEach((name) => {
      const det = document.createElement('details');
      if (name === '_inbox') det.open = true;
      const sum = document.createElement('summary');
      sum.textContent = name;
      det.appendChild(sum);
      const inner = document.createElement('div');
      inner.className = 'vp-tree-children';
      renderTree(node.dirs[name], inner, openNote, activeRel);
      det.appendChild(inner);
      el.appendChild(det);
    });
    node.notes.sort((a, b) => a.title.localeCompare(b.title)).forEach((n) => {
      const row = document.createElement('div');
      row.className = 'vp-note-row';
      row.textContent = n.title;
      row.title = n.rel;
      row.dataset.rel = n.rel;
      if (n.rel === activeRel) row.dataset.active = '1';
      row.onclick = () => openNote(n.rel);
      el.appendChild(row);
    });
  }

  async function createVaultPane(tabId, container, opts) {
    opts = opts || {};
    injectStyles();
    const root = await invoke('vault_init');
    let vault = localStorage.getItem('xnaut-vault:last') || 'work';
    if (!VAULTS.includes(vault)) vault = 'work';

    const row = document.createElement('div');
    row.style.cssText = 'display:flex; flex:1 1 0%; width:100%; height:100%; min-width:0; min-height:0; overflow:hidden;';
    const rail = document.createElement('div');
    rail.className = 'vp-rail';
    const chatHost = document.createElement('div');
    chatHost.style.cssText = 'display:flex; flex:1 1 0%; min-width:0; min-height:0; overflow:hidden;';
    const divider = document.createElement('div');
    divider.style.cssText = 'width:4px; cursor:col-resize; background:var(--border-color,#333); flex-shrink:0;';
    const doc = document.createElement('div');
    doc.style.cssText = 'display:flex; flex-direction:column; flex:1 1 0%; min-width:0; min-height:0; overflow:hidden; background:var(--editor-surface,#1b1d23);';

    rail.innerHTML = `
      <div class="vp-rail-head">
        <select class="vp-vault">${VAULTS.map((v) => `<option value="${v}"${v === vault ? ' selected' : ''}>${v}</option>`).join('')}</select>
        <button class="vp-icon-btn vp-new" title="New note">+</button>
        <button class="vp-icon-btn vp-refresh" title="Refresh">R</button>
      </div>
      <div class="vp-tabs">
        <button data-tab="notes" data-active="1">Notes</button>
        <button data-tab="tags">Tags</button>
        <button data-tab="search">Search</button>
      </div>
      <div class="vp-body"></div>
      <div class="vp-status"><span class="vp-count"></span><span class="vp-sync" style="margin-left:auto"></span></div>`;
    const body = rail.querySelector('.vp-body');
    const countEl = rail.querySelector('.vp-count');

    const bar = document.createElement('div');
    bar.style.cssText = 'display:flex; align-items:center; gap:8px; padding:8px 12px; border-bottom:1px solid var(--border-color,#333); font-size:12px; color:var(--text-muted,#8a8f98); flex-shrink:0;';
    const title = document.createElement('span');
    title.textContent = 'No note open';
    const toggle = document.createElement('div');
    toggle.className = 'plan-doc-toggle';
    toggle.innerHTML = '<button data-mode="preview" data-active="1">Preview</button><button data-mode="edit">Edit</button>';
    const status = document.createElement('span');
    status.style.cssText = 'font-size:11px; opacity:.7; min-width:48px; text-align:right;';
    bar.appendChild(title);
    bar.appendChild(toggle);
    bar.appendChild(status);

    const view = document.createElement('div');
    view.className = 'plan-doc-view';
    const ta = document.createElement('textarea');
    ta.className = 'plan-doc-edit';
    ta.spellcheck = false;
    ta.placeholder = 'Open a note from the tree, or ask the librarian...';
    ta.style.cssText = 'flex:1 1 0%; width:100%; min-height:0; box-sizing:border-box; resize:none; border:none; outline:none; padding:14px 16px; background:transparent; color:var(--text,#d7dae0); font-family:"SF Mono",Menlo,monospace; font-size:13px; line-height:1.55; display:none;';
    doc.appendChild(bar);
    doc.appendChild(view);
    doc.appendChild(ta);

    row.appendChild(rail);
    row.appendChild(chatHost);
    row.appendChild(divider);
    row.appendChild(doc);
    container.appendChild(row);

    let currentRel = null;
    let mode = 'preview';
    let saveTimer = null;
    const entry = {
      kind: 'vault', pane: row, tabId, root,
      vault: () => vault,
      openNote: null,
      refresh: null,
      getDoc: () => ta.value,
      currentRel: () => currentRel,
      viewEl: view, taEl: ta, bodyEl: body, railEl: rail, chatHostEl: chatHost,
      statusEl: status, notes: [],
      renderView: null,
      mode: () => mode,
    };

    const renderView = () => {
      window.xnautMarkdown.renderInto(view, ta.value || '_Empty note._');
      if (entry.decorate) entry.decorate();
    };
    entry.renderView = renderView;

    const setMode = (m) => {
      mode = m;
      if (m === 'preview') {
        renderView();
        view.style.display = 'block';
        ta.style.display = 'none';
      } else {
        view.style.display = 'none';
        ta.style.display = 'block';
        ta.focus();
      }
      toggle.querySelectorAll('button').forEach((b) => { b.dataset.active = b.dataset.mode === m ? '1' : '0'; });
    };
    toggle.querySelectorAll('button').forEach((b) => { b.onclick = () => setMode(b.dataset.mode); });

    divider.addEventListener('mousedown', (e) => {
      e.preventDefault();
      const startX = e.clientX;
      const leftW = chatHost.getBoundingClientRect().width;
      const total = row.getBoundingClientRect().width;
      const onMove = (ev) => {
        const w = Math.max(240, Math.min(total - 240, leftW + (ev.clientX - startX)));
        chatHost.style.flex = `0 0 ${w}px`;
        doc.style.flex = '1 1 0%';
      };
      const onUp = () => {
        document.removeEventListener('mousemove', onMove);
        document.removeEventListener('mouseup', onUp);
      };
      document.addEventListener('mousemove', onMove);
      document.addEventListener('mouseup', onUp);
    });

    const persist = async () => {
      if (!currentRel) return;
      try {
        await invoke('vault_note_write', { vault, rel: currentRel, content: ta.value });
        status.textContent = 'saved';
      } catch (e) {
        status.textContent = 'save failed';
        console.error('[vault] save failed', e);
      }
    };
    const flushSave = () => {
      if (saveTimer) {
        clearTimeout(saveTimer);
        saveTimer = null;
        return persist();
      }
      return Promise.resolve();
    };
    ta.addEventListener('input', () => {
      status.textContent = 'editing...';
      clearTimeout(saveTimer);
      saveTimer = setTimeout(() => { saveTimer = null; persist(); }, 600);
    });

    async function openNote(rel) {
      await flushSave();
      try {
        ta.value = await invoke('vault_note_read', { vault, rel });
      } catch (e) {
        console.error('[vault] open failed', e);
        return;
      }
      currentRel = rel;
      title.textContent = rel;
      status.textContent = '';
      if (mode === 'preview') renderView();
      body.querySelectorAll('.vp-note-row').forEach((r) => { r.dataset.active = r.dataset.rel === rel ? '1' : '0'; });
    }
    entry.openNote = openNote;

    async function refresh() {
      const tree = await invoke('vault_tree', { vault });
      body.innerHTML = '';
      renderTree(buildTree(tree.dirs, tree.notes), body, openNote, currentRel);
      countEl.textContent = `${tree.notes.length} notes`;
      entry.notes = tree.notes;
    }
    entry.refresh = refresh;

    async function switchVault(v) {
      await flushSave();
      try { await invoke('vault_close', { vault }); } catch (_) { /* not open */ }
      vault = v;
      localStorage.setItem('xnaut-vault:last', v);
      currentRel = null;
      ta.value = '';
      title.textContent = 'No note open';
      if (mode === 'preview') renderView();
      await invoke('vault_open', { vault });
      await refresh();
    }

    rail.querySelector('.vp-vault').onchange = (e) => switchVault(e.target.value).catch((err) => console.error('[vault] switch failed', err));
    rail.querySelector('.vp-new').onclick = async () => {
      const name = window.prompt('Note name (folders ok: project/idea):');
      if (!name) return;
      const rel = name.replace(/\.md$/i, '') + '.md';
      try {
        await invoke('vault_note_create', { vault, rel, content: null });
        await refresh();
        await openNote(rel);
      } catch (e) {
        window.alert('Create failed: ' + e);
      }
    };
    rail.querySelector('.vp-refresh').onclick = () => refresh().catch((e) => console.error('[vault] refresh failed', e));

    await invoke('vault_open', { vault });
    await refresh();
    setMode('preview');

    activePane = entry;
    return entry;
  }

  window.xnautVaultOpenNote = (absPath) => {
    if (!activePane || !absPath) return false;
    const v = activePane.vault();
    const prefix = activePane.root + '/' + v + '/';
    if (!absPath.startsWith(prefix)) return false;
    activePane.openNote(absPath.slice(prefix.length));
    return true;
  };

  window.xnautCreateVaultPane = createVaultPane;
})();
