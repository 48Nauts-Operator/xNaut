// Vault pane - Obsidian-style markdown vault inside xNaut.
// Layout: [ rail: Notes/Tags/Search | librarian chat | divider | note doc ].
// The Rust vault.rs module owns the index; this pane is a thin client.
(function () {
  'use strict';

  const invoke = (...a) => window.__TAURI__.core.invoke(...a);
  const listen = (...a) => window.__TAURI__.event.listen(...a);

  const VAULTS = ['work', 'personal'];
  const MODE_TOGGLE_HTML = `
    <button data-mode="preview" data-active="1" title="Preview" aria-label="Preview">
      <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M1.8 8s2.3-4 6.2-4 6.2 4 6.2 4-2.3 4-6.2 4-6.2-4-6.2-4z"/><circle cx="8" cy="8" r="2"/></svg>
    </button>
    <button data-mode="edit" title="Edit" aria-label="Edit">
      <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M3 13l1-3 6.8-6.8a1.4 1.4 0 0 1 2 2L6 12z"/><path d="M9.8 4.2l2 2"/></svg>
    </button>`;
  let activePane = null;

  function showLibrarianConversationsPane() {
    if (typeof window.xnautRightPaneShowLibrarianConversations === 'function') {
      window.xnautRightPaneShowLibrarianConversations();
    }
  }

  function stripPreviewFrontmatter(markdown) {
    const text = String(markdown || '');
    return text.replace(/^---\s*\n[\s\S]*?\n---\s*(?:\n|$)/, '');
  }

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
.vp-icon-btn svg { width:14px; height:14px; display:block; }
.vp-tabs { display:flex; border-bottom:1px solid var(--border-color,#333); }
.vp-tabs button { flex:1; background:transparent; border:none; color:var(--text-muted,#888); font:inherit; font-size:11px; padding:6px 0; cursor:pointer; border-bottom:2px solid transparent; }
.vp-tabs button[data-active="1"] { color:#fff; border-bottom-color:var(--accent,#4f8cff); }
.vp-create-panel { display:none; padding:8px; border-bottom:1px solid var(--border-color,#333); background:rgba(255,255,255,.03); }
.vp-create-panel[data-open="1"] { display:block; }
.vp-create-hint { margin-bottom:6px; color:var(--text-muted,#888); font-size:11px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.vp-create-panel input, .vp-create-panel select { width:100%; box-sizing:border-box; margin-bottom:6px; background:var(--input-bg,rgba(255,255,255,.06)); color:inherit; border:1px solid var(--border-color,#333); border-radius:6px; padding:5px 7px; font:inherit; }
.vp-create-panel select[hidden] { display:none; }
.vp-create-actions { display:flex; flex-wrap:wrap; gap:6px; }
.vp-create-actions button { flex:1 1 calc(50% - 3px); min-width:0; background:rgba(255,255,255,.06); color:var(--text,#d7dae0); border:1px solid var(--border-color,#333); border-radius:6px; padding:4px 6px; font:inherit; font-size:11px; cursor:pointer; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
.vp-create-actions button:hover { background:rgba(255,255,255,.1); }
.vp-body { flex:1 1 0%; min-height:0; overflow-y:auto; padding:6px; }
.vp-body details { margin:1px 0; }
.vp-body summary { cursor:pointer; padding:3px 6px; border-radius:5px; color:var(--text-secondary,#aaa); list-style:none; }
.vp-body summary::before { content:'> '; font-size:9px; }
.vp-body details[open] > summary::before { content:'v '; }
.vp-body summary:hover { background:var(--hover-bg,rgba(255,255,255,.06)); }
.vp-tree-children { padding-left:14px; }
.vp-tree-label { display:flex; align-items:center; gap:4px; }
.vp-row-main { flex:1 1 auto; min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.vp-menu-btn { flex:0 0 auto; opacity:.35; background:transparent; border:none; color:inherit; cursor:pointer; border-radius:4px; padding:0 4px; line-height:1.2; }
.vp-tree-label:hover .vp-menu-btn, .vp-menu-btn:focus { opacity:1; background:rgba(255,255,255,.08); }
.vp-drop-target { outline:1px solid var(--accent,#4f8cff); background:rgba(79,140,255,.14); }
.vp-context-menu { position:fixed; z-index:9999; min-width:150px; padding:4px; border:1px solid var(--border-color,#333); border-radius:7px; background:var(--editor-surface,#202228); box-shadow:0 8px 24px rgba(0,0,0,.45); }
.vp-context-menu button { display:block; width:100%; text-align:left; background:transparent; border:none; color:var(--text,#d7dae0); font:inherit; font-size:12px; padding:6px 8px; border-radius:5px; cursor:pointer; }
.vp-context-menu button:hover { background:rgba(255,255,255,.08); }
.vp-context-menu button[data-danger="1"] { color:#ff8f8f; }
.vp-note-row { padding:3px 6px; border-radius:5px; cursor:pointer; color:var(--text,#d7dae0); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
.vp-note-row:hover { background:var(--hover-bg,rgba(255,255,255,.08)); }
.vp-note-row[data-active="1"] { background:var(--active-bg,rgba(79,140,255,.18)); }
.vp-status { padding:4px 8px; font-size:11px; color:var(--text-muted,#777); border-top:1px solid var(--border-color,#333); display:flex; gap:8px; align-items:center; }
.plan-doc-toggle { margin-left:auto; display:flex; align-items:center; gap:2px; padding:2px; border:1px solid var(--border, rgba(255,255,255,.16)); border-radius:999px; background:rgba(255,255,255,.035); }
.plan-doc-toggle button { width:28px; height:24px; display:flex; align-items:center; justify-content:center; background:transparent; border:none; border-radius:999px; color:var(--text-muted, #8a8f98); cursor:pointer; padding:0; }
.plan-doc-toggle button:hover { color:var(--text, #fff); background:rgba(255,255,255,.07); }
.plan-doc-toggle button[data-active="1"] { background:var(--accent, #4f8cff); color:var(--accent-foreground,#fff); }
.plan-doc-toggle svg { width:14px; height:14px; }
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

  function dragPayload(kind, rel) {
    return JSON.stringify({ kind, rel });
  }

  function readDragPayload(e) {
    try {
      return JSON.parse(e.dataTransfer.getData('application/xnaut-vault') || '');
    } catch (_) {
      return null;
    }
  }

  function renderTree(node, el, ctx, activeRel, prefix) {
    prefix = prefix || '';
    const names = Object.keys(node.dirs).sort((a, b) => (a === '_inbox' ? -1 : b === '_inbox' ? 1 : a.localeCompare(b)));
    names.forEach((name) => {
      const rel = prefix ? `${prefix}/${name}` : name;
      const det = document.createElement('details');
      if (name === '_inbox') det.open = true;
      const sum = document.createElement('summary');
      sum.className = 'vp-tree-label';
      sum.dataset.rel = rel;
      const label = document.createElement('span');
      label.className = 'vp-row-main';
      label.textContent = name;
      label.draggable = true;
      const menu = document.createElement('button');
      menu.className = 'vp-menu-btn';
      menu.type = 'button';
      menu.title = 'Folder actions';
      menu.textContent = '...';
      menu.onclick = (e) => {
        e.preventDefault();
        e.stopPropagation();
        ctx.showMenu('folder', rel, e.clientX, e.clientY);
      };
      sum.appendChild(label);
      sum.appendChild(menu);
      sum.oncontextmenu = (e) => {
        e.preventDefault();
        ctx.showMenu('folder', rel, e.clientX, e.clientY);
      };
      label.ondragstart = (e) => {
        e.dataTransfer.effectAllowed = 'move';
        e.dataTransfer.setData('application/xnaut-vault', dragPayload('folder', rel));
      };
      sum.ondragover = (e) => {
        e.preventDefault();
        sum.classList.add('vp-drop-target');
      };
      sum.ondragleave = () => sum.classList.remove('vp-drop-target');
      sum.ondrop = (e) => {
        e.preventDefault();
        e.stopPropagation();
        sum.classList.remove('vp-drop-target');
        ctx.onDrop(readDragPayload(e), rel);
      };
      det.appendChild(sum);
      const inner = document.createElement('div');
      inner.className = 'vp-tree-children';
      renderTree(node.dirs[name], inner, ctx, activeRel, rel);
      det.appendChild(inner);
      el.appendChild(det);
    });
    node.notes.sort((a, b) => a.title.localeCompare(b.title)).forEach((n) => {
      const row = document.createElement('div');
      row.className = 'vp-note-row vp-tree-label';
      row.title = n.rel;
      row.dataset.rel = n.rel;
      if (n.rel === activeRel) row.dataset.active = '1';
      const label = document.createElement('span');
      label.className = 'vp-row-main';
      label.textContent = n.title;
      label.draggable = true;
      const menu = document.createElement('button');
      menu.className = 'vp-menu-btn';
      menu.type = 'button';
      menu.title = 'Note actions';
      menu.textContent = '...';
      row.appendChild(label);
      row.appendChild(menu);
      row.onclick = (e) => {
        if (e.target.closest('.vp-menu-btn')) return;
        ctx.openNote(n.rel);
      };
      menu.onclick = (e) => {
        e.preventDefault();
        e.stopPropagation();
        ctx.showMenu('note', n.rel, e.clientX, e.clientY);
      };
      row.oncontextmenu = (e) => {
        e.preventDefault();
        ctx.showMenu('note', n.rel, e.clientX, e.clientY);
      };
      label.ondragstart = (e) => {
        e.dataTransfer.effectAllowed = 'move';
        e.dataTransfer.setData('application/xnaut-vault', dragPayload('note', n.rel));
      };
      el.appendChild(row);
    });
  }

  async function createVaultPane(tabId, container, opts) {
    opts = opts || {};
    injectStyles();
    const root = await invoke('vault_init');
    let vault = opts.vault || localStorage.getItem('xnaut-vault:last') || 'work';
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
        <button class="vp-icon-btn vp-refresh" title="Refresh Vault" aria-label="Refresh Vault"><svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M3.2 6.3A5 5 0 1 1 4.5 11"/><path d="M3.2 3v3.3h3.3"/></svg></button>
        <button class="vp-icon-btn vp-graph" title="Open graph">G</button>
      </div>
      <div class="vp-create-panel">
        <div class="vp-create-hint">Create note or folder</div>
        <input class="vp-create-name" placeholder="path/name or folder/path" spellcheck="false" />
        <select class="vp-template-select" title="Template" hidden></select>
        <div class="vp-create-actions">
          <button class="vp-create-note" type="button">Note</button>
          <button class="vp-create-template" type="button" hidden>From Template</button>
          <button class="vp-create-folder" type="button">Folder</button>
          <button class="vp-create-cancel" type="button">Cancel</button>
        </div>
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
    title.style.cssText = 'flex:1 1 auto; min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;';
    const toggle = document.createElement('div');
    toggle.className = 'plan-doc-toggle';
    toggle.innerHTML = MODE_TOGGLE_HTML;
    const status = document.createElement('span');
    status.style.cssText = 'font-size:11px; opacity:.7; min-width:48px; text-align:right;';
    bar.appendChild(title);
    bar.appendChild(toggle);
    bar.appendChild(status);

    const view = document.createElement('div');
    view.className = 'plan-doc-view';
    view.style.cssText = 'flex:1 1 0%; min-height:0; width:100%; box-sizing:border-box; overflow:auto; padding:14px 16px;';
    const ta = document.createElement('textarea');
    ta.className = 'plan-doc-edit';
    ta.spellcheck = false;
    ta.placeholder = 'Open a note from the tree, or ask the librarian...';
    ta.style.cssText = 'flex:1 1 0%; width:100%; min-height:0; box-sizing:border-box; overflow:auto; resize:none; border:none; outline:none; padding:14px 16px; background:transparent; color:var(--text,#d7dae0); font-family:"SF Mono",Menlo,monospace; font-size:13px; line-height:1.55; display:none;';
    doc.appendChild(bar);
    doc.appendChild(view);
    doc.appendChild(ta);
    const blStrip = document.createElement('div');
    blStrip.style.cssText = 'flex-shrink:0; max-height:140px; overflow-y:auto; border-top:1px solid var(--border-color,#333); padding:6px 12px; font-size:12px; display:none;';
    doc.appendChild(blStrip);

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
      openAgentFather: null,
      refresh: null,
      getDoc: () => ta.value,
      currentRel: () => currentRel,
      viewEl: view, taEl: ta, bodyEl: body, railEl: rail, chatHostEl: chatHost,
      statusEl: status, notes: [],
      renderView: null,
      mode: () => mode,
    };

    function openAgentFatherFromVault(seed) {
      if (typeof window.xnautOpenAgentFather !== 'function') return false;
      window.xnautOpenAgentFather({
        source: 'vault',
        vault: seed && seed.vault,
        rel: seed && seed.rel,
        responsibility: seed && seed.responsibility,
      });
      return true;
    }
    entry.openAgentFather = openAgentFatherFromVault;

    const renderView = () => {
      window.xnautMarkdown.renderInto(view, stripPreviewFrontmatter(ta.value) || '_Empty note._');
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
      refreshBacklinks();
    }
    entry.openNote = openNote;

    title.style.cursor = 'pointer';
    title.title = 'Click to rename (updates all [[links]])';
    title.onclick = async () => {
      if (!currentRel) return;
      beginRenameNote(currentRel);
    };

    async function refreshBacklinks() {
      if (!currentRel) {
        blStrip.style.display = 'none';
        return;
      }
      const links = await invoke('vault_backlinks', { vault, rel: currentRel }).catch(() => []);
      if (!links.length) {
        blStrip.style.display = 'none';
        return;
      }
      blStrip.style.display = 'block';
      blStrip.innerHTML = `<div style="opacity:.6; margin-bottom:4px;">${links.length} backlink${links.length > 1 ? 's' : ''}</div>`;
      links.forEach((l) => {
        const rowEl = document.createElement('div');
        rowEl.className = 'vp-note-row';
        rowEl.innerHTML = `<strong>${l.title}</strong> <span style="opacity:.55">${l.snippet || ''}</span>`;
        rowEl.onclick = () => openNote(l.rel);
        blStrip.appendChild(rowEl);
      });
    }

    function resolveStem(target) {
      const key = target.split('/').pop().trim().toLowerCase();
      const hit = (entry.notes || []).find((n) => n.stem_key === key);
      return hit ? hit.rel : null;
    }

    entry.decorate = () => {
      const walker = document.createTreeWalker(view, NodeFilter.SHOW_TEXT, {
        acceptNode: (n) => (n.parentElement && n.parentElement.closest('pre, code, a'))
          ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT,
      });
      const nodes = [];
      while (walker.nextNode()) nodes.push(walker.currentNode);
      const re = /\[\[([^\]\|#]+)(?:#[^\]\|]*)?(?:\|([^\]]+))?\]\]|(^|\s)#([A-Za-z][A-Za-z0-9_/-]*)/g;
      nodes.forEach((tn) => {
        const text = tn.nodeValue;
        if (!re.test(text)) { re.lastIndex = 0; return; }
        re.lastIndex = 0;
        const frag = document.createDocumentFragment();
        let last = 0;
        let m;
        while ((m = re.exec(text))) {
          frag.appendChild(document.createTextNode(text.slice(last, m.index)));
          if (m[1] !== undefined) {
            const a = document.createElement('a');
            a.className = 'vault-wikilink';
            a.dataset.target = m[1].trim();
            a.textContent = m[2] ? m[2].trim() : m[1].trim();
            frag.appendChild(a);
          } else {
            frag.appendChild(document.createTextNode(m[3]));
            const t = document.createElement('span');
            t.className = 'vault-tagchip';
            t.dataset.tag = m[4].toLowerCase();
            t.textContent = '#' + m[4];
            frag.appendChild(t);
          }
          last = m.index + m[0].length;
        }
        frag.appendChild(document.createTextNode(text.slice(last)));
        tn.parentNode.replaceChild(frag, tn);
      });
    };

    view.addEventListener('click', async (e) => {
      const link = e.target.closest('.vault-wikilink');
      if (link) {
        const rel = resolveStem(link.dataset.target);
        if (rel) return openNote(rel);
        setCreatePrefix(`_inbox/${link.dataset.target.split('/').pop().trim()}`);
        return undefined;
      }
      const tag = e.target.closest('.vault-tagchip');
      if (tag && entry.showTag) entry.showTag(tag.dataset.tag);
      return undefined;
    });

    const ac = document.createElement('div');
    ac.style.cssText = 'position:absolute; z-index:40; display:none; background:var(--editor-surface,#22242b); border:1px solid var(--border-color,#333); border-radius:6px; max-height:180px; overflow-y:auto; font-size:12px; box-shadow:0 6px 18px rgba(0,0,0,.4);';
    doc.style.position = 'relative';
    doc.appendChild(ac);
    const closeAc = () => { ac.style.display = 'none'; };
    ta.addEventListener('keyup', (e) => {
      if (['ArrowDown', 'ArrowUp', 'Enter', 'Escape'].includes(e.key)) return;
      const upto = ta.value.slice(0, ta.selectionStart);
      const m = upto.match(/\[\[([^\]\n]*)$/);
      if (!m) return closeAc();
      const q = m[1].toLowerCase();
      const hits = (entry.notes || []).filter((n) => n.stem_key.includes(q) || n.title.toLowerCase().includes(q)).slice(0, 8);
      if (!hits.length) return closeAc();
      ac.innerHTML = hits.map((n) => `<div class="vp-note-row" data-stem="${n.rel.split('/').pop().replace(/\.md$/i, '')}">${n.title}</div>`).join('');
      ac.style.display = 'block';
      ac.style.left = '20px';
      ac.style.top = (bar.getBoundingClientRect().height + 20) + 'px';
      ac.querySelectorAll('.vp-note-row').forEach((rowEl) => {
        rowEl.onclick = () => {
          const start = ta.selectionStart;
          const before = ta.value.slice(0, start).replace(/\[\[[^\]\n]*$/, '[[' + rowEl.dataset.stem + ']]');
          ta.value = before + ta.value.slice(start);
          closeAc();
          ta.focus();
          ta.dispatchEvent(new Event('input'));
        };
      });
      return undefined;
    });
    ta.addEventListener('keydown', (e) => {
      if (ac.style.display === 'none') return;
      if (e.key === 'Escape') {
        e.preventDefault();
        closeAc();
      }
      if (e.key === 'Enter') {
        const first = ac.querySelector('.vp-note-row');
        if (first) {
          e.preventDefault();
          first.onclick();
        }
      }
    });
    ta.addEventListener('blur', () => setTimeout(closeAc, 200));

    let menuEl = null;
    function closeMenu() {
      if (menuEl) menuEl.remove();
      menuEl = null;
    }
    const closeMenuOnDoc = () => closeMenu();
    document.addEventListener('click', closeMenuOnDoc);
    const basename = (rel) => rel.split('/').filter(Boolean).pop() || rel;
    const dirname = (rel) => {
      const i = rel.lastIndexOf('/');
      return i < 0 ? '' : rel.slice(0, i);
    };
    const joinRel = (dir, name) => (dir ? `${dir}/${name}` : name);
    const relUnder = (parent, child) => child === parent || child.startsWith(parent + '/');
    function configureActionPanel(opts) {
      createPanel.dataset.open = '1';
      createHint.textContent = opts.hint || '';
      createName.hidden = !!opts.hideInput;
      createName.readOnly = !!opts.readOnly;
      createName.placeholder = opts.placeholder || '';
      createName.value = opts.value || '';
      createTemplateSelect.hidden = !opts.showTemplateSelect;
      createNoteBtn.textContent = opts.primaryLabel || 'OK';
      createTemplateBtn.hidden = !opts.onTemplate;
      createFolderBtn.textContent = opts.secondaryLabel || '';
      createFolderBtn.hidden = !opts.onSecondary;
      createNoteBtn.onclick = async () => {
        try {
          await opts.onPrimary(createName.value);
        } catch (e) {
          countEl.textContent = 'action failed';
          console.error('[vault] action failed', e);
        }
      };
      createFolderBtn.onclick = opts.onSecondary ? async () => {
        try {
          await opts.onSecondary(createName.value);
        } catch (e) {
          countEl.textContent = 'action failed';
          console.error('[vault] action failed', e);
        }
      } : null;
      createTemplateBtn.onclick = opts.onTemplate ? async () => {
        try {
          await opts.onTemplate(createName.value, createTemplateSelect.value);
        } catch (e) {
          countEl.textContent = 'action failed';
          console.error('[vault] action failed', e);
        }
      } : null;
      createName.focus();
      createName.setSelectionRange(createName.value.length, createName.value.length);
    }
    function setCreatePrefix(prefix) {
      const base = prefix || '';
      configureActionPanel({
        hint: 'Create note or folder',
        placeholder: 'path/name or folder/path',
        value: base ? (base.endsWith('/') ? base : base + '/') : '',
        primaryLabel: 'Note',
        showTemplateSelect: true,
        secondaryLabel: 'Folder',
        onPrimary: createNoteFromPanel,
        onTemplate: createNoteFromTemplate,
        onSecondary: createFolderFromPanel,
      });
      refreshTemplateSelect();
    }
    async function clearCurrentIfUnder(rel) {
      if (!currentRel || !relUnder(rel, currentRel)) return;
      currentRel = null;
      ta.value = '';
      title.textContent = 'No note open';
      blStrip.style.display = 'none';
      if (mode === 'preview') renderView();
    }
    async function performRenameNote(rel, next) {
      const oldStem = basename(rel).replace(/\.md$/i, '');
      next = next.trim();
      if (!next || next === oldStem || next.includes('/')) return;
      await flushSave();
      const res = await invoke('vault_note_rename', { vault, rel, newStem: next });
      if (currentRel === rel) currentRel = res.new_rel;
      closeCreate();
      await refresh();
      if (currentRel === res.new_rel) await openNote(currentRel);
      countEl.textContent = `renamed - ${res.links_updated} links updated`;
    }
    function beginRenameNote(rel) {
      configureActionPanel({
        hint: 'Rename note: ' + rel,
        placeholder: 'New note name',
        value: basename(rel).replace(/\.md$/i, ''),
        primaryLabel: 'Rename',
        onPrimary: (value) => performRenameNote(rel, value),
      });
    }
    async function performRenameFolder(rel, next) {
      next = next.trim();
      if (!next || next === basename(rel) || next.includes('/')) return;
      const toRel = joinRel(dirname(rel), next);
      await invoke('vault_folder_move', { vault, fromRel: rel, toRel });
      const movedCurrent = currentRel && relUnder(rel, currentRel);
      if (movedCurrent) currentRel = toRel + currentRel.slice(rel.length);
      closeCreate();
      await refresh();
      if (movedCurrent) await openNote(currentRel);
      countEl.textContent = 'folder renamed';
    }
    function beginRenameFolder(rel) {
      configureActionPanel({
        hint: 'Rename folder: ' + rel,
        placeholder: 'New folder name',
        value: basename(rel),
        primaryLabel: 'Rename',
        onPrimary: (value) => performRenameFolder(rel, value),
      });
    }
    async function moveNoteToFolder(rel, targetFolder) {
      const toRel = joinRel(targetFolder, basename(rel));
      if (toRel === rel) return;
      await flushSave();
      await invoke('vault_note_move', { vault, fromRel: rel, toRel });
      if (currentRel === rel) currentRel = toRel;
      closeCreate();
      await refresh();
      if (currentRel === toRel) await openNote(toRel);
      countEl.textContent = 'moved ' + basename(rel);
    }
    async function moveFolderToFolder(rel, targetFolder) {
      const toRel = joinRel(targetFolder, basename(rel));
      if (toRel === rel) return;
      if (relUnder(rel, toRel)) {
        countEl.textContent = 'cannot move folder there';
        return;
      }
      await flushSave();
      await invoke('vault_folder_move', { vault, fromRel: rel, toRel });
      const movedCurrent = currentRel && relUnder(rel, currentRel);
      if (movedCurrent) currentRel = toRel + currentRel.slice(rel.length);
      closeCreate();
      await refresh();
      if (movedCurrent) await openNote(currentRel);
      countEl.textContent = 'folder moved';
    }
    function beginMove(kind, rel) {
      configureActionPanel({
        hint: `Move ${kind}: ${rel}`,
        placeholder: 'Target folder, blank for vault root',
        value: dirname(rel),
        primaryLabel: 'Move',
        onPrimary: (value) => {
          const folder = cleanRel(value);
          return kind === 'note' ? moveNoteToFolder(rel, folder) : moveFolderToFolder(rel, folder);
        },
      });
    }
    async function deleteNote(rel) {
      await flushSave();
      await invoke('vault_note_delete', { vault, rel });
      if (currentRel === rel) await clearCurrentIfUnder(rel);
      closeCreate();
      const tree = await refresh();
      countEl.textContent = `deleted - ${tree.notes.length} notes`;
    }
    async function deleteFolder(rel) {
      await flushSave();
      await invoke('vault_folder_delete', { vault, rel });
      await clearCurrentIfUnder(rel);
      closeCreate();
      const tree = await refresh();
      countEl.textContent = `folder deleted - ${tree.notes.length} notes`;
    }
    function beginDelete(kind, rel) {
      configureActionPanel({
        hint: `Move ${kind} to .trash: ${rel}`,
        value: rel,
        readOnly: true,
        primaryLabel: 'Delete',
        onPrimary: () => kind === 'note' ? deleteNote(rel) : deleteFolder(rel),
      });
    }
    async function handleDrop(payload, targetFolder) {
      if (!payload || !payload.rel || payload.rel === targetFolder) return;
      try {
        if (payload.kind === 'note') await moveNoteToFolder(payload.rel, targetFolder);
        else if (payload.kind === 'folder') await moveFolderToFolder(payload.rel, targetFolder);
      } catch (e) {
        countEl.textContent = 'move failed';
        console.error('[vault] drop move failed', e);
      }
    }
    function showContextMenu(kind, rel, x, y) {
      closeMenu();
      menuEl = document.createElement('div');
      menuEl.className = 'vp-context-menu';
      const add = (label, fn, danger) => {
        const btn = document.createElement('button');
        btn.type = 'button';
        btn.textContent = label;
        if (danger) btn.dataset.danger = '1';
        btn.onmousedown = async (e) => {
          e.preventDefault();
          e.stopPropagation();
          closeMenu();
          try { await fn(); } catch (err) {
            countEl.textContent = 'action failed';
            console.error('[vault] menu action failed', err);
          }
        };
        btn.onclick = (e) => {
          e.preventDefault();
          e.stopPropagation();
        };
        menuEl.appendChild(btn);
      };
      if (kind === 'note') {
        add('Open', () => openNote(rel));
        add('Rename', () => beginRenameNote(rel));
        add('Move to...', () => beginMove('note', rel));
        add('Delete', () => beginDelete('note', rel), true);
      } else {
        add('New note here', () => setCreatePrefix(rel));
        add('New folder here', () => setCreatePrefix(rel));
        add('Rename', () => beginRenameFolder(rel));
        add('Move to...', () => beginMove('folder', rel));
        add('Delete folder', () => beginDelete('folder', rel), true);
      }
      document.body.appendChild(menuEl);
      const rect = menuEl.getBoundingClientRect();
      menuEl.style.left = Math.min(x, window.innerWidth - rect.width - 8) + 'px';
      menuEl.style.top = Math.min(y, window.innerHeight - rect.height - 8) + 'px';
    }

    async function refresh() {
      const tree = await invoke('vault_tree', { vault });
      body.innerHTML = '';
      renderTree(buildTree(tree.dirs, tree.notes), body, {
        openNote,
        showMenu: showContextMenu,
        onDrop: handleDrop,
      }, currentRel);
      countEl.textContent = `${tree.notes.length} notes`;
      entry.notes = tree.notes;
      return tree;
    }
    entry.refresh = refresh;

    let railTab = 'notes';
    let externalRefreshTimer = null;
    function refreshExternalChanges(reason) {
      if (railTab !== 'notes') return;
      clearTimeout(externalRefreshTimer);
      externalRefreshTimer = setTimeout(async () => {
        try {
          const tree = await refresh();
          countEl.textContent = `refreshed - ${tree.notes.length} notes`;
        } catch (e) {
          console.error(`[vault] external refresh failed${reason ? ` (${reason})` : ''}`, e);
        }
      }, 120);
    }
    const tabsEl = rail.querySelector('.vp-tabs');
    async function showRailTab(t, arg) {
      railTab = t;
      tabsEl.querySelectorAll('button').forEach((b) => { b.dataset.active = b.dataset.tab === t ? '1' : '0'; });
      body.innerHTML = '';
      if (t === 'notes') return refresh();
      if (t === 'tags') {
        const tags = await invoke('vault_tags', { vault });
        if (arg) {
          const back = document.createElement('div');
          back.className = 'vp-note-row';
          back.textContent = '< all tags';
          back.onclick = () => showRailTab('tags');
          body.appendChild(back);
          const notes = await invoke('vault_tag_notes', { vault, tag: arg });
          notes.forEach((n) => {
            const rowEl = document.createElement('div');
            rowEl.className = 'vp-note-row';
            rowEl.textContent = n.title;
            rowEl.onclick = () => openNote(n.rel);
            body.appendChild(rowEl);
          });
          return undefined;
        }
        tags.forEach(({ tag, count }) => {
          const rowEl = document.createElement('div');
          rowEl.className = 'vp-note-row';
          rowEl.innerHTML = `<span class="vault-tagchip">#${tag}</span> <span style="opacity:.5">${count}</span>`;
          rowEl.onclick = () => showRailTab('tags', tag);
          body.appendChild(rowEl);
        });
        return undefined;
      }
      if (t === 'search') {
        const input = document.createElement('input');
        input.placeholder = 'Search vault...';
        input.style.cssText = 'width:100%; box-sizing:border-box; margin-bottom:6px; background:var(--input-bg,rgba(255,255,255,.06)); color:inherit; border:1px solid var(--border-color,#333); border-radius:6px; padding:5px 8px; font:inherit;';
        const results = document.createElement('div');
        body.appendChild(input);
        body.appendChild(results);
        let t2 = null;
        input.oninput = () => {
          clearTimeout(t2);
          t2 = setTimeout(async () => {
            const hits = await invoke('vault_search', { vault, query: input.value });
            results.innerHTML = '';
            hits.forEach((h) => {
              const rowEl = document.createElement('div');
              rowEl.className = 'vp-note-row';
              rowEl.innerHTML = `<div>${h.title}</div><div style="font-size:11px; opacity:.55; white-space:normal">${h.snippet || ''}</div>`;
              rowEl.onclick = () => openNote(h.rel);
              results.appendChild(rowEl);
            });
          }, 250);
        };
        input.focus();
      }
      return undefined;
    }
    tabsEl.querySelectorAll('button').forEach((b) => { b.onclick = () => showRailTab(b.dataset.tab).catch((e) => console.error('[vault] tab failed', e)); });
    entry.showTag = (tag) => showRailTab('tags', tag);

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
      showLibrarianConversationsPane();
    }

    rail.querySelector('.vp-vault').onchange = (e) => switchVault(e.target.value).catch((err) => console.error('[vault] switch failed', err));
    const createPanel = rail.querySelector('.vp-create-panel');
    const createHint = rail.querySelector('.vp-create-hint');
    const createName = rail.querySelector('.vp-create-name');
    const createTemplateSelect = rail.querySelector('.vp-template-select');
    const createNoteBtn = rail.querySelector('.vp-create-note');
    const createTemplateBtn = rail.querySelector('.vp-create-template');
    const createFolderBtn = rail.querySelector('.vp-create-folder');
    const createCancelBtn = rail.querySelector('.vp-create-cancel');
    const cleanRel = (value) => value.trim().replace(/^\/+|\/+$/g, '');
    const closeCreate = () => {
      createPanel.dataset.open = '0';
      createHint.textContent = 'Create note or folder';
      createName.value = '';
      createName.hidden = false;
      createName.readOnly = false;
      createName.placeholder = 'path/name or folder/path';
      createTemplateSelect.hidden = true;
      createNoteBtn.textContent = 'Note';
      createTemplateBtn.hidden = true;
      createFolderBtn.textContent = 'Folder';
      createFolderBtn.hidden = false;
    };
    rail.querySelector('.vp-new').onclick = () => {
      if (createPanel.dataset.open === '1') closeCreate();
      else setCreatePrefix('');
    };
    createCancelBtn.onclick = closeCreate;
    async function createNoteFromPanel(value) {
      const name = cleanRel(value);
      if (!name) {
        createName.focus();
        return;
      }
      const rel = name.replace(/\.md$/i, '') + '.md';
      await invoke('vault_note_create', { vault, rel, content: null });
      closeCreate();
      await refresh();
      await openNote(rel);
      countEl.textContent = 'created ' + rel;
    }
    function templateNotes() {
      return (entry.notes || [])
        .filter((note) => note && /^Templates\/.+\.md$/i.test(note.rel || ''))
        .sort((a, b) => a.rel.localeCompare(b.rel));
    }
    function templateLabel(rel) {
      return rel
        .replace(/^Templates\//i, '')
        .replace(/\.md$/i, '')
        .replace(/[_-]?template$/i, '')
        .replace(/[_-]+/g, ' ');
    }
    function titleFromRel(rel) {
      const raw = basename(rel).replace(/\.md$/i, '').replace(/[_-]+/g, ' ').trim();
      if (!raw) return 'Untitled';
      return raw.replace(/\b[a-z]/g, (ch) => ch.toUpperCase());
    }
    function applyTemplateVars(content, rel) {
      const today = new Date().toISOString().slice(0, 10);
      return String(content || '')
        .replace(/\{\{\s*title\s*\}\}/gi, titleFromRel(rel))
        .replace(/\{\{\s*date\s*\}\}/gi, today);
    }
    function refreshTemplateSelect() {
      const selected = createTemplateSelect.value;
      const templates = templateNotes();
      createTemplateSelect.innerHTML = '';
      const placeholder = document.createElement('option');
      placeholder.value = '';
      placeholder.textContent = templates.length ? 'Select template...' : 'No templates found';
      createTemplateSelect.appendChild(placeholder);
      templates.forEach((note) => {
        const option = document.createElement('option');
        option.value = note.rel;
        option.textContent = templateLabel(note.rel);
        createTemplateSelect.appendChild(option);
      });
      if (templates.some((note) => note.rel === selected)) createTemplateSelect.value = selected;
      else if (templates.length === 1) createTemplateSelect.value = templates[0].rel;
    }
    async function createNoteFromTemplate(value, templateRel) {
      const name = cleanRel(value);
      if (!name) {
        createName.focus();
        return;
      }
      if (!templateRel) {
        countEl.textContent = 'select a template';
        createTemplateSelect.focus();
        return;
      }
      const rel = name.replace(/\.md$/i, '') + '.md';
      const templateContent = await invoke('vault_note_read', { vault, rel: templateRel });
      const content = applyTemplateVars(templateContent, rel);
      await invoke('vault_note_create', { vault, rel, content });
      closeCreate();
      await refresh();
      await openNote(rel);
      countEl.textContent = 'created from template ' + rel;
    }
    async function createFolderFromPanel(value) {
      const rel = cleanRel(value);
      if (!rel) {
        createName.focus();
        return;
      }
      await invoke('vault_folder_create', { vault, rel });
      closeCreate();
      const tree = await refresh();
      countEl.textContent = `folder created - ${tree.notes.length} notes`;
    }
    setCreatePrefix('');
    closeCreate();
    createName.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') closeCreate();
      if (e.key === 'Enter') createNoteBtn.click();
    });
    rail.querySelector('.vp-refresh').onclick = async () => {
      countEl.textContent = 'refreshing...';
      try {
        const tree = await refresh();
        countEl.textContent = `refreshed - ${tree.notes.length} notes`;
      } catch (e) {
        countEl.textContent = 'refresh failed';
        console.error('[vault] refresh failed', e);
      }
    };
    rail.querySelector('.vp-graph').onclick = () => {
      if (window.xnautAttachGraphTab) window.xnautAttachGraphTab({ path: root + '/' + vault });
    };

    const syncEl = rail.querySelector('.vp-sync');
    syncEl.innerHTML = '<button class="vp-icon-btn vp-push" title="Push to NAS">up</button><button class="vp-icon-btn vp-pull" title="Pull from NAS">dn</button><span class="vp-sync-state"></span>';
    const syncState = syncEl.querySelector('.vp-sync-state');
    async function doSync(direction) {
      syncState.textContent = '...';
      try {
        await invoke('vault_sync', { direction });
        syncState.textContent = direction === 'push' ? 'ok up' : 'ok dn';
      } catch (e) {
        syncState.textContent = 'err';
        syncState.title = String(e);
        console.error('[vault] sync failed', e);
      }
    }
    syncEl.querySelector('.vp-push').onclick = () => doSync('push');
    syncEl.querySelector('.vp-pull').onclick = async () => {
      await doSync('pull');
      await refresh();
    };
    let pushTimer = null;
    entry.onVaultChanged = () => {
      clearTimeout(pushTimer);
      pushTimer = setTimeout(() => doSync('push'), 60000);
    };

    await invoke('vault_open', { vault });
    await refresh();
    if (opts.openRel) await openNote(String(opts.openRel).replace(/^\/+/, ''));
    showLibrarianConversationsPane();
    const onWindowFocus = () => refreshExternalChanges('focus');
    const onVisibilityChange = () => {
      if (!document.hidden) refreshExternalChanges('visibilitychange');
    };
    window.addEventListener('focus', onWindowFocus);
    document.addEventListener('visibilitychange', onVisibilityChange);
    let changeTimer = null;
    const unlisten = await listen('vault://changed', (ev) => {
      const p = ev.payload || {};
      if (p.vault !== vault) return;
      clearTimeout(changeTimer);
      changeTimer = setTimeout(async () => {
        if (railTab === 'notes') await refresh();
        if (p.rel === currentRel && mode === 'preview' && !saveTimer) {
          ta.value = await invoke('vault_note_read', { vault, rel: currentRel }).catch(() => ta.value);
          renderView();
        }
        refreshBacklinks();
        if (entry.onVaultChanged) entry.onVaultChanged();
      }, 400);
    });
    entry.dispose = () => {
      try { unlisten(); } catch (_) { /* already gone */ }
      clearTimeout(externalRefreshTimer);
      window.removeEventListener('focus', onWindowFocus);
      document.removeEventListener('visibilitychange', onVisibilityChange);
      document.removeEventListener('click', closeMenuOnDoc);
      closeMenu();
    };
    setMode('preview');

    const persona = [
      '',
      '--- VAULT LIBRARIAN ---',
      `You are the librarian of the user's personal markdown vault "${vault}". You manage notes, find old ideas, and keep things filed.`,
      'The OPEN NOTE is shown in a side pane and given to you each turn as "CURRENT PLAN DOCUMENT" (it is the note, not a plan).',
      'When you create or revise the OPEN note, output its COMPLETE content wrapped EXACTLY between a line "===PLAN DOCUMENT===" and a line "===END PLAN DOCUMENT===". Normal Markdown; [[Wikilinks]] to reference other notes; ```mermaid allowed. Never paste note content into chat prose.',
      'Build on the CURRENT PLAN DOCUMENT - extend and refine; never drop content the user kept.',
      'TOOLS - to act on the REST of the vault, reply with ONLY a JSON object (no other text):',
      '  {"action":"vault_search","query":"..."} find notes by title/tag/content',
      '  {"action":"vault_read","rel":"folder/note.md"} read another note',
      '  {"action":"vault_create","rel":"folder/name.md","content":"..."} create a new note (file unfiled ideas under _inbox/)',
      '  {"action":"vault_move","from":"a.md","to":"folder/a.md"} move/refile a note',
      '  {"action":"vault_tag","rel":"a.md","add":["tag"],"remove":[]} change a note frontmatter tags',
      'Tool results arrive as a system message; continue from them. Max 5 tool calls per user request. You cannot delete notes.',
      'Chat replies: short - one or two sentences plus any question.',
    ].join('\n');

    entry.chat = await window.xnautCreateChatPane(tabId, chatHost, {
      chatKey: 'vault:' + vault,
      planMode: {
        getDoc: () => ta.value,
        onPlanDoc: async (md) => {
          ta.value = md;
          if (mode === 'preview') renderView();
          if (currentRel) {
            await persist();
          } else {
            const stem = (md.match(/^#\s+(.+)$/m) || [, 'untitled'])[1].trim().replace(/[/:]/g, '-');
            const rel = `_inbox/${stem}.md`;
            await invoke('vault_note_write', { vault, rel, content: md });
            currentRel = rel;
            title.textContent = rel;
            await refresh();
          }
        },
        title: 'Librarian - ' + vault,
        persona,
      },
      vaultTools: { vault: () => vault, entry },
    });

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
