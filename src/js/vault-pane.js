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
.vp-column-resize { flex:0 0 5px; width:5px; cursor:col-resize; background:var(--border-color,#333); position:relative; z-index:3; }
.vp-column-resize:hover,.vp-column-resize[data-active="1"] { background:var(--accent,#4f8cff); }
.vp-file-pane { display:flex; flex:0 0 230px; min-width:0; min-height:0; overflow:hidden; background:var(--bg-secondary,#15171c); }
.vp-main { display:flex; flex-direction:column; flex:1 1 0%; min-width:420px; min-height:0; overflow:hidden; }
.vp-chat-section { display:flex; flex-direction:column; flex:0 0 auto; min-height:0; overflow:hidden; }
.vp-row-resize { display:none; }
.vp-row-resize:hover,.vp-row-resize[data-active="1"] { background:var(--accent,#4f8cff); }
.vp-chat-controls { display:flex; align-items:center; gap:5px; flex:1 1 auto; min-width:0; order:2; }
.vp-chat-controls select { min-width:0; height:28px; border:0; border-radius:6px; background:rgba(255,255,255,.045); color:var(--text-secondary,#aaa); padding:0 6px; font:inherit; font-size:10px; }
.vp-chat-agent { flex:0 1 120px; }.vp-chat-model { flex:0 1 165px; }
.vp-chat-section .chatp-input-area { flex-wrap:wrap; gap:6px; padding:9px 10px; }
.vp-chat-section .chatp-bar { display:none; }
.vp-chat-section .chatp-list { flex:0 1 auto; max-height:180px; min-height:0; padding:8px 10px; overflow:auto; }
.vp-chat-section .chatp-list:empty { display:none; }
.vp-chat-section .chatp-pane,.vp-chat-section .chatp-input-area { height:auto !important; min-height:0; }
.vp-chat-section .chatp-input-area { position:relative; }
.vp-chat-section .chatp-input { flex:1 1 100%; order:1; height:54px; min-height:54px; max-height:54px; padding-right:76px; }
/* Mic + send both live inside the message box, bottom-right, like a standard composer. */
.vp-chat-section .chatp-dictate { position:absolute; right:44px; top:18px; z-index:2; order:0; }
.vp-chat-section .chatp-send { position:absolute; right:12px; top:18px; z-index:2; order:0; }
.vp-upload { width:28px; height:28px; display:flex; align-items:center; justify-content:center; border:1px solid var(--border-color,#333); border-radius:6px; background:rgba(255,255,255,.04); color:var(--text-secondary,#aaa); cursor:pointer; }
.vp-upload:hover { color:var(--text-primary,#fff); background:rgba(255,255,255,.08); }
.vp-upload input { display:none; }
.vp-run-detail .vp-chat-section { flex:1 1 0%; height:100%; }
.vp-run-detail .vp-chat-section .chatp-pane { height:100% !important; }
.vp-run-detail .vp-chat-section .chatp-list { display:flex; flex:1 1 0%; max-height:none; }
.vp-run-detail .vp-chat-section .chatp-list:empty { display:flex; }
.vp-run-detail { display:flex; flex-direction:column; flex:0 0 390px; min-width:0; min-height:0; overflow:hidden; background:var(--bg-secondary,#15171c); }
.vp-run-head { display:flex; align-items:center; gap:8px; flex:0 0 auto; padding:8px 10px; border-bottom:1px solid var(--border-color,#333); font-size:12px; font-weight:650; }
.vp-run-heading { flex:1; min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.vp-run-switch { display:flex; gap:2px; padding:2px; border:1px solid var(--border-color,#333); border-radius:7px; }
.vp-run-switch button { border:0; border-radius:5px; padding:4px 7px; background:transparent; color:var(--text-muted,#777); font:10px var(--font-mono,monospace); cursor:pointer; }
.vp-run-switch button[data-active="1"] { background:rgba(255,255,255,.09); color:var(--text-primary,#eee); }
.vp-run-body { flex:1; min-height:0; overflow:auto; padding:8px; color:var(--text-secondary,#aaa); font-size:12px; line-height:1.55; }
.vp-ticket { border-bottom:1px solid var(--border-color,#333); }
.vp-ticket summary { display:flex; align-items:flex-start; gap:8px; padding:10px 7px; cursor:pointer; list-style:none; }
.vp-ticket summary::-webkit-details-marker { display:none; }
.vp-ticket-caret { color:var(--text-muted,#777); width:10px; flex:0 0 10px; }
.vp-ticket[open] .vp-ticket-caret { transform:rotate(90deg); }
.vp-ticket-main { flex:1; min-width:0; }
.vp-ticket-id { color:var(--xnaut-yellow,#f5b840); font:10px var(--font-mono,monospace); }
.vp-ticket-title { color:var(--text-primary,#eee); font-weight:600; margin-top:2px; }
/* Fixed columns, not content-sized, or the right edge is ragged: "+4786 -432"
   is four times the width of "+0 -0", and REVIEW is wider than DONE, so every
   row landed its numbers and its badge somewhere different (2026-09-13).
   No backticks in here: this block is a JS template literal, and one closes
   the string without making the file invalid, so node --check stays quiet and
   the pane throws at runtime instead. */
.vp-ticket-status { flex:0 0 58px; text-align:center; padding:3px 0; border:1px solid currentColor; border-radius:999px; color:var(--text-muted,#777); font:9px var(--font-mono,monospace); text-transform:uppercase; }
.vp-ticket-status[data-status="done"] { color:#76c893; }.vp-ticket-status[data-status="complete"] { color:#10b981; }.vp-ticket-status[data-status="blocked"] { color:#ef6f6c; }
.vp-ticket-status[data-status="in_progress"],.vp-ticket-status[data-status="review"] { color:var(--xnaut-yellow,#f5b840); }
.vp-ticket-status[data-status="ready"] { color:#73a9ff; }
.vp-ticket-stats { flex:0 0 86px; display:flex; justify-content:flex-end; gap:5px; border:0; border-radius:5px;
  padding:3px 6px; background:rgba(255,255,255,.055); font:9px var(--font-mono,monospace);
  font-variant-numeric:tabular-nums; cursor:pointer; }
.vp-ticket-stats:hover { background:rgba(255,255,255,.1); }
.vp-ticket-add { flex:1 1 0; text-align:right; color:#76c893; }
.vp-ticket-del { flex:1 1 0; text-align:right; color:#ef8b85; }
.vp-ticket-group { padding:8px 12px 4px; font-size:11px; letter-spacing:.06em; text-transform:uppercase;
  color:var(--text-muted,#7e838d); }
.vp-ticket-rest > summary { padding:8px 12px; cursor:pointer; font-size:12px; color:var(--text-secondary,#9aa0ac);
  border-top:1px solid var(--border-color,#2a2c33); margin-top:6px; }
.vp-ticket-rest > summary:hover { color:var(--text-primary,#e7eaf0); }
.vp-ticket-text { padding:2px 25px 14px; color:var(--text-secondary,#aaa); overflow-wrap:anywhere; }
.vp-ticket-text.xnaut-md h1,.vp-ticket-text.xnaut-md h2,.vp-ticket-text.xnaut-md h3 { margin-top:14px; }
.vp-ticket-text.xnaut-md h2 { padding:6px 8px; border-left:3px solid var(--xnaut-yellow,#f5b840); background:rgba(245,184,64,.07);
  font-size:12.5px; letter-spacing:.04em; text-transform:uppercase; color:var(--text-primary,#e7eaf0); }
.vp-ticket-text.xnaut-md h3 { margin:12px 0 4px; padding-left:8px; border-left:2px solid var(--border-color,#3a3d45);
  font-size:12px; letter-spacing:.03em; text-transform:uppercase; color:var(--text-secondary,#9aa0ac); }
.vp-ticket-text.xnaut-md p { margin:6px 0; line-height:1.6; }
.vp-ticket-text.xnaut-md ul, .vp-ticket-text.xnaut-md ol { margin:6px 0; padding-left:20px; line-height:1.6; }
.vp-code-file { display:flex; gap:8px; align-items:center; width:100%; border:0; border-bottom:1px solid var(--border-color,#333); padding:9px 7px; background:transparent; color:var(--text-secondary,#aaa); cursor:pointer; text-align:left; }
.vp-code-file:hover { background:rgba(255,255,255,.05); }.vp-code-path { flex:1; min-width:0; overflow:hidden; text-overflow:ellipsis; display:flex; gap:7px; align-items:baseline; }
.vp-code-file[data-active="1"] { background:rgba(245,184,64,.09); }
.vp-code-base { color:var(--text-primary,#eee); white-space:nowrap; }
.vp-code-dir { color:var(--text-muted,#777); font-size:10px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.vp-code-status { flex:0 0 16px; text-align:center; font:10px var(--font-mono,monospace); border-radius:3px; padding:1px 0; color:#0c0c0c; }
.vp-code-status[data-s="M"] { background:#d9a441; } .vp-code-status[data-s="A"],.vp-code-status[data-s="?"] { background:#4a9d5b; color:#eee; } .vp-code-status[data-s="D"] { background:#c0554d; color:#eee; } .vp-code-status[data-s="R"] { background:#5a8bd6; color:#eee; }
.vp-changes-head { display:flex; align-items:center; gap:7px; padding:12px 8px 5px; color:var(--text-muted,#777); font:9px var(--font-mono,monospace); letter-spacing:.12em; text-transform:uppercase; }
.vp-changes-count { padding:0 6px; border-radius:999px; background:rgba(255,255,255,.08); color:var(--text-secondary,#aaa); font-size:9px; }
.vp-wt-row { display:flex; align-items:center; gap:7px; min-height:30px; padding:0 8px; cursor:pointer; color:var(--text-secondary,#aaa); font-size:12px; border-bottom:1px solid var(--border-color,#2a2c33); }
.vp-wt-row:hover { background:rgba(255,255,255,.05); color:var(--text-primary,#eee); }
.vp-wt-branch { flex:1; min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.vp-wt-head { color:var(--text-muted,#666); font:10px var(--font-mono,monospace); }
.vp-grp-head { display:flex; align-items:center; gap:6px; padding:6px 8px; cursor:pointer; color:var(--text-secondary,#9aa); font:11px var(--font-mono,monospace); background:rgba(255,255,255,.02); }
.vp-grp-head:hover { background:rgba(255,255,255,.05); }
.vp-grp-dir { flex:1; min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.vp-commit-row { display:flex; align-items:center; gap:8px; width:100%; border:0; border-bottom:1px solid var(--border-color,#2a2c33); padding:8px 8px; background:transparent; color:var(--text-secondary,#aaa); cursor:pointer; text-align:left; }
.vp-commit-row:hover { background:rgba(255,255,255,.05); } .vp-commit-row[data-active="1"] { background:rgba(245,184,64,.09); }
.vp-commit-sha { flex:0 0 auto; color:var(--xnaut-yellow,#f5b840); font:10px var(--font-mono,monospace); }
.vp-commit-subject { flex:1; min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; color:var(--text-primary,#ddd); }
.vp-commit-tag { flex:0 0 auto; padding:1px 6px; border-radius:999px; background:rgba(118,200,147,.18); color:#8fdca6; font:9px var(--font-mono,monospace); }
.vp-commit-date { flex:0 0 auto; color:var(--text-muted,#666); font:10px var(--font-mono,monospace); }
/* center viewer: code + diff */
.vp-center-viewer { }
.vp-cv-bar { display:flex; align-items:center; gap:8px; padding:8px 12px; border-bottom:1px solid var(--border-color,#333); font-size:12px; color:var(--text-muted,#8a8f98); flex-shrink:0; }
.vp-cv-path { flex:1 1 auto; min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; font-family:var(--font-mono,monospace); }
.vp-cv-modes { display:flex; gap:6px; }
.vp-cv-open, .vp-cv-close { background:transparent; border:1px solid var(--border-color,#333); border-radius:5px; color:var(--text-secondary,#aaa); font:11px inherit; padding:3px 9px; cursor:pointer; }
.vp-cv-open:hover, .vp-cv-close:hover { background:rgba(255,255,255,.06); color:var(--text-primary,#eee); }
.vp-cv-body { flex:1 1 0%; min-height:0; overflow:auto; }
.vp-cv-code code { display:block; }
/* file tree */
.vp-ftree-row { display:flex; align-items:center; gap:7px; min-height:30px; padding-right:8px; cursor:pointer; color:var(--text-secondary,#b4b8c0); font-size:13px; border-radius:5px; }
.vp-ftree-row:hover { background:rgba(255,255,255,.06); color:var(--text-primary,#eee); }
.vp-ftree-row[data-active="1"] { background:rgba(245,184,64,.12); color:var(--text-primary,#eee); box-shadow:inset 2px 0 var(--xnaut-yellow,#f5b840); }
.vp-ftree-caret { flex:0 0 10px; text-align:center; color:var(--text-muted,#6c6f78); font-size:8px; }
.vp-ftree-icon { flex:0 0 15px; display:flex; align-items:center; justify-content:center; }
.vp-ftree-row[data-dir="1"] .vp-ftree-icon { color:var(--text-secondary,#c4c8d0); }
.vp-ftree-row[data-dir="0"] .vp-ftree-icon { color:var(--text-muted,#6f727b); }
.vp-ftree-name { flex:1; min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.vp-ftree-row[data-dir="1"] .vp-ftree-name { color:var(--text-primary,#e7e7eb); font-weight:560; }
.vp-ticket-meta { margin-top:10px; color:var(--text-muted,#777); font-size:10px; }
.vp-master { display:flex; flex:1; min-height:0; flex-direction:column; background:var(--editor-surface,#1b1d23); color:var(--text-primary,#eee); }
.vp-master-title { padding:12px 14px; font-size:13px; font-weight:700; }
.vp-master-label { padding:16px 14px 5px; color:var(--text-muted,#777); font:9px var(--font-mono,monospace); letter-spacing:.12em; text-transform:uppercase; }
.vp-master-row { display:flex; align-items:center; gap:9px; min-height:30px; padding:0 14px; color:var(--text-secondary,#aaa); cursor:pointer; font-size:12px; }
.vp-master-row:hover { background:rgba(255,255,255,.05); color:var(--text-primary,#eee); }
.vp-master-row[data-active="1"] { background:rgba(245,184,64,.07); box-shadow:inset 2px 0 var(--xnaut-yellow,#f5b840); color:var(--text-primary,#eee); }
.vp-master-icon { width:12px; flex:0 0 12px; color:var(--xnaut-yellow,#f5b840); text-align:center; }
.vp-master-name { flex:1; min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.vp-master-count { color:var(--text-muted,#777); font:9px var(--font-mono,monospace); }
.vp-master-spacer { flex:1; }
.vp-master-new { border-top:1px solid var(--border-color,#333); min-height:40px; }
.vp-rail-head { display:flex; align-items:center; gap:6px; padding:8px; border-bottom:1px solid var(--border-color,#333); }
.vp-rail[data-collapsed="1"] .vp-vault,.vp-rail[data-collapsed="1"] .vp-scope,.vp-rail[data-collapsed="1"] .vp-body,.vp-rail[data-collapsed="1"] .vp-status,.vp-rail[data-collapsed="1"] .vp-create-panel { display:none; }
.vp-rail[data-collapsed="1"] .vp-rail-head { flex-direction:column; }
.vp-rail[data-collapsed="1"] .vp-tabs { flex-direction:column; align-items:center; border-bottom:0; }
.vp-rail[data-collapsed="1"] .vp-tabs button { width:40px; flex:0 0 34px; font-size:0; border-bottom:0; border-left:2px solid transparent; }
.vp-rail[data-collapsed="1"] .vp-tabs button::first-letter { font-size:12px; }
.vp-rail[data-collapsed="1"] .vp-tabs button[data-active="1"] { border-left-color:var(--accent,#4f8cff); }
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
.vp-filter { padding:6px; border-bottom:1px solid var(--border-color,#333); }
.vp-filter[hidden] { display:none; }
.vp-filter input { width:100%; box-sizing:border-box; background:var(--input-bg,rgba(255,255,255,.06)); color:inherit; border:1px solid var(--border-color,#333); border-radius:6px; padding:4px 7px; font:inherit; font-size:11px; }
.vp-rail[data-collapsed="1"] .vp-filter { display:none; }
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

  // stem_key comes from the backend tree; fall back to the filename so a note
  // without one filters instead of throwing.
  function noteNameKey(n) {
    return n.stem_key || n.rel.split('/').pop().replace(/\.[^.]+$/, '').toLowerCase();
  }

  // Every folder on the path to a matched note, so a filtered tree shows the
  // hits in context without dragging along folders that matched nothing.
  function ancestorDirs(notes) {
    const out = new Set();
    (notes || []).forEach((n) => {
      const segs = n.rel.split('/').slice(0, -1);
      segs.forEach((_, i) => out.add(segs.slice(0, i + 1).join('/')));
    });
    return [...out];
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
    // Date from a leading YYYY-MM-DD in the filename (dated docs); '' otherwise.
    const dateOf = (n) => (String(n.rel).split('/').pop().match(/^(\d{4}-\d{2}-\d{2})/) || [, ''])[1];
    // Sort by date descending (newest first); undated notes fall to the bottom, alpha.
    node.notes.sort((a, b) => {
      const da = dateOf(a), db = dateOf(b);
      if (da && db) return db.localeCompare(da);
      if (da) return -1;
      if (db) return 1;
      return a.title.localeCompare(b.title);
    }).forEach((n) => {
      const row = document.createElement('div');
      row.className = 'vp-note-row vp-tree-label';
      row.title = n.rel;
      row.dataset.rel = n.rel;
      if (n.rel === activeRel) row.dataset.active = '1';
      const label = document.createElement('span');
      label.className = 'vp-row-main';
      const d = dateOf(n);
      // Strip a redundant leading date from the H1 title so it isn't shown twice.
      const cleanTitle = n.title.replace(/^\d{4}-\d{2}-\d{2}[_ ]+/, '');
      label.textContent = d ? d + '  ' + cleanTitle : cleanTitle;
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
    const scopePrefix = String(opts.scopePrefix || '').replace(/^\/+|\/+$/g, '');
    let scoped = Boolean(scopePrefix);

    const row = document.createElement('div');
    row.style.cssText = 'display:flex; flex:1 1 0%; width:100%; height:100%; min-width:0; min-height:0; overflow:hidden;';
    const rail = document.createElement('div');
    rail.className = 'vp-rail';
    const vaultMenu = document.createElement('div');
    vaultMenu.className = 'vp-master';
    const main = document.createElement('div');
    main.className = 'vp-main';
    const chatSection = document.createElement('div');
    chatSection.className = 'vp-chat-section';
    const chatControls = document.createElement('div');
    chatControls.className = 'vp-chat-controls';
    chatControls.innerHTML = `<select class="vp-chat-agent" aria-label="Document chat agent"><option value="">Librarian</option></select><select class="vp-chat-model" aria-label="Document chat model"><option value="">Workspace default</option></select><label class="vp-upload" title="Upload text document into Vault" aria-label="Upload text document">⇪<input type="file" accept=".md,.markdown,.txt,.csv,.json,.yaml,.yml,text/plain,text/markdown,text/csv,application/json"></label><button class="vp-upload vp-diagram" title="Create flow diagram in document" aria-label="Create flow diagram in document">⌁</button><button class="vp-upload vp-clear-chat" title="Clear this chat" aria-label="Clear this chat">⌫</button>`;
    const chatHost = document.createElement('div');
    chatHost.style.cssText = 'display:flex; flex:1 1 0%; min-width:0; min-height:0; overflow:hidden;';
    const runHost = document.createElement('aside');
    runHost.className = 'vp-run-detail';
    runHost.innerHTML = '<div class="vp-run-head"><span class="vp-run-heading">Chat</span><span class="vp-run-switch"><button data-run-view="chat" data-active="1">Chat</button><button data-run-view="tickets">Tickets</button><button data-run-view="changes">Changes</button><button data-run-view="files">Files</button></span></div><div class="vp-run-body"></div>';
    const doc = document.createElement('div');
    doc.style.cssText = 'display:flex; flex-direction:column; flex:1 1 0%; min-width:0; min-height:0; overflow:hidden; background:var(--editor-surface,#1b1d23);';

    rail.innerHTML = `
      <div class="vp-rail-head">
        <button class="vp-icon-btn vp-collapse" title="Collapse Vault menu" aria-label="Collapse Vault menu">‹</button>
        <select class="vp-vault">${VAULTS.map((v) => `<option value="${v}"${v === vault ? ' selected' : ''}>${v}</option>`).join('')}</select>
        ${scopePrefix ? '<button class="vp-icon-btn vp-scope" style="width:auto;padding:0 7px" title="Show all Vault documents">Project</button>' : ''}
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
      <div class="vp-filter"><input class="vp-filter-input" type="search" placeholder="Filter by filename" spellcheck="false" aria-label="Filter notes by filename" /></div>
      <div class="vp-body"></div>
      <div class="vp-status"><span class="vp-count"></span><span class="vp-sync" style="margin-left:auto"></span></div>`;
    const body = rail.querySelector('.vp-body');
    const countEl = rail.querySelector('.vp-count');
    const filterWrap = rail.querySelector('.vp-filter');
    const filterInput = rail.querySelector('.vp-filter-input');

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

    // Orca-style center viewer: an overlay that hosts a code file or a colorized
    // diff on top of the note editor. Opening a note (openNote) hides it, so the
    // note state underneath never has to be torn down and rebuilt.
    doc.style.position = 'relative';
    const centerViewer = document.createElement('div');
    centerViewer.className = 'vp-center-viewer';
    centerViewer.style.cssText = 'position:absolute; inset:0; display:none; flex-direction:column; background:var(--editor-surface,#1b1d23); z-index:5;';
    centerViewer.innerHTML = '<div class="vp-cv-bar"><span class="vp-cv-path"></span><span class="vp-cv-modes"></span><button class="vp-cv-close" title="Back to note">Close</button></div><div class="vp-cv-body"></div>';
    doc.appendChild(centerViewer);
    const cvPath = centerViewer.querySelector('.vp-cv-path');
    const cvModes = centerViewer.querySelector('.vp-cv-modes');
    const cvBody = centerViewer.querySelector('.vp-cv-body');
    centerViewer.querySelector('.vp-cv-close').onclick = () => closeCenterViewer();

    function closeCenterViewer() { centerViewer.style.display = 'none'; }


    // A code/markdown file in the center. Markdown renders; everything else is
    // syntax-highlighted with line numbers, same look as the standalone editor.
    async function showFileInCenter(absPath) {
      const name = absPath.split('/').pop();
      const ext = (name.split('.').pop() || '').toLowerCase();
      cvPath.textContent = absPath.replace(cvDirname(currentProjectRoot) + '/', '');
      centerChanged(cvPath.textContent);
      cvModes.textContent = '';
      cvBody.innerHTML = '<div style="padding:14px;opacity:.6">Loading…</div>';
      centerViewer.style.display = 'flex';
      let content;
      try {
        content = await invoke('read_file', { path: absPath });
      } catch (e) {
        cvBody.innerHTML = `<div style="padding:14px;color:var(--danger,#e5534b)">Could not read ${escapeRun(name)}: ${escapeRun(String(e))}</div>`;
        return;
      }
      if (['md', 'markdown', 'mdx'].includes(ext) && window.xnautMarkdown) {
        cvBody.className = 'vp-cv-body xnaut-md';
        cvBody.style.padding = '14px 18px';
        window.xnautMarkdown.renderInto(cvBody, content);
        return;
      }
      cvBody.className = 'vp-cv-body vp-cv-code';
      cvBody.style.padding = '0';
      // One renderer for every code surface in the app (code-render.js): the
      // Code tab, this viewer and the diff all share a face, a size and a
      // gutter, rather than three copies that drift apart.
      cvBody.innerHTML = window.xnautRenderCode(content, path);
    }

    // A single file's diff in the center, colorized line by line. Reuses the raw
    // unified string from git_file_diff and paints +/- lines, so no diff library
    // is pulled in and untracked files still show their add-diff.
    async function showDiffInCenter(repo, relPath, opts) {
      opts = opts || {};
      cvPath.textContent = relPath;
      centerChanged(relPath);
      cvBody.className = 'vp-cv-body vp-cv-diff';
      cvBody.style.padding = '0';
      cvBody.innerHTML = '<div style="padding:14px;opacity:.6">Loading diff…</div>';
      centerViewer.style.display = 'flex';
      let raw;
      try {
        raw = await invoke('git_file_diff', { repo, path: relPath, staged: !!opts.staged, outgoing: !!opts.outgoing });
        // A fully-staged file has no unstaged diff; show its staged change instead.
        if ((!raw || !raw.trim()) && !opts.staged && !opts.outgoing) {
          raw = await invoke('git_file_diff', { repo, path: relPath, staged: true, outgoing: false });
        }
      } catch (e) {
        cvBody.innerHTML = `<div style="padding:14px;color:var(--danger,#e5534b)">${escapeRun(String(e))}</div>`;
        return;
      }
      const openInEditor = document.createElement('button');
      openInEditor.className = 'vp-cv-open';
      openInEditor.textContent = 'Open file';
      openInEditor.onclick = () => showFileInCenter(cvJoinPath(repo, relPath));
      cvModes.innerHTML = '';
      cvModes.appendChild(openInEditor);
      if (!raw || !raw.trim()) {
        cvBody.innerHTML = '<div style="padding:14px;opacity:.6">No textual diff (binary, or unchanged).</div>';
        return;
      }
      cvBody.innerHTML = window.xnautRenderDiff(raw, relPath);
    }

    // A whole commit's diff in the center — so work that's already committed and
    // pushed (a shipped release) is still reviewable, not just uncommitted edits.
    async function showCommitDiffInCenter(repo, sha, subject) {
      cvPath.textContent = subject ? `${sha.slice(0, 8)} · ${subject}` : sha;
      centerChanged('');   // a whole commit is not one file
      cvModes.innerHTML = '';
      cvBody.className = 'vp-cv-body vp-cv-diff';
      cvBody.style.padding = '0';
      cvBody.innerHTML = '<div style="padding:14px;opacity:.6">Loading commit…</div>';
      centerViewer.style.display = 'flex';
      let raw;
      try {
        raw = await invoke('git_commit_diff', { repo, sha });
      } catch (e) {
        cvBody.innerHTML = `<div style="padding:14px;color:var(--danger,#e5534b)">${escapeRun(String(e))}</div>`;
        return;
      }
      cvBody.innerHTML = window.xnautRenderDiff(raw || '', subject || sha);
    }


    function cvDirname(p) { return String(p || '').replace(/\/+$/, '').split('/').slice(0, -1).join('/'); }
    function cvJoinPath(a, b) { return `${String(a).replace(/\/+$/, '')}/${String(b).replace(/^\/+/, '')}`; }

    function columnDivider() {
      const el = document.createElement('div');
      el.className = 'vp-column-resize';
      return el;
    }
    const railDivider = columnDivider();
    const workflowDivider = columnDivider();
    const vaultMenuInMaster = typeof window.xnautSidebarShowSubmenu === 'function'
      && window.xnautSidebarShowSubmenu('Vault', vaultMenu);
    row.appendChild(rail);
    row.appendChild(railDivider);
    main.appendChild(doc);
    if (!opts.hideChat) chatSection.appendChild(chatHost);
    row.appendChild(main);
    row.appendChild(workflowDivider);
    row.appendChild(runHost);
    container.appendChild(row);

    let currentRel = null;
    let mode = 'preview';
    let saveTimer = null;
    let projectTickets = [];
    let currentProjectKey = '';
    let currentProjectRoot = '';
    let runView = opts.hideChat ? 'tickets' : 'chat';
    let selectedTicket = null;
    const documentChangeListeners = new Set();
    const emitDocumentChange = () => documentChangeListeners.forEach((listener) => {
      try { listener({ vault, rel: currentRel, content: ta.value }); } catch (_) { /* isolated UI listener */ }
    });
    const entry = {
      kind: 'vault', pane: row, tabId, root,
      vault: () => vault,
      openNote: null,
      openAgentFather: null,
      refresh: null,
      getDoc: () => ta.value,
      currentRel: () => currentRel,
      viewEl: view, taEl: ta, bodyEl: body, railEl: rail, chatHostEl: chatHost, runHostEl: runHost,
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

    const escapeRun = (value) => String(value == null ? '' : value).replace(/[&<>"']/g, (char) => ({ '&':'&amp;', '<':'&lt;', '>':'&gt;', '"':'&quot;', "'":'&#39;' }[char]));
    // A ticket body is prose with shouted labels in it: THE ISSUE, FIX, SCOPE:,
    // WHAT'S MISSING, PACKAGING SPLIT, VERIFY, DO FIRST, Tests. Rendered flat
    // it is a wall, and Andre could not tell what from why from how
    // (2026-09-13). These two rules turn the labels the writers already use
    // into headings, so the structure that was always in the text becomes
    // structure on the screen. Nothing is reworded and nothing is dropped.
    //
    // A fenced code block is left alone: SQL and shell are full of capitals
    // and a line inside a fence is not a heading.
    const CAPS_LINE = /^\s*([A-Z][A-Z0-9''’ /()&.,+-]{2,60}?)\s*:?\s*$/;
    const CAPS_LEAD = /^\s*([A-Z][A-Z0-9''’ /()&+-]{2,48}?)\s*:\s+(\S.*)$/;
    const KNOWN = /^(THE ISSUE|THE FIX|FIX|BUILD|SCOPE|VERIFY|TESTS?|WHAT|WHY|HOW|DO FIRST|NOTE|WARNING|MOVE|REMOVE|ADD)\b/;

    function ticketMarkdown(body) {
      let text = String(body || '').trim();
      if (!text) return '_No ticket text._';
      // The two labels this app has always promoted, and they are Title Case
      // rather than shouted, so the caps rules below never see them.
      text = text
        .replace(/^\s*(?:#{1,6}\s*)?(the\s+issue)\s*:?[ \t]*$/gim, '## The Issue')
        .replace(/^\s*(?:#{1,6}\s*)?(the\s+fix)\s*:?[ \t]*$/gim, '## The Fix');
      let fenced = false;
      return text.split('\n').map((line) => {
        if (/^\s*```/.test(line)) { fenced = !fenced; return line; }
        if (fenced || /^\s*#{1,6}\s/.test(line)) return line;
        const whole = CAPS_LINE.exec(line);
        // A label on its own line is a section. Two words minimum, or one
        // word this project actually uses as a heading, so a stray
        // "OK" or an acronym in prose is not promoted.
        if (whole && (whole[1].includes(' ') || KNOWN.test(whole[1]))) {
          return `\n## ${whole[1].replace(/\s+/g, ' ').trim()}\n`;
        }
        const lead = CAPS_LEAD.exec(line);
        if (lead && (lead[1].includes(' ') || KNOWN.test(lead[1]))) {
          return `\n### ${lead[1].replace(/\s+/g, ' ').trim()}\n\n${lead[2]}`;
        }
        return line;
      }).join('\n');
    }
    // What the centre is showing, so the ticket list can lead with the tickets
    // about it. Andre, 2026-09-13: "ideally I see here in this view only
    // related tickets, not all". NautGate has 56 and one open file is about
    // two of them.
    let centerFile = '';

    // The list is about what the centre shows, so it has to be repainted when
    // the centre changes. Only when the Tickets view is the one on screen;
    // repainting a hidden view is work nobody asked for.
    function centerChanged(file) {
      if (centerFile === file) return;
      centerFile = file;
      if (runView === 'tickets' && currentProjectKey) renderProjectTickets(currentProjectKey);
    }

    // Related means one of two things we can actually prove, in this order:
    // the ticket's own changed files include this path (git_ticket_files, so
    // it is the commits talking, not a guess), or the body names the file.
    // Anything weaker would be a search dressed up as a relationship.
    function relatedTo(ticket, target) {
      if (!target) return false;
      const path = String(target).replace(/^\/+/, '');
      const base = path.split('/').pop();
      if (!base) return false;

      // A ticket's `documentation` names its design note as
      // `work:NautGate/Development/features/2026-07-23_Docker-Distribution.md`.
      // That is a link the app maintains, not an inference, so it is checked
      // first and on its own it is enough. Compared on the basename because
      // the vault-relative path and the `work:` reference agree on the file
      // and not always on the prefix.
      const linked = (ticket.documentation || []).some((ref) => {
        const doc = String(ref).replace(/^work:/, '');
        return doc === path || doc.endsWith(`/${base}`);
      });
      if (linked) return true;

      // Then the commits: git_ticket_files is what the work actually touched.
      if ((ticket._files || []).some((f) => String(f.path || '').endsWith(path))) return true;

      // Last, and only for a code path, the body naming the file. A note's
      // name is prose and would match half the project on a common word.
      return !/\.(md|markdown|html|txt)$/i.test(base) && String(ticket.body || '').includes(base);
    }

    const totalsFor = (ticket) => (ticket._files || []).reduce((sum, file) => ({
      additions: sum.additions + Number(file.additions || 0),
      deletions: sum.deletions + Number(file.deletions || 0),
    }), { additions: 0, deletions: 0 });
    function openTicketFiles(ticket) {
      const files = ticket && ticket._files || [];
      files.forEach((file) => {
        if (window.xnautAttachDiffTab) window.xnautAttachDiffTab({
          name: file.path.split('/').pop(), worktree: currentProjectRoot, filePath: file.path, viewMode: 'split',
        });
      });
    }
    function renderProjectTickets(projectKey) {
      const target = runHost.querySelector('.vp-run-body');
      runHost.querySelector('.vp-run-heading').textContent = projectKey ? `Tickets · ${projectKey} · ${projectTickets.length}` : 'Project tickets';
      if (!projectKey) {
        target.innerHTML = '<div style="padding:6px;color:var(--text-muted,#777)">Open a project document to see its tickets.</div>';
        return;
      }
      if (!projectTickets.length) {
        target.innerHTML = '<div style="padding:6px;color:var(--text-muted,#777)">No tickets for this project.</div>';
        return;
      }
      // Related first, the rest behind one row. Never hidden: a ticket you
      // cannot reach is worse than a list you have to scroll, and "related"
      // is a heuristic, so the escape hatch is always there.
      const related = centerFile ? projectTickets.filter((t) => relatedTo(t, centerFile)) : [];
      const rest = related.length ? projectTickets.filter((t) => !related.includes(t)) : projectTickets;
      const card = (ticket) => { const index = projectTickets.indexOf(ticket); const totals = totalsFor(ticket); return `<details class="vp-ticket" data-ticket-index="${index}"><summary><span class="vp-ticket-caret">›</span><span class="vp-ticket-main"><span class="vp-ticket-id">${escapeRun(ticket.id)}</span><div class="vp-ticket-title">${escapeRun(ticket.title)}</div></span><button class="vp-ticket-stats" title="Open changed files as review tabs"><span class="vp-ticket-add">+${totals.additions}</span><span class="vp-ticket-del">−${totals.deletions}</span></button><span class="vp-ticket-status" data-status="${escapeRun(ticket.status)}">${escapeRun(ticket.status)}</span></summary><div class="vp-ticket-text xnaut-md"></div><div class="vp-ticket-meta" style="padding:0 25px 14px">${escapeRun(ticket.ticket_type || ticket.type || '')}${ticket.priority ? ` · ${escapeRun(ticket.priority)} priority` : ''}${ticket.owner ? ` · ${escapeRun(ticket.owner)}` : ''}</div></details>`; };
      const headRow = (text) => `<div class="vp-ticket-group">${escapeRun(text)}</div>`;
      const openName = centerFile.split('/').pop();
      target.innerHTML = (related.length
        ? headRow(`${related.length} linked to ${openName}`) + related.map(card).join('')
          + `<details class="vp-ticket-rest"><summary>${rest.length} other ticket${rest.length === 1 ? '' : 's'} in this project</summary>${rest.map(card).join('')}</details>`
        : (centerFile ? headRow(`Nothing links to ${openName}, showing all ${rest.length}`) : '')
          + rest.map(card).join(''));
      target.querySelectorAll('.vp-ticket').forEach((ticketEl) => {
        const ticket = projectTickets[Number(ticketEl.dataset.ticketIndex)];
        window.xnautMarkdown.renderInto(ticketEl.querySelector('.vp-ticket-text'), ticketMarkdown(ticket.body));
        ticketEl.addEventListener('toggle', () => { if (ticketEl.open) selectedTicket = ticket; });
        ticketEl.querySelector('.vp-ticket-stats').onclick = (event) => {
          event.preventDefault(); event.stopPropagation(); selectedTicket = ticket; openTicketFiles(ticket);
        };
      });
    }

    function showRunView(viewName) {
      runView = viewName;
      runHost.querySelectorAll('[data-run-view]').forEach((button) => { button.dataset.active = button.dataset.runView === runView ? '1' : '0'; });
      const target = runHost.querySelector('.vp-run-body');
      if (runView === 'chat') {
        runHost.querySelector('.vp-run-heading').textContent = 'Chat';
        target.innerHTML = '';
        target.style.padding = '0';
        target.appendChild(chatSection);
        return;
      }
      target.style.padding = '8px';
      if (runView === 'tickets') {
        renderProjectTickets(currentProjectKey);
        return;
      }
      if (runView === 'files') {
        runHost.querySelector('.vp-run-heading').textContent = currentProjectKey ? `Files · ${currentProjectKey}` : 'Files';
        target.innerHTML = '';
        if (!currentProjectRoot) {
          target.innerHTML = '<div style="padding:6px;color:var(--text-muted,#777)">No local codebase is linked to this project.</div>';
          return;
        }
        renderFileTree(target, currentProjectRoot, target);
        return;
      }
      runHost.querySelector('.vp-run-heading').textContent = currentProjectKey ? `Changes · ${currentProjectKey}` : 'Changes';
      target.innerHTML = '';
      if (!currentProjectRoot) {
        target.innerHTML = '<div style="padding:6px;color:var(--text-muted,#777)">No local codebase is linked to this project.</div>';
        return;
      }
      renderRepoChanges(target);
      return;
    }

    // A single changed-file row that opens the old-vs-new diff in the center.
    function fileRow(file, repo, opts) {
      const dir = file.path.split('/').slice(0, -1).join('/');
      const base = file.path.split('/').pop();
      const el = document.createElement('button');
      el.className = 'vp-code-file';
      el.innerHTML = `<span class="vp-code-status" data-s="${escapeRun(file.status)}">${escapeRun(file.status)}</span><span class="vp-code-path"><span class="vp-code-base">${escapeRun(base)}</span>${dir ? `<span class="vp-code-dir">${escapeRun(dir)}</span>` : ''}</span><span class="vp-ticket-add">+${file.additions || 0}</span><span class="vp-ticket-del">−${file.deletions || 0}</span>`;
      el.onclick = () => {
        runHost.querySelectorAll('.vp-code-file').forEach((b) => { b.dataset.active = '0'; });
        el.dataset.active = '1';
        showDiffInCenter(repo, file.path, opts || {});
      };
      return el;
    }
    function changeSection(target, label, count) {
      const head = document.createElement('div');
      head.className = 'vp-changes-head';
      head.innerHTML = `${escapeRun(label)}${count != null ? ` <span class="vp-changes-count">${count}</span>` : ''}`;
      target.appendChild(head);
      return head;
    }

    // Group a flat file list by directory into collapsible folders — the only
    // grouping that fits an uncommitted pile (one branch, one worktree, not yet
    // released), and what makes 90+ changed files navigable.
    function renderFileGroups(container, files, repo, opts) {
      const groups = new Map();
      files.forEach((f) => {
        const dir = f.path.split('/').slice(0, -1).join('/') || '(root)';
        if (!groups.has(dir)) groups.set(dir, []);
        groups.get(dir).push(f);
      });
      [...groups.keys()].sort().forEach((dir) => {
        const gfiles = groups.get(dir);
        const head = document.createElement('div');
        head.className = 'vp-grp-head';
        head.innerHTML = `<span class="vp-ftree-caret">⌄</span><span class="vp-grp-dir">${escapeRun(dir)}</span><span class="vp-changes-count">${gfiles.length}</span>`;
        container.appendChild(head);
        const kids = document.createElement('div');
        container.appendChild(kids);
        gfiles.forEach((f) => kids.appendChild(fileRow(f, repo, opts)));
        head.onclick = () => {
          const open = kids.style.display !== 'none';
          kids.style.display = open ? 'none' : '';
          head.querySelector('.vp-ftree-caret').textContent = open ? '›' : '⌄';
        };
      });
    }

    // The project's whole git surface, the way Orca shows it: uncommitted work
    // (grouped by folder), commits not yet pushed, recent commits (so a shipped
    // release is reviewable), and every linked worktree. Click any file to open
    // its diff, or a commit to open the whole commit's diff, in the center.
    async function renderRepoChanges(target) {
      target.innerHTML = '<div style="padding:6px;opacity:.6">Loading changes…</div>';
      // .catch() covers a rejection; it does not cover a command that RESOLVES
      // with null, which is what a backend returning None does. Reading .length
      // off that killed the whole pane mid-render (same shape as XNAUT-137).
      const [uncommitted, outgoing, commits, worktrees] = (await Promise.all([
        invoke('git_uncommitted_files', { repo: currentProjectRoot }).catch(() => []),
        invoke('git_outgoing_files', { repo: currentProjectRoot }).catch(() => []),
        invoke('git_commit_log', { repo: currentProjectRoot, since: '3 months ago', limit: 25, keys: currentProjectKey ? [currentProjectKey] : [] }).catch(() => []),
        invoke('git_worktree_list', { repo: currentProjectRoot }).catch(() => []),
      ])).map((r) => r || []);
      target.innerHTML = '';
      runHost.querySelector('.vp-run-heading').textContent = `Changes · ${currentProjectKey || 'repo'} · ${uncommitted.length}`;

      changeSection(target, 'Uncommitted', uncommitted.length);
      if (uncommitted.length) renderFileGroups(target, uncommitted, currentProjectRoot, {});
      else target.appendChild(emptyLine('Working tree is clean.'));

      if (outgoing.length) {
        changeSection(target, 'Not pushed', outgoing.length);
        renderFileGroups(target, outgoing, currentProjectRoot, { outgoing: true });
      }

      if (commits.length) {
        changeSection(target, 'Recent commits', commits.length);
        commits.forEach((c) => {
          const el = document.createElement('button');
          el.className = 'vp-commit-row';
          el.innerHTML = `<span class="vp-commit-sha">${escapeRun(c.short_sha)}</span><span class="vp-commit-subject">${escapeRun(c.subject)}</span>${c.tag ? `<span class="vp-commit-tag">${escapeRun(c.tag)}</span>` : ''}<span class="vp-commit-date">${escapeRun(c.date)}</span>`;
          el.onclick = () => {
            runHost.querySelectorAll('.vp-commit-row').forEach((b) => { b.dataset.active = '0'; });
            el.dataset.active = '1';
            showCommitDiffInCenter(currentProjectRoot, c.sha, c.subject);
          };
          target.appendChild(el);
        });
      }

      const others = (worktrees || []).filter((w) => !w.current);
      if (others.length) {
        changeSection(target, 'Worktrees', others.length);
        others.forEach((w) => {
          const row = document.createElement('div');
          row.className = 'vp-wt-row';
          const name = w.path.split('/').pop();
          row.innerHTML = `<span class="vp-ftree-caret">›</span><span class="vp-wt-branch">${escapeRun(w.branch || name)}</span>${w.changes ? `<span class="vp-changes-count">${w.changes}</span>` : ''}<span class="vp-wt-head">${escapeRun(w.head)}</span>`;
          target.appendChild(row);
          const kids = document.createElement('div');
          kids.className = 'vp-ftree-kids';
          kids.style.display = 'none';
          target.appendChild(kids);
          let loaded = false;
          row.onclick = async () => {
            const open = kids.style.display === 'none';
            kids.style.display = open ? 'block' : 'none';
            row.querySelector('.vp-ftree-caret').textContent = open ? '⌄' : '›';
            if (open && !loaded) {
              loaded = true;
              const wf = await invoke('git_uncommitted_files', { repo: w.path }).catch(() => []);
              if (!wf.length) kids.appendChild(emptyLine('Clean.'));
              else wf.forEach((f) => kids.appendChild(fileRow(f, w.path, {})));
            }
          };
        });
      }
    }
    function emptyLine(text) {
      const el = document.createElement('div');
      el.style.cssText = 'padding:6px 8px;color:var(--text-muted,#777);';
      el.textContent = text;
      return el;
    }

    // Orca's Files panel: the linked repo's tree, expanded lazily one folder at
    // a time via list_directory, click a file to open it in the center.
    const HIDE_ENTRY = new Set(['.git', '.DS_Store', 'node_modules', 'target', '.next', 'dist', '.turbo']);
    const FOLDER_ICON = '<svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.3"><path d="M1.8 4.4c0-.6.5-1.1 1.1-1.1h3l1.4 1.5h5.9c.6 0 1.1.5 1.1 1.1v6.2c0 .6-.5 1.1-1.1 1.1H2.9c-.6 0-1.1-.5-1.1-1.1V4.4z"/></svg>';
    const FOLDER_OPEN_ICON = '<svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.3"><path d="M2 5.3V4.2c0-.6.5-1.1 1.1-1.1h2.8l1.4 1.5h5.6c.6 0 1.1.5 1.1 1.1v.6M1.7 5.3h11.8c.7 0 1.2.7 1 1.4l-1 4.5c-.2.5-.6.8-1.1.8H2.9c-.6 0-1.1-.5-1.1-1.1V5.3z"/></svg>';
    const FILE_ICON = '<svg viewBox="0 0 16 16" width="12" height="12" fill="none" stroke="currentColor" stroke-width="1.3"><path d="M4 1.8h4.8L12 5v9.2H4z"/><path d="M8.6 1.8V5H12"/></svg>';
    async function renderFileTree(container, dir, rootTarget, depth) {
      depth = depth || 0;
      let listing;
      try {
        listing = await invoke('list_directory', { path: dir });
      } catch (e) {
        container.innerHTML = `<div style="padding:6px;color:var(--danger,#e5534b)">${escapeRun(String(e))}</div>`;
        return;
      }
      if (depth === 0) container.innerHTML = '';
      listing.entries.filter((e) => !HIDE_ENTRY.has(e.name)).forEach((entry) => {
        const rowEl = document.createElement('div');
        rowEl.className = 'vp-ftree-row';
        rowEl.dataset.dir = entry.is_directory ? '1' : '0';
        rowEl.style.paddingLeft = `${10 + depth * 14}px`;
        rowEl.innerHTML = `<span class="vp-ftree-caret">${entry.is_directory ? '▸' : ''}</span><span class="vp-ftree-icon">${entry.is_directory ? FOLDER_ICON : fileGlyph(entry.name)}</span><span class="vp-ftree-name">${escapeRun(entry.name)}</span>`;
        container.appendChild(rowEl);
        if (entry.is_directory) {
          const kids = document.createElement('div');
          kids.className = 'vp-ftree-kids';
          kids.style.display = 'none';
          container.appendChild(kids);
          let loaded = false;
          rowEl.onclick = async () => {
            const open = kids.style.display === 'none';
            kids.style.display = open ? 'block' : 'none';
            rowEl.querySelector('.vp-ftree-caret').textContent = open ? '▾' : '▸';
            rowEl.querySelector('.vp-ftree-icon').innerHTML = open ? FOLDER_OPEN_ICON : FOLDER_ICON;
            if (open && !loaded) { loaded = true; await renderFileTree(kids, entry.path, rootTarget, depth + 1); }
          };
        } else {
          rowEl.onclick = () => {
            rootTarget.querySelectorAll('.vp-ftree-row').forEach((r) => { r.dataset.active = '0'; });
            rowEl.dataset.active = '1';
            showFileInCenter(entry.path);
          };
        }
      });
    }
    function fileGlyph() { return FILE_ICON; }
    runHost.querySelectorAll('[data-run-view]').forEach((button) => { button.onclick = () => showRunView(button.dataset.runView); });
    if (opts.hideChat) runHost.querySelector('[data-run-view="chat"]').hidden = true;
    showRunView(runView);

    async function refreshRunDetail() {
      projectTickets = [];
      try {
        const projects = await invoke('pm_project_list').catch(() => []);
        const folder = (scopePrefix || String(currentRel || '').split('/')[0]).toLowerCase();
        const ticketPrefix = ((ta.value.match(/\b([A-Z][A-Z0-9]+)-\d+\b/) || [])[1] || '').toLowerCase();
        const project = (projects || []).find((item) => String(item.key).toLowerCase() === String(opts.projectKey || '').toLowerCase())
          || (projects || []).find((item) => [item.key, item.name].some((value) => String(value || '').toLowerCase() === folder))
          || (projects || []).find((item) => String(item.key).toLowerCase() === ticketPrefix);
        const projectKey = project && project.key;
        currentProjectKey = projectKey || '';
        currentProjectRoot = project && (project.source_path || project.source_repo || project.repo_path) || '';
        // Per-project chat: each project keeps its own thread, so switching
        // projects on the left shows that project's conversation (or an empty
        // one), not the single ever-growing pile shared across all of them.
        if (entry.chat && entry.chat.setChatKey) {
          entry.chat.setChatKey(currentProjectKey
            ? 'vault-document:v2:proj:' + currentProjectKey.toLowerCase()
            : 'vault-document:v2:vault:' + vault);
        }
        if (projectKey) projectTickets = await invoke('pm_ticket_list', { project: projectKey }).catch(() => []);
        if (currentProjectRoot) await Promise.all(projectTickets.map(async (ticket) => {
          ticket._files = await invoke('git_ticket_files', { repo: currentProjectRoot, ticketId: ticket.id }).catch(() => []);
        }));
        selectedTicket = projectTickets.find((ticket) => ticket.id === selectedTicket?.id) || projectTickets[0] || null;
        projectTickets.sort((a, b) => String(b.updated_at || '').localeCompare(String(a.updated_at || '')));
        showRunView(runView);
      } catch (error) {
        runHost.querySelector('.vp-run-body').textContent = `Tickets unavailable: ${error}`;
      }
    }

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

    function makeColumnResizable(handle, pane, min, max, storageKey, reverse) {
      const saved = Number(localStorage.getItem(storageKey));
      if (Number.isFinite(saved) && saved >= min && saved <= max) pane.style.flexBasis = `${saved}px`;
      handle.addEventListener('pointerdown', (event) => {
        event.preventDefault();
        const startX = event.clientX;
        const startWidth = pane.getBoundingClientRect().width;
        handle.dataset.active = '1';
        const move = (next) => {
          const delta = (next.clientX - startX) * (reverse ? -1 : 1);
          const width = Math.max(min, Math.min(max, startWidth + delta));
          pane.style.flexBasis = `${width}px`;
          localStorage.setItem(storageKey, String(Math.round(width)));
        };
        const done = () => {
          handle.dataset.active = '0';
          window.removeEventListener('pointermove', move);
          window.removeEventListener('pointerup', done);
        };
        window.addEventListener('pointermove', move);
        window.addEventListener('pointerup', done);
      });
    }
    makeColumnResizable(railDivider, rail, 180, 420, 'xnaut-vault-files-width', false);
    makeColumnResizable(workflowDivider, runHost, 300, 640, 'xnaut-vault-run-width', true);
    const collapseButton = rail.querySelector('.vp-collapse');
    rail.dataset.collapsed = '0';
    collapseButton.hidden = true;

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
      emitDocumentChange();
    });

    async function openNote(rel) {
      await flushSave();
      closeCenterViewer();
      try {
        ta.value = await invoke('vault_note_read', { vault, rel });
      } catch (e) {
        console.error('[vault] open failed', e);
        return;
      }
      currentRel = rel;
      centerChanged(rel);
      title.textContent = rel;
      status.textContent = '';
      if (mode === 'preview') renderView();
      body.querySelectorAll('.vp-note-row').forEach((r) => { r.dataset.active = r.dataset.rel === rel ? '1' : '0'; });
      renderVaultMenu().catch((error) => console.error('[vault] menu refresh failed', error));
      refreshBacklinks();
      refreshRunDetail();
      emitDocumentChange();
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
      window.xnautPlaceAtClick(menuEl, x, y);
    }

    async function renderVaultMenu() {
      const makeLabel = (text) => {
        const el = document.createElement('div');
        el.className = 'vp-master-label';
        el.textContent = text;
        vaultMenu.appendChild(el);
      };
      const makeRow = (icon, name, count, onClick, active) => {
        const el = document.createElement('div');
        el.className = 'vp-master-row';
        if (active) el.dataset.active = '1';
        const iconEl = document.createElement('span');
        iconEl.className = 'vp-master-icon';
        iconEl.textContent = icon;
        const nameEl = document.createElement('span');
        nameEl.className = 'vp-master-name';
        nameEl.textContent = name;
        el.append(iconEl, nameEl);
        if (count !== undefined && count !== null) {
          const countEl = document.createElement('span');
          countEl.className = 'vp-master-count';
          countEl.textContent = String(count);
          el.appendChild(countEl);
        }
        el.onclick = onClick;
        vaultMenu.appendChild(el);
        return el;
      };

      vaultMenu.innerHTML = '';
      const notes = entry.notes || [];
      const pinned = notes.filter((note) => (note.tags || []).some((tag) => /^(pin|pinned)$/i.test(tag)));
      if (pinned.length) {
        makeLabel('Pinned');
        pinned.slice(0, 5).forEach((note) => makeRow('★', note.title, null, () => openNote(note.rel), note.rel === currentRel));
      }
      const recent = notes.slice().sort((a, b) => Number(b.mtime || 0) - Number(a.mtime || 0))[0];
      if (recent) {
        makeLabel('Recent');
        makeRow('◷', recent.title, null, () => openNote(recent.rel), recent.rel === currentRel);
      }
      makeLabel('Vaults');
      const counts = {};
      await Promise.all(VAULTS.map(async (value) => {
        if (value === vault) counts[value] = notes.length;
        else counts[value] = await invoke('vault_tree', { vault: value }).then((tree) => tree.notes.length).catch(() => '');
      }));
      makeRow('◆', 'Work', counts.work, () => switchVault('work'), vault === 'work');
      makeRow('◇', 'Private', counts.personal, () => switchVault('personal'), vault === 'personal');
      const spacer = document.createElement('div');
      spacer.className = 'vp-master-spacer';
      vaultMenu.appendChild(spacer);
      const newRow = makeRow('+', 'New note', null, () => rail.querySelector('.vp-new').click());
      newRow.classList.add('vp-master-new');
    }

    let lastTree = null;
    async function refresh() {
      const tree = await invoke('vault_tree', { vault });
      const visibleTree = scoped ? {
        notes: tree.notes.filter((note) => note.rel === scopePrefix || note.rel.startsWith(`${scopePrefix}/`)),
        dirs: tree.dirs.filter((dir) => dir === scopePrefix || dir.startsWith(`${scopePrefix}/`) || scopePrefix.startsWith(`${dir}/`)),
      } : tree;
      lastTree = visibleTree;
      drawTree();
      entry.notes = visibleTree.notes;
      await renderVaultMenu();
      return visibleTree;
    }
    entry.refresh = refresh;

    // Filename filter over the cached tree, so a keystroke costs no IPC round
    // trip. Same predicate as the wikilink autocomplete: case-insensitive
    // substring over stem_key and title. Matches keep their folder context, and
    // every folder is opened so a hit three levels deep is actually visible.
    function drawTree() {
      if (!lastTree) return;
      const q = filterInput.value.trim().toLowerCase();
      const notes = q
        ? lastTree.notes.filter((n) => noteNameKey(n).includes(q) || n.title.toLowerCase().includes(q))
        : lastTree.notes;
      const dirs = q ? ancestorDirs(notes) : lastTree.dirs;
      body.innerHTML = '';
      renderTree(buildTree(dirs, notes), body, {
        openNote,
        showMenu: showContextMenu,
        onDrop: handleDrop,
      }, currentRel);
      if (q) {
        body.querySelectorAll('details').forEach((d) => { d.open = true; });
        if (!notes.length) body.innerHTML = '<div style="padding:8px;font-size:11px;opacity:.6">No filename matches</div>';
      }
      countEl.textContent = q ? `${notes.length} of ${lastTree.notes.length} notes` : `${lastTree.notes.length} notes`;
    }
    filterInput.addEventListener('input', drawTree);
    filterInput.addEventListener('keydown', (e) => {
      if (e.key !== 'Escape' || !filterInput.value) return;
      filterInput.value = '';
      drawTree();
    });

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
      filterWrap.hidden = t !== 'notes';
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
      emitDocumentChange();
    }

    rail.querySelector('.vp-vault').onchange = (e) => switchVault(e.target.value).catch((err) => console.error('[vault] switch failed', err));
    const scopeButton = rail.querySelector('.vp-scope');
    if (scopeButton) scopeButton.onclick = async () => {
      scoped = !scoped;
      scopeButton.textContent = scoped ? 'Project' : 'All Vault';
      scopeButton.title = scoped ? 'Show all Vault documents' : `Show only ${scopePrefix}`;
      await refresh();
    };
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
      else setCreatePrefix(scoped ? scopePrefix : '');
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
    setCreatePrefix(scoped ? scopePrefix : '');
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
      if (vaultMenuInMaster && vaultMenu.isConnected && typeof window.xnautSidebarShowMain === 'function') window.xnautSidebarShowMain();
      documentChangeListeners.clear();
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
      'DIAGRAMS GO IN THE DOCUMENT. When asked to draw or diagram anything, add a ```mermaid``` block inside the PLAN DOCUMENT above. You have no separate canvas here; never say you drew "on the canvas".',
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

    let selectedAgent = 'Librarian';
    if (!opts.hideChat) entry.chat = await window.xnautCreateChatPane(tabId, chatHost, {
      // Do not hydrate the embedded workspace with the legacy Librarian
      // conversation. That history belongs to Agent Space and can contain
      // long template answers that crowd the document composer.
      // Per-vault fallback key; refreshRunDetail re-keys it per project once a
      // project-scoped note is open. Deliberately NOT the old shared
      // 'vault-document:v2:<vault>' blob, so a fresh workspace starts empty.
      chatKey: 'vault-document:v2:vault:' + vault,
      title: 'Document chat',
      embedded: true,
      contextProvider: () => `Active document agent: ${selectedAgent}. Work in that role while preserving the document's voice.`,
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
        title: 'Document chat',
        persona,
      },
      vaultTools: { vault: () => vault, entry },
    });

    if (!opts.hideChat && entry.chat) {
      const inputArea = entry.chat.pane.querySelector('.chatp-input-area');
      // Live voice wraps the mic and its menu in one control. Insert beside
      // that wrapper: the mic itself is no longer a direct composer child.
      const micControl = entry.chat.dictateBtn?.closest('.voice-mic-control') || entry.chat.dictateBtn;
      if (inputArea) inputArea.insertBefore(chatControls, micControl || entry.chat.sendBtn);
      entry.chat.inputEl.rows = 2;
    }

    async function populateChatControls() {
      const agentSelect = chatControls.querySelector('.vp-chat-agent');
      const modelSelect = chatControls.querySelector('.vp-chat-model');
      const [agents, models] = await Promise.all([
        invoke('agent_list').catch(() => []),
        invoke('chat_list_provider_models').catch(() => []),
      ]);
      (agents || []).forEach((agent) => {
        const option = document.createElement('option');
        option.value = agent.display_name || agent.handle || agent.name || '';
        option.textContent = agent.display_name || agent.handle || agent.name || 'Agent';
        agentSelect.appendChild(option);
      });
      (models || []).forEach((model) => {
        const option = document.createElement('option');
        option.value = `${model.provider}\t${model.model}`;
        option.textContent = `${model.provider} · ${model.model}`;
        modelSelect.appendChild(option);
      });
      const applyModel = (provider, model) => {
        if (!entry.chat) return;
        entry.chat.providerOverride = provider || '';
        entry.chat.modelOverride = model || '';
        const modelLabel = entry.chat.pane.querySelector('.chatp-model');
        if (modelLabel) modelLabel.textContent = model || 'Workspace default';
      };
      // Default the Librarian to nautgate · auto: the funded, tool-capable
      // route. claude-opus-* via nautgate hits the unfunded Anthropic API (400
      // "credit balance too low"), which reads as "chat is broken". auto routes
      // to a working model that carries tool calls.
      const hasNautgate = (models || []).some((m) => String(m.provider).toLowerCase() === 'nautgate');
      if (hasNautgate && entry.chat && !entry.chat.modelOverride && !entry.chat.providerOverride) {
        const autoValue = 'nautgate\tauto';
        if (![...modelSelect.options].some((o) => o.value === autoValue)) {
          const opt = document.createElement('option');
          opt.value = autoValue; opt.textContent = 'nautgate · auto';
          modelSelect.insertBefore(opt, modelSelect.options[1] || null);
        }
        modelSelect.value = autoValue;
        applyModel('nautgate', 'auto');
      }
      agentSelect.onchange = () => { selectedAgent = agentSelect.value || 'Librarian'; };
      modelSelect.onchange = () => {
        const [provider, model] = modelSelect.value.split('\t');
        applyModel(provider, model);
      };
    }
    if (!opts.hideChat) populateChatControls().catch((error) => console.error('[vault] chat controls failed', error));

    function insertMarkdown(markdown) {
      if (!currentRel) return false;
      const addition = String(markdown || '');
      if (!addition.trim()) return false;
      const cursor = mode === 'edit' ? ta.selectionStart : ta.value.length;
      ta.value = ta.value.slice(0, cursor) + addition + ta.value.slice(cursor);
      ta.selectionStart = ta.selectionEnd = cursor + addition.length;
      status.textContent = 'editing...';
      if (mode === 'preview') renderView();
      emitDocumentChange();
      persist();
      return true;
    }

    const uploadInput = chatControls.querySelector('.vp-upload input');
    uploadInput.onchange = async () => {
      const file = uploadInput.files && uploadInput.files[0];
      uploadInput.value = '';
      if (!file) return;
      try {
        const content = await file.text();
        const safeName = file.name.replace(/[^A-Za-z0-9._-]+/g, '-').replace(/\.(markdown|txt|csv|json|ya?ml)$/i, '.md');
        const rel = `_inbox/${safeName.endsWith('.md') ? safeName : `${safeName}.md`}`;
        await invoke('vault_note_create', { vault, rel, content });
        await refresh();
        await openNote(rel);
        countEl.textContent = `uploaded ${file.name}`;
      } catch (error) {
        countEl.textContent = 'upload failed';
        console.error('[vault] upload failed', error);
      }
    };

    chatControls.querySelector('.vp-clear-chat').onclick = () => {
      if (entry.chat && entry.chat.clearChat) entry.chat.clearChat();
      status.textContent = 'chat cleared';
    };

    chatControls.querySelector('.vp-diagram').onclick = async () => {
      if (!currentRel) {
        status.textContent = 'open a note first';
        return;
      }
      const button = chatControls.querySelector('.vp-diagram');
      button.disabled = true;
      status.textContent = 'diagram…';
      try {
        const source = await invoke('vault_generate_diagram', { vault, rel: currentRel, diagramType: 'process flow' });
        insertMarkdown(`\n\n## Flow diagram\n\n\`\`\`mermaid\n${source.trim()}\n\`\`\`\n`);
        status.textContent = 'diagram inserted';
      } catch (error) {
        status.textContent = 'diagram failed';
        console.error('[vault] diagram generation failed', error);
      } finally {
        button.disabled = false;
      }
    };

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
