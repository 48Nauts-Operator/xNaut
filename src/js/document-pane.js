// The agent's document, beside the conversation.
//
// The other half of Cockpit's split screen (`src/components/research-screen.tsx`):
// Preview | Code, a title, and the buttons that get the thing out of the app.
// Cockpit renders with react-markdown; `marked` is already loaded here, so this
// uses that.
//
// "Save to vault" is ours, and it is the point: a document that only exists
// inside the app is a document that gets lost. It lands in the work vault under
// the project, with the frontmatter every doc there carries.
(function () {
  'use strict';

  const invoke = (...args) => window.__TAURI__.core.invoke(...args);
  const esc = (value) => String(value == null ? '' : value).replace(/[&<>"']/g, (c) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  }[c]));

  function ensureStyles() {
    if (document.getElementById('document-pane-styles')) return;
    const style = document.createElement('style');
    style.id = 'document-pane-styles';
    style.textContent = `
      .doc { display:flex; flex-direction:column; height:100%; min-height:0; background:var(--bg-primary,#0a0a0f); }
      .doc-bar { display:flex; align-items:center; gap:8px; padding:9px 12px; border-bottom:1px solid var(--border,#26262c); }
      .doc-toggle { display:flex; gap:2px; padding:2px; border-radius:8px; background:rgba(255,255,255,.05); }
      .doc-toggle button { padding:4px 11px; border:0; border-radius:6px; background:transparent;
        color:var(--text-secondary,#8a8a94); font:inherit; font-size:11px; cursor:pointer; }
      .doc-toggle button.on { background:rgba(255,255,255,.09); color:var(--text-primary,#e8e8ec); }
      .doc-title { flex:1; min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;
        color:var(--text-primary,#e8e8ec); font-size:12px; font-weight:600; }
      .doc-btn { padding:4px 10px; border:1px solid var(--border,#2a2a2f); border-radius:7px; background:transparent;
        color:var(--text-secondary,#a0a0a0); font:inherit; font-size:11px; cursor:pointer; white-space:nowrap; }
      .doc-btn:hover { color:var(--text-primary,#e8e8ec); }
      .doc-btn.primary { border-color:var(--as-accent,#f5b840); color:var(--as-accent,#f5b840); }
      .doc-body { flex:1 1 auto; min-height:0; overflow-y:auto; padding:26px 30px 40px; }
      .doc-view { max-width:720px; margin:0 auto; color:var(--text-primary,#e4e4e9); font-size:14px; line-height:1.72; }
      .doc-view h1 { font-size:26px; margin:0 0 16px; } .doc-view h2 { font-size:19px; margin:28px 0 10px; }
      .doc-view h3 { font-size:15px; margin:22px 0 8px; }
      .doc-view p, .doc-view li { color:var(--text-secondary,#b4b4bd); }
      .doc-view pre { padding:12px 14px; overflow-x:auto; border:1px solid var(--border,#26262c); border-radius:9px;
        background:#0d0e11; font-family:var(--font-mono,monospace); font-size:12px; }
      .doc-view code { font-family:var(--font-mono,monospace); font-size:12px; }
      .doc-view table { width:100%; border-collapse:collapse; font-size:12px; }
      .doc-view th, .doc-view td { padding:7px 9px; border:1px solid var(--border,#26262c); text-align:left; }
      .doc-view blockquote { margin:14px 0; padding:2px 0 2px 14px; border-left:2px solid var(--as-accent,#f5b840);
        color:var(--text-secondary,#a0a0aa); }
      .doc-code { width:100%; height:100%; min-height:320px; padding:16px 18px; resize:none;
        border:1px solid var(--border,#26262c); border-radius:10px; background:#0d0e11;
        color:#c9d1d9; font-family:var(--font-mono,monospace); font-size:12px; line-height:1.6; }
      .doc-empty { margin:auto; padding:40px; text-align:center; color:var(--text-secondary,#8a8a94); font-size:12px; }
      .doc-saved { color:#4ade80; font-size:11px; }
    `;
    document.head.appendChild(style);
  }

  function createDocumentPane(key, parent, options) {
    ensureStyles();
    const pane = document.createElement('section');
    pane.className = 'doc';
    parent.appendChild(pane);

    let doc = { title: '', content: '' };
    let view = 'preview';
    let saved = '';

    async function load() {
      doc = (await invoke('document_get', { key }).catch(() => null)) || { title: '', content: '' };
      render();
    }

    function bodyMarkup() {
      if (!String(doc.content || '').trim()) {
        return '<div class="doc-empty">Nothing written yet. Ask the agent for a report, a spec or release notes and it appears here.</div>';
      }
      if (view === 'code') {
        return `<textarea class="doc-code" data-code spellcheck="false">${esc(doc.content)}</textarea>`;
      }
      const html = typeof marked !== 'undefined'
        ? marked.parse(doc.content)
        : `<pre>${esc(doc.content)}</pre>`;
      return `<div class="doc-view">${html}</div>`;
    }

    function render() {
      pane.innerHTML = `<div class="doc-bar">
          <span class="doc-toggle">
            <button data-view="preview" class="${view === 'preview' ? 'on' : ''}">Preview</button>
            <button data-view="code" class="${view === 'code' ? 'on' : ''}">Code</button>
          </span>
          <span class="doc-title">${esc(doc.title || 'Document')}</span>
          ${saved ? `<span class="doc-saved" title="${esc(saved)}">saved</span>` : ''}
          <button class="doc-btn" data-copy>Copy</button>
          <button class="doc-btn primary" data-vault>Save to vault</button>
          ${options && options.onFullScreen ? '<button class="doc-btn" data-full>Full screen</button>' : ''}
          ${options && options.onClose ? '<button class="doc-btn" data-close aria-label="Close document">✕</button>' : ''}
        </div>
        <div class="doc-body">${bodyMarkup()}</div>`;
      wire();
    }

    function wire() {
      pane.querySelectorAll('[data-view]').forEach((button) => {
        button.onclick = () => { view = button.dataset.view; render(); };
      });
      pane.querySelector('[data-copy]').onclick = () => {
        if (navigator.clipboard) navigator.clipboard.writeText(doc.content || '');
      };
      pane.querySelector('[data-vault]').onclick = async () => {
        try {
          const path = await invoke('document_save_to_vault', {
            key,
            project: (options && options.project) || 'xNAUT',
            author: (options && options.author) || 'xNAUT agent',
          });
          saved = path;
          render();
        } catch (error) { alert(String(error)); }
      };
      const full = pane.querySelector('[data-full]');
      if (full) full.onclick = () => options.onFullScreen();
      const close = pane.querySelector('[data-close]');
      if (close) close.onclick = () => options.onClose();
      // Editing in Code writes straight back: the document is his to change,
      // and the agent reads what is actually there on its next turn.
      const code = pane.querySelector('[data-code]');
      if (code) {
        let timer = null;
        code.oninput = () => {
          doc.content = code.value;
          clearTimeout(timer);
          timer = setTimeout(() => {
            invoke('document_set', { key, document: { ...doc, previous: null } }).catch(() => {});
          }, 600);
        };
      }
    }

    const changed = window.__TAURI__.event.listen('document-changed', (event) => {
      if (!event || !event.payload || event.payload.key !== key) return;
      load();
    });

    load();
    return {
      kind: 'document',
      label: `document-${key}`,
      pane,
      dispose() {
        Promise.resolve(changed).then((off) => { try { off(); } catch (_) {} }).catch(() => {});
      },
    };
  }

  window.xnautCreateDocumentPane = createDocumentPane;
})();
