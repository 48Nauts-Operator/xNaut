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

  // Lazy TipTap loader. Resolves to the full extension set on first call,
  // returns the same promise on subsequent calls so we don't re-import.
  let tiptapPromise = null;
  function loadTipTap() {
    if (tiptapPromise) return tiptapPromise;
    // jsdelivr's `+esm` resolver returns a single module that wraps the
    // package with its dependencies pre-bundled — works under the existing
    // CSP (jsdelivr is already allowed for scripts).
    const j = (p, v) => import(`https://cdn.jsdelivr.net/npm/@tiptap/${p}@${v}/+esm`);
    const jnp = (p, v) => import(`https://cdn.jsdelivr.net/npm/${p}@${v}/+esm`);
    tiptapPromise = Promise.all([
      j('core', '2.10.3'),
      j('starter-kit', '2.10.3'),
      j('extension-image', '2.10.3'),
      j('extension-link', '2.10.3'),
      j('extension-task-list', '2.10.3'),
      j('extension-task-item', '2.10.3'),
      j('extension-table', '2.10.3'),
      j('extension-table-row', '2.10.3'),
      j('extension-table-cell', '2.10.3'),
      j('extension-table-header', '2.10.3'),
      j('extension-placeholder', '2.10.3'),
      jnp('tiptap-markdown', '0.8.10'),
    ]).then(([core, starterMod, image, link, taskList, taskItem, table, tableRow, tableCell, tableHeader, placeholder, md]) => ({
      Editor: core.Editor,
      StarterKit: starterMod.default || starterMod.StarterKit,
      Image: image.default || image.Image,
      Link: link.default || link.Link,
      TaskList: taskList.default || taskList.TaskList,
      TaskItem: taskItem.default || taskItem.TaskItem,
      Table: table.default || table.Table,
      TableRow: tableRow.default || tableRow.TableRow,
      TableCell: tableCell.default || tableCell.TableCell,
      TableHeader: tableHeader.default || tableHeader.TableHeader,
      Placeholder: placeholder.default || placeholder.Placeholder,
      Markdown: md.Markdown || md.default,
    })).catch((err) => {
      // tiptap-markdown sometimes exposes its symbol weirdly through +esm — keep going
      // without it; markdown round-trip will fall back to raw HTML save.
      console.warn('[markdown-pane] partial extension load:', err);
      throw err;
    });
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
      const t = await loadTipTap();
      if (!document.body.contains(pane)) return null; // tab closed during load
      const extensions = [
        t.StarterKit.configure({ codeBlock: { HTMLAttributes: { class: 'md-codeblock' } } }),
        t.Image.configure({ inline: false, allowBase64: true }),
        t.Link.configure({ openOnClick: false, autolink: true, linkOnPaste: true }),
        t.TaskList,
        t.TaskItem.configure({ nested: true }),
        t.Table.configure({ resizable: true }),
        t.TableRow,
        t.TableHeader,
        t.TableCell,
        t.Placeholder.configure({ placeholder: 'Start writing… (`# heading`, `- ` list, `**bold**`, paste images)' }),
      ];
      if (t.Markdown) extensions.push(t.Markdown.configure({ html: true, linkify: true, breaks: false }));

      // If we have content already (passed in opts) and it's markdown source,
      // hand it to the editor as markdown; otherwise use the default HTML.
      const initialContent = (opts.markdown && t.Markdown)
        ? opts.markdown
        : (opts.content || '<h1>New document</h1><p>Start writing…</p>');

      editor = new Editor({
        element: mount,
        extensions,
        content: initialContent,
        autofocus: 'end',
        editorProps: {
          attributes: { class: 'md-prose' },
          handleDrop: (view, event) => handleImageDrop(view, event),
          handlePaste: (view, event) => handleImagePaste(view, event),
        },
        onUpdate: () => markDirty(true),
      });

      // Save the loaded extension bundle on the entry so the save handler
      // can call editor.storage.markdown.getMarkdown() without reloading.
      pane.dataset.tiptapLoaded = '1';
    } catch (e) {
      mount.innerHTML = `<div class="md-load-error">Failed to load TipTap: ${escapeText(String(e))}</div>`;
      console.error('[markdown-pane] failed to load TipTap', e);
      return { kind: 'markdown', label, pane, filePath: null, editor: null };
    }

    // Update the bar's filename display whenever the file path changes.
    const setFilePath = (p) => {
      filePath = p;
      if (filenameEl) {
        if (p) {
          const parts = p.split('/');
          filenameEl.textContent = parts[parts.length - 1] || p;
          filenameEl.title = p;
        } else {
          filenameEl.textContent = 'untitled.md';
          filenameEl.title = 'Unsaved';
        }
      }
    };
    setFilePath(filePath);

    // If we were given a path on open, eagerly load its contents now.
    if (opts.filePath && !opts.markdown && !opts.content) {
      try {
        const text = await invokeRust('read_file', { path: opts.filePath });
        if (editor) {
          if (editor.storage.markdown) editor.commands.setContent(text, false);
          else editor.commands.setContent(text, false);
          markDirty(false);
        }
      } catch (e) {
        console.error('[markdown-pane] failed to open file', e);
      }
    }

    // Save: writes current markdown to filePath (prompts for one if absent).
    async function save() {
      if (!editor) return;
      const text = editor.storage.markdown
        ? editor.storage.markdown.getMarkdown()
        : editor.getHTML(); // fallback if tiptap-markdown failed to load
      let target = filePath;
      if (!target) {
        target = prompt('Save as (absolute path or ~/relative):', '~/Documents/untitled.md');
        if (!target) return;
        setFilePath(target);
      }
      try {
        await invokeRust('write_file', { path: expandHome(target), content: text });
        markDirty(false);
      } catch (e) {
        alert('Save failed: ' + e);
      }
    }

    // Open: load a file's contents into the current editor.
    async function open() {
      const target = prompt('Open file (absolute or ~/path):', filePath || '~/Documents/');
      if (!target) return;
      try {
        const text = await invokeRust('read_file', { path: expandHome(target) });
        if (editor) {
          editor.commands.setContent(text, false);
          setFilePath(target);
          markDirty(false);
        }
      } catch (e) {
        alert('Open failed: ' + e);
      }
    }

    pane.addEventListener('keydown', (e) => {
      const isMod = e.metaKey || e.ctrlKey;
      if (isMod && (e.key === 's' || e.key === 'S')) {
        e.preventDefault();
        save();
      } else if (isMod && (e.key === 'o' || e.key === 'O')) {
        e.preventDefault();
        open();
      }
    });

    bar.querySelector('.md-save').onclick = () => save();
    bar.querySelector('.md-close').onclick = () => destroyMarkdownPane(label);

    // Bubble menu — floats above the text selection with formatting controls.
    // Vanilla DOM, no @tiptap/extension-bubble-menu / floating-ui needed.
    const bubble = buildBubbleMenu(editor);
    pane.appendChild(bubble);
    if (editor) {
      const updateBubble = () => positionBubble(bubble, editor, mount);
      editor.on('selectionUpdate', updateBubble);
      editor.on('blur', () => {
        // hide briefly so click-outside-to-dismiss feels right; reposition on focus
        setTimeout(() => { if (!bubble.contains(document.activeElement)) bubble.hidden = true; }, 80);
      });
      editor.on('focus', updateBubble);
    }

    const entry = { kind: 'markdown', label, pane, filePath, editor, save, open };
    panes.set(label, entry);
    return entry;
  }

  // Invoke wrapper — checks for Tauri presence and falls back to a friendly error.
  async function invokeRust(cmd, args) {
    const inv = window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke;
    if (!inv) throw new Error('Tauri API not available');
    return inv(cmd, args);
  }

  // Expand ~/ to the user's home directory (lazy, cached).
  let cachedHome = null;
  function expandHome(p) {
    if (!p) return p;
    if (!p.startsWith('~')) return p;
    if (cachedHome) return cachedHome + p.slice(1);
    // Best-effort sync expansion: pull from env via existing get_home_directory.
    // If it's not pre-cached we just return as-is; user can retype with full path.
    try {
      invokeRust('get_home_directory', {}).then((h) => { cachedHome = h; }).catch(() => {});
    } catch (e) {}
    return p;
  }

  // Drag-and-drop image handler: read the dropped file as data URL and insert.
  function handleImageDrop(view, event) {
    const file = event.dataTransfer && event.dataTransfer.files && event.dataTransfer.files[0];
    if (!file || !file.type.startsWith('image/')) return false;
    event.preventDefault();
    const reader = new FileReader();
    reader.onload = () => {
      const { schema } = view.state;
      const node = schema.nodes.image.create({ src: reader.result, alt: file.name });
      const transaction = view.state.tr.replaceSelectionWith(node);
      view.dispatch(transaction);
    };
    reader.readAsDataURL(file);
    return true;
  }

  // Bubble menu — builds a floating toolbar attached to the pane. Hidden until
  // the editor has a non-empty text selection. Buttons run TipTap commands.
  function buildBubbleMenu(editor) {
    const wrap = document.createElement('div');
    wrap.className = 'md-bubble';
    wrap.hidden = true;
    const mk = (label, title, run) => {
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.className = 'md-bubble-btn';
      btn.title = title;
      btn.innerHTML = label;
      btn.addEventListener('mousedown', (e) => {
        // mousedown not click — prevents the editor losing selection
        e.preventDefault();
        if (!editor) return;
        run(editor.chain().focus());
      });
      return btn;
    };
    wrap.appendChild(mk('<b>B</b>', 'Bold (⌘B)', (c) => c.toggleBold().run()));
    wrap.appendChild(mk('<i>i</i>', 'Italic (⌘I)', (c) => c.toggleItalic().run()));
    wrap.appendChild(mk('<s>S</s>', 'Strikethrough', (c) => c.toggleStrike().run()));
    wrap.appendChild(mk('&lt;/&gt;', 'Inline code', (c) => c.toggleCode().run()));
    wrap.appendChild(document.createElement('span')).className = 'md-bubble-sep';
    wrap.appendChild(mk('H1', 'Heading 1', (c) => c.toggleHeading({ level: 1 }).run()));
    wrap.appendChild(mk('H2', 'Heading 2', (c) => c.toggleHeading({ level: 2 }).run()));
    wrap.appendChild(mk('H3', 'Heading 3', (c) => c.toggleHeading({ level: 3 }).run()));
    wrap.appendChild(document.createElement('span')).className = 'md-bubble-sep';
    wrap.appendChild(mk('• list', 'Bullet list', (c) => c.toggleBulletList().run()));
    wrap.appendChild(mk('1. list', 'Ordered list', (c) => c.toggleOrderedList().run()));
    wrap.appendChild(mk('☐', 'Task list', (c) => c.toggleTaskList().run()));
    wrap.appendChild(mk('❝', 'Blockquote', (c) => c.toggleBlockquote().run()));
    wrap.appendChild(document.createElement('span')).className = 'md-bubble-sep';
    wrap.appendChild(mk('🔗', 'Link', (c) => {
      const url = prompt('URL:', 'https://');
      if (url) c.setLink({ href: url }).run();
      else c.unsetLink().run();
    }));
    return wrap;
  }

  function positionBubble(wrap, editor, mount) {
    if (!editor) return;
    const { from, to, empty } = editor.state.selection;
    if (empty) { wrap.hidden = true; return; }
    // Coordinates of selection in viewport
    const start = editor.view.coordsAtPos(from);
    const end = editor.view.coordsAtPos(to);
    const left = (start.left + end.left) / 2;
    const top = Math.min(start.top, end.top);
    // Position relative to the .md-mount container (offset parent of the bubble)
    const mountRect = mount.getBoundingClientRect();
    wrap.hidden = false;
    wrap.style.left = (left - mountRect.left) + 'px';
    wrap.style.top = (top - mountRect.top - wrap.offsetHeight - 8) + 'px';
  }

  // Clipboard paste handler: same as drop, for images on the clipboard.
  function handleImagePaste(view, event) {
    const items = event.clipboardData && event.clipboardData.items;
    if (!items) return false;
    for (const item of items) {
      if (item.type && item.type.startsWith('image/')) {
        const file = item.getAsFile();
        if (!file) continue;
        event.preventDefault();
        const reader = new FileReader();
        reader.onload = () => {
          const { schema } = view.state;
          const node = schema.nodes.image.create({ src: reader.result });
          view.dispatch(view.state.tr.replaceSelectionWith(node));
        };
        reader.readAsDataURL(file);
        return true;
      }
    }
    return false;
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
