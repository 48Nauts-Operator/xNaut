// Markdown panes — Phase 7 of the Orca port.
//
// Architecture: a markdown pane is a TipTap editor instance attached to a
// pane DOM element. Unlike browser panes (which float a native webview),
// markdown panes are pure DOM — they live inside the parent webview and
// don't need bounds-sync tricks.
//
// TipTap is loaded lazily as ESM from jsdelivr the first time a markdown
// pane is created. Subsequent panes reuse the cached module promise. This
// keeps xNaut's initial paint cheap (TipTap is ~500KB minified+deps) while
// still delivering Orca-class editing once requested.
(function () {
  'use strict';

  const $ = (id) => document.getElementById(id);

  // label -> { paneEl, editor, filePath, tabId }
  const panes = new Map();
  let labelCounter = 0;
  function nextLabel() {
    labelCounter += 1;
    return `md-${Date.now().toString(36)}-${labelCounter}`;
  }

  // Lazy TipTap loader. Resolves to { Editor, StarterKit } on first call,
  // returns the same promise on subsequent calls so we don't re-import.
  let tiptapPromise = null;
  function loadTipTap() {
    if (tiptapPromise) return tiptapPromise;
    // jsdelivr's `+esm` resolver returns a single module that wraps the
    // package with its dependencies pre-bundled — works under the existing
    // CSP (jsdelivr is already allowed for scripts).
    tiptapPromise = Promise.all([
      import('https://cdn.jsdelivr.net/npm/@tiptap/[email protected]/+esm'),
      import('https://cdn.jsdelivr.net/npm/@tiptap/[email protected]/+esm'),
    ]).then(([core, starterMod]) => ({
      Editor: core.Editor,
      StarterKit: starterMod.default || starterMod.StarterKit,
    }));
    return tiptapPromise;
  }

  /**
   * Build the DOM for a markdown pane and instantiate a TipTap editor inside.
   * Returns the pane entry that the tab system stores in tab.terminals[]:
   *   { kind: 'markdown', label, pane, filePath, editor }
   */
  async function createMarkdownPane(tabId, parentContainer, opts) {
    opts = opts || {};
    const label = nextLabel();

    const pane = document.createElement('div');
    pane.className = 'md-pane';
    pane.dataset.markdownLabel = label;
    pane.style.cssText = [
      'display:flex',
      'flex-direction:column',
      'flex:1 1 0%',
      'width:100%',
      'height:100%',
      'min-width:0',
      'min-height:0',
      'overflow:hidden',
      'background:var(--editor-surface)',
      'border-radius:var(--radius-md)',
    ].join('; ');

    const bar = document.createElement('div');
    bar.className = 'md-bar';
    bar.innerHTML = `
      <span class="md-filename" title="Unsaved">untitled.md</span>
      <span class="md-dirty" hidden> •</span>
      <span class="md-spacer"></span>
      <button class="btn-icon md-save" title="Save (Cmd+S)" aria-label="Save markdown">
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor"><path d="M3 2h8l2 2v10H3z"/><path d="M5 2v4h6V2"/></svg>
      </button>
      <button class="btn-icon md-close" data-variant="destructive" title="Close pane" aria-label="Close markdown pane">
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor"><line x1="4" y1="4" x2="12" y2="12"/><line x1="12" y1="4" x2="4" y2="12"/></svg>
      </button>
    `;
    pane.appendChild(bar);

    // The editor mount point. TipTap will inject its ProseMirror DOM here.
    const mount = document.createElement('div');
    mount.className = 'md-mount';
    pane.appendChild(mount);

    parentContainer.appendChild(pane);

    const filenameEl = bar.querySelector('.md-filename');
    const dirtyEl = bar.querySelector('.md-dirty');

    let editor = null;
    let filePath = opts.filePath || null;
    let isDirty = false;
    const markDirty = (d) => {
      isDirty = d;
      if (dirtyEl) dirtyEl.hidden = !d;
    };

    try {
      const { Editor, StarterKit } = await loadTipTap();
      if (!document.body.contains(pane)) return null; // tab closed during load
      editor = new Editor({
        element: mount,
        extensions: [StarterKit],
        content: opts.content || '<h1>New document</h1><p>Start writing…</p>',
        autofocus: 'end',
        editorProps: {
          attributes: { class: 'md-prose' },
        },
        onUpdate: () => markDirty(true),
      });
    } catch (e) {
      mount.innerHTML = `<div class="md-load-error">Failed to load TipTap: ${escapeText(String(e))}</div>`;
      console.error('[markdown-pane] failed to load TipTap', e);
      return { kind: 'markdown', label, pane, filePath: null, editor: null };
    }

    // Cmd/Ctrl+S to save (handled at pane level so it doesn't fight other shortcuts).
    pane.addEventListener('keydown', (e) => {
      const isMod = e.metaKey || e.ctrlKey;
      if (isMod && (e.key === 's' || e.key === 'S')) {
        e.preventDefault();
        // Save handler will land in Commit 2 — for now just mark clean.
        markDirty(false);
        console.log('[markdown-pane] save requested (handler TBD in Commit 2)');
      }
    });

    bar.querySelector('.md-save').onclick = () => {
      markDirty(false);
      console.log('[markdown-pane] save clicked (handler TBD in Commit 2)');
    };
    bar.querySelector('.md-close').onclick = () => destroyMarkdownPane(label);

    const entry = { kind: 'markdown', label, pane, filePath, editor };
    panes.set(label, entry);
    return entry;
  }

  async function destroyMarkdownPane(label) {
    const entry = panes.get(label);
    if (!entry) return;
    if (entry.editor) entry.editor.destroy();
    if (entry.pane && entry.pane.parentNode) entry.pane.parentNode.removeChild(entry.pane);
    panes.delete(label);
  }

  function escapeText(s) {
    return String(s).replace(/[&<>"']/g, (c) => ({ '&':'&amp;', '<':'&lt;', '>':'&gt;', '"':'&quot;', "'":'&#39;' }[c]));
  }

  // Public API hooks (mirror the browser-pane shape).
  window.xnautCreateMarkdownPane = createMarkdownPane;
  window.xnautDestroyMarkdownPane = destroyMarkdownPane;

  /**
   * Top-bar "New Markdown" handler — creates a fresh tab with a single
   * markdown pane. Delegates to app.js's xnautAttachMarkdownTab so the
   * tab object goes through the same renderTabs/switchTab path as
   * terminal/browser/agent tabs.
   */
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
