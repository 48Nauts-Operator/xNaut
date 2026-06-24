// Markdown panes — Phase 7 of the Orca port (dependency-free rewrite).
//
// A markdown pane is a plain <textarea> (Edit) plus a rendered Preview using
// the shared window.xnautMarkdown renderer (headings, code w/ highlight.js,
// tables, task lists, ```mermaid diagrams). It deliberately does NOT use the
// CDN ESM TipTap editor, which fails under this WebKit ("Can't find variable:
// Editor"). Pure DOM — lives inside the parent webview.
(function () {
  'use strict';

  const $ = (id) => document.getElementById(id);

  const panes = new Map();
  let labelCounter = 0;
  function nextLabel() {
    labelCounter += 1;
    return `md-${Date.now().toString(36)}-${labelCounter}`;
  }

  function injectStyles() {
    if ($('md-pane-styles')) return;
    const st = document.createElement('style');
    st.id = 'md-pane-styles';
    st.textContent = `
.md-pane { display:flex; flex-direction:column; flex:1 1 0%; width:100%; height:100%; min-width:0; min-height:0; overflow:hidden; background:var(--editor-surface, #1b1d23); border-radius:var(--radius-md, 8px); }
.md-bar { display:flex; align-items:center; gap:8px; padding:8px 12px; border-bottom:1px solid var(--border, rgba(255,255,255,.1)); font-size:12px; color:var(--text-muted, #8a8f98); flex-shrink:0; }
.md-filename { font-weight:600; color:var(--text, #d7dae0); }
.md-dirty { color:var(--agent-thinking, #4dffd0); }
.md-toggle { margin-left:auto; display:flex; border:1px solid var(--border, rgba(255,255,255,.16)); border-radius:6px; overflow:hidden; }
.md-toggle button { background:transparent; border:none; color:var(--text-muted, #8a8f98); font:inherit; font-size:11px; padding:3px 10px; cursor:pointer; }
.md-toggle button[data-active="1"] { background:var(--accent, #4f8cff); color:#fff; }
.md-iconbtn { background:transparent; border:none; color:var(--text-muted, #8a8f98); cursor:pointer; padding:3px; display:flex; }
.md-iconbtn:hover { color:var(--text, #fff); }
.md-iconbtn svg { width:14px; height:14px; }
.md-view { flex:1 1 0%; min-height:0; overflow:auto; padding:24px 32px; }
.md-edit { flex:1 1 0%; width:100%; min-height:0; box-sizing:border-box; resize:none; border:none; outline:none; padding:14px 16px; background:transparent; color:var(--text, #d7dae0); font-family:"SF Mono",Menlo,"JetBrains Mono",monospace; font-size:13px; line-height:1.55; }
`;
    document.head.appendChild(st);
  }

  async function createMarkdownPane(tabId, parentContainer, opts) {
    opts = opts || {};
    injectStyles();
    const label = nextLabel();

    const pane = document.createElement('div');
    pane.className = 'md-pane';
    pane.dataset.markdownLabel = label;

    const bar = document.createElement('div');
    bar.className = 'md-bar';
    bar.innerHTML = `
      <span class="md-filename" title="Unsaved">untitled.md</span>
      <span class="md-dirty" hidden>•</span>
      <div class="md-toggle"><button data-mode="preview" data-active="1">Preview</button><button data-mode="edit">Edit</button></div>
      <button class="md-iconbtn md-save" title="Save (Cmd+S)" aria-label="Save markdown"><svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4"><path d="M3 2h8l2 2v10H3z"/><path d="M5 2v4h6V2"/></svg></button>
      <button class="md-iconbtn md-close" title="Close pane" aria-label="Close markdown pane"><svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><line x1="4" y1="4" x2="12" y2="12"/><line x1="12" y1="4" x2="4" y2="12"/></svg></button>`;
    pane.appendChild(bar);

    const view = document.createElement('div');
    view.className = 'md-view xnaut-md';
    const ta = document.createElement('textarea');
    ta.className = 'md-edit';
    ta.spellcheck = false;
    ta.placeholder = 'Write Markdown… (# heading, - list, **bold**, ```mermaid diagrams)';
    ta.style.display = 'none';
    pane.appendChild(view);
    pane.appendChild(ta);
    parentContainer.appendChild(pane);

    const filenameEl = bar.querySelector('.md-filename');
    const dirtyEl = bar.querySelector('.md-dirty');
    let filePath = opts.filePath || null;
    const markDirty = (d) => { if (dirtyEl) dirtyEl.hidden = !d; };
    const setFilePath = (p) => {
      filePath = p;
      filenameEl.textContent = p ? (p.split('/').pop() || p) : 'untitled.md';
      filenameEl.title = p || 'Unsaved';
    };

    // Initial content: explicit markdown/content, else read the file, else empty.
    let initial = opts.markdown || opts.content || '';
    if (!initial && opts.filePath) {
      try {
        const body = await invokeRust('read_file', { path: opts.filePath });
        initial = typeof body === 'string' ? body : (body && body.content) || '';
      } catch (e) { console.error('[markdown-pane] open failed', e); }
    }
    if (!document.body.contains(pane)) return null; // tab closed during load
    ta.value = initial;
    setFilePath(filePath);

    let mode = 'preview';
    const renderView = () => {
      window.xnautMarkdown.renderInto(view, ta.value || '_Empty document._');
    };
    const setMode = (m) => {
      mode = m;
      if (m === 'preview') { renderView(); view.style.display = 'block'; ta.style.display = 'none'; }
      else { view.style.display = 'none'; ta.style.display = 'block'; ta.focus(); }
      bar.querySelectorAll('.md-toggle button').forEach((b) => { b.dataset.active = b.dataset.mode === m ? '1' : '0'; });
    };
    bar.querySelectorAll('.md-toggle button').forEach((b) => { b.onclick = () => setMode(b.dataset.mode); });

    ta.addEventListener('input', () => markDirty(true));

    async function save() {
      if (!filePath) { filenameEl.textContent = 'untitled.md (open a file to set a path)'; return; }
      try {
        await invokeRust('write_file', { path: expandHome(filePath), content: ta.value });
        markDirty(false);
      } catch (e) { console.error('[markdown-pane] save failed', e); filenameEl.title = 'Save failed: ' + e; }
    }
    async function open() {
      // Programmatic open (used by the file navigator); pass a path in opts instead.
      if (!opts.openPath) return;
      try {
        const body = await invokeRust('read_file', { path: expandHome(opts.openPath) });
        ta.value = typeof body === 'string' ? body : (body && body.content) || '';
        setFilePath(opts.openPath);
        markDirty(false);
        if (mode === 'preview') renderView();
      } catch (e) { console.error('[markdown-pane] open failed', e); }
    }

    pane.addEventListener('keydown', (e) => {
      if ((e.metaKey || e.ctrlKey) && (e.key === 's' || e.key === 'S')) { e.preventDefault(); save(); }
    });
    bar.querySelector('.md-save').onclick = () => save();
    bar.querySelector('.md-close').onclick = () => destroyMarkdownPane(label);

    // New empty docs open in Edit; opened files open in Preview.
    setMode(opts.filePath || initial ? 'preview' : 'edit');

    const entry = { kind: 'markdown', label, pane, filePath, save, open, getMarkdown: () => ta.value };
    panes.set(label, entry);
    return entry;
  }

  async function invokeRust(cmd, args) {
    const inv = window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke;
    if (!inv) throw new Error('Tauri API not available');
    return inv(cmd, args);
  }

  let cachedHome = null;
  function expandHome(p) {
    if (!p || !p.startsWith('~')) return p;
    if (cachedHome) return cachedHome + p.slice(1);
    try { invokeRust('get_home_directory', {}).then((h) => { cachedHome = h; }).catch(() => {}); } catch (_) {}
    return p;
  }

  async function destroyMarkdownPane(label) {
    const entry = panes.get(label);
    if (!entry) return;
    if (entry.pane && entry.pane.parentNode) entry.pane.parentNode.removeChild(entry.pane);
    panes.delete(label);
  }

  window.xnautCreateMarkdownPane = createMarkdownPane;
  window.xnautDestroyMarkdownPane = destroyMarkdownPane;

  async function newMarkdownTab(opts) {
    if (typeof window.xnautAttachMarkdownTab !== 'function') {
      console.warn('xnautAttachMarkdownTab not yet defined in app.js');
      return;
    }
    return window.xnautAttachMarkdownTab(opts || {});
  }
  window.xnautNewMarkdownTab = newMarkdownTab;

  function wireButton() {
    const btn = $('btn-new-markdown');
    if (btn) btn.onclick = () => newMarkdownTab().catch((e) => console.error('new markdown tab failed:', e));
  }
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', wireButton);
  } else {
    wireButton();
  }
})();
