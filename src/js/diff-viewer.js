// Diff viewer — singleton full-area overlay rendering side-by-side diffs.
//
// Same architecture as markdown-pane.js: IIFE + 'use strict', window.xnaut*
// exports, lazy ESM import from jsdelivr (diff2html here, TipTap there).
// Not a tab on purpose: a fixed overlay keeps integration zero-touch.
(function () {
  'use strict';

  const invoke = (...a) => window.__TAURI__.core.invoke(...a);

  function escapeText(s) {
    return String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
  }

  // Lazy diff2html loader — cached module promise, same pattern as the
  // TipTap loader in markdown-pane.js.
  let diff2htmlPromise = null;
  function loadDiff2Html() {
    if (diff2htmlPromise) return diff2htmlPromise;
    diff2htmlPromise = import('https://cdn.jsdelivr.net/npm/diff2html@3.4.48/+esm');
    return diff2htmlPromise;
  }

  function injectDiff2HtmlCss() {
    if (document.getElementById('diff2html-css')) return;
    const link = document.createElement('link');
    link.id = 'diff2html-css';
    link.rel = 'stylesheet';
    link.href = 'https://cdn.jsdelivr.net/npm/diff2html@3.4.48/bundles/css/diff2html.min.css';
    document.head.appendChild(link);
  }

  // Overlay chrome + scoped dark-theme overrides for diff2html so the
  // viewer doesn't glow white inside a dark app.
  function injectStyles() {
    if (document.getElementById('diff-viewer-styles')) return;
    const st = document.createElement('style');
    st.id = 'diff-viewer-styles';
    st.textContent = `
      .dv-overlay{position:fixed;inset:40px;z-index:100;display:flex;flex-direction:column;
        background:var(--editor-surface,#1e1e1e);color:var(--text-color,#ddd);
        border:1px solid #333;border-radius:8px;box-shadow:0 12px 48px rgba(0,0,0,.6);overflow:hidden}
      .dv-header{display:flex;align-items:center;gap:8px;padding:8px 12px;border-bottom:1px solid #333;flex:0 0 auto}
      .dv-title{flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-family:monospace;font-size:13px}
      .dv-close{border:none;background:transparent;color:var(--text-color,#ddd);font-size:18px;line-height:1;cursor:pointer;padding:2px 8px;border-radius:4px}
      .dv-close:hover{background:#333}
      .dv-body{flex:1;min-height:0;overflow:auto;padding:8px}
      .dv-msg{padding:24px;text-align:center;color:#888}
      .dv-msg.err{color:var(--destructive,#ff6568)}
      .dv-pre{font-family:monospace;font-size:12px;white-space:pre;line-height:1.45;margin:0}
      .dv-pre .dv-add{color:#7ee787;background:rgba(0,255,128,.12);display:block}
      .dv-pre .dv-del{color:#ff7b72;background:rgba(255,64,64,.12);display:block}
      .dv-pre .dv-hunk{color:#79c0ff;display:block}
      .dv-pre .dv-ctx{display:block}
      /* diff2html dark overrides, scoped to the overlay */
      .dv-overlay .d2h-wrapper,.dv-overlay .d2h-file-wrapper{background:var(--editor-surface,#1e1e1e);border-color:#333;color:var(--text-color,#ddd)}
      .dv-overlay .d2h-file-header{background:var(--editor-surface,#1e1e1e);border-bottom:1px solid #333;color:var(--text-color,#ddd)}
      .dv-overlay .d2h-file-name{color:var(--text-color,#ddd)}
      .dv-overlay .d2h-code-line,.dv-overlay .d2h-code-side-line{background:transparent;color:var(--text-color,#ddd)}
      .dv-overlay .d2h-code-linenumber,.dv-overlay .d2h-code-side-linenumber{background:var(--editor-surface,#1e1e1e);border-color:#333;color:#777}
      .dv-overlay .d2h-diff-table{color:var(--text-color,#ddd)}
      .dv-overlay .d2h-diff-tbody tr td{border-color:#333}
      .dv-overlay .d2h-ins{background:rgba(0,255,128,.12)}
      .dv-overlay .d2h-del{background:rgba(255,64,64,.12)}
      .dv-overlay ins,.dv-overlay .d2h-ins ins{background:rgba(0,255,128,.25);color:inherit;text-decoration:none}
      .dv-overlay del,.dv-overlay .d2h-del del{background:rgba(255,64,64,.25);color:inherit;text-decoration:none}
      .dv-overlay .d2h-info{background:transparent;color:#79c0ff;border-color:#333}
      .dv-overlay .d2h-emptyplaceholder,.dv-overlay .d2h-code-side-emptyplaceholder{background:rgba(255,255,255,.03);border-color:#333}
      .dv-overlay .d2h-tag{background:transparent;border-color:#333;color:#aaa}
    `;
    document.head.appendChild(st);
  }

  let overlayEl = null;

  function onKeyDown(e) {
    if (e.key === 'Escape') { e.preventDefault(); closeDiff(); }
  }

  function closeDiff() {
    if (!overlayEl) return;
    overlayEl.remove();
    overlayEl = null;
    document.removeEventListener('keydown', onKeyDown, true);
  }

  function renderFallbackPre(body, diffString) {
    const pre = document.createElement('pre');
    pre.className = 'dv-pre';
    pre.innerHTML = diffString.split('\n').map((line) => {
      let cls = 'dv-ctx';
      if (line.startsWith('+++') || line.startsWith('---')) cls = 'dv-hunk';
      else if (line.startsWith('@@')) cls = 'dv-hunk';
      else if (line.startsWith('+')) cls = 'dv-add';
      else if (line.startsWith('-')) cls = 'dv-del';
      return `<span class="${cls}">${escapeText(line) || ' '}</span>`;
    }).join('');
    body.innerHTML = '';
    body.appendChild(pre);
  }

  async function openDiff(opts) {
    opts = opts || {};
    const { repo, path, staged, outgoing, title } = opts;
    injectStyles();
    closeDiff(); // singleton — replace any existing overlay

    overlayEl = document.createElement('div');
    overlayEl.className = 'dv-overlay';
    overlayEl.innerHTML = `
      <div class="dv-header">
        <span class="dv-title">${escapeText(title || path || 'Diff')}</span>
        <button class="dv-close" title="Close (Esc)" aria-label="Close diff viewer">&times;</button>
      </div>
      <div class="dv-body"><div class="dv-msg">Loading diff…</div></div>`;
    overlayEl.querySelector('.dv-close').onclick = closeDiff;
    document.body.appendChild(overlayEl);
    document.addEventListener('keydown', onKeyDown, true);

    const body = overlayEl.querySelector('.dv-body');
    let diffString = '';
    try {
      diffString = await invoke('git_file_diff', { repo, path, staged: !!staged, outgoing: !!outgoing });
    } catch (e) {
      if (body && overlayEl) body.innerHTML = `<div class="dv-msg err">Failed to load diff: ${escapeText(String(e))}</div>`;
      return;
    }
    if (!overlayEl || !document.body.contains(overlayEl)) return; // closed during load

    if (!diffString || !diffString.trim()) {
      body.innerHTML = `<div class="dv-msg">No changes for ${escapeText(path || '')}</div>`;
      return;
    }

    try {
      injectDiff2HtmlCss();
      const mod = await loadDiff2Html();
      if (!overlayEl || !document.body.contains(overlayEl)) return;
      const htmlFn = mod.html || (mod.Diff2Html && mod.Diff2Html.html) || (mod.default && mod.default.html);
      if (typeof htmlFn !== 'function') throw new Error('diff2html: html() export not found');
      body.innerHTML = htmlFn(diffString, {
        drawFileList: false,
        matching: 'lines',
        outputFormat: 'side-by-side',
      });
    } catch (e) {
      console.error('[diff-viewer] diff2html failed, falling back to <pre>', e);
      if (overlayEl && document.body.contains(overlayEl)) renderFallbackPre(body, diffString);
    }
  }

  window.xnautOpenDiff = openDiff;
  window.xnautCloseDiff = closeDiff;
})();
