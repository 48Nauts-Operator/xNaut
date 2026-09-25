// Left sidebar: an icon rail and a project tree (XNAUT-335).
//
// LAYOUT BORROWED FROM ORCA (the reference André pointed at on 2026-09-12),
// per the borrowed-ideas rule in CLAUDE.md. Orca puts a handful of global rows
// at the top as icons only, then gives the whole rest of the sidebar to
// Projects: a Pinned group, one collapsible group per project, and a project's
// children are its WORKTREES, each with a state dot and a star. The row that
// says how many worktrees are being HIDDEN is Orca's too, and it is the piece
// that makes the shape work here at all: this repo has fourteen worktrees, so
// an uncapped tree buries every other project below the fold.
//
// Where we departed from Orca:
//   * Orca's tree is one source; ours merges two. Projects come from the PM
//     control repo (pm_project_list) AND from the local task registry
//     (tasks_list), matched on path. Dropping either would make projects
//     unreachable that are reachable today, so the group row keeps the
//     registry row's whole behaviour (session dot, click-to-attach, context
//     menu) and gains worktree children.
//   * Our dot has exactly three states and no fourth: an agent is running on
//     that worktree, the worktree is dirty, or it is clean.
//   * The surfaces that are not in the rail are not gone. They live behind the
//     rail's More button and route through exactly the globals they routed
//     through when they were rows; XNAUT-337/342 fold them into the workspace,
//     and that is a different ticket.
//
// Architecture mirrors markdown-pane.js: IIFE module, window.xnaut* exports,
// inline-SVG icon buttons, scoped <style> injected once, defensive Tauri
// access. app.js owns mounting (calls xnautMountSidebar) and panel routing
// (provides window.xnautSidebarNavigate); this module owns rendering and
// sidebar-local state (active rail icon, pins, the tree, context menu).
(function () {
  'use strict';

  const invoke = (...a) => window.__TAURI__.core.invoke(...a);

  const PIN_KEY = 'xnaut-pinned-projects';
  // Pinned worktrees are a different thing from pinned projects and get their
  // own store: a pin is a project/worktree PAIR, which the old key cannot hold.
  const WT_PIN_KEY = 'xnaut-pinned-worktrees';
  // "Hide all except pinned": the list shows the Pinned group alone.
  const ONLY_PINNED_KEY = 'xnaut-projects-only-pinned';
  const PINNED_COLLAPSE_KEY = 'xnaut-pinned-collapsed';
  // Which list the sidebar body shows: 'projects' or 'sessions' (2026-09-15).
  // The sessions list itself is read fresh from zellij on every refresh.
  const SIDEBAR_VIEW_KEY = 'xnaut-sidebar-view';
  // How many worktrees a group shows before the rest go behind one row. Orca's
  // answer to a repo with fourteen of them.
  const WORKTREE_CAP = 5;
  // Where the selected project/worktree pair is remembered across a reload.
  const SCOPE_KEY = 'xnaut-active-scope';

  function escapeText(s) {
    return String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
  }

  // For an attribute selector: a project key or a path can hold a quote.
  const cssAttr = (s) => String(s).replace(/\\/g, '\\\\').replace(/"/g, '\\"');

  function navigate(key, payload) {
    if (typeof window.xnautSidebarNavigate === 'function') {
      try { window.xnautSidebarNavigate(key, payload); } catch (e) { console.error('[sidebar] navigate failed:', e); }
    } else {
      console.warn('[sidebar] xnautSidebarNavigate not wired yet (key:', key, ')');
    }
  }

  // ---------- pin state ----------
  function loadPins() {
    try {
      const v = JSON.parse(localStorage.getItem(PIN_KEY) || '[]');
      return Array.isArray(v) ? v : [];
    } catch (_) { return []; }
  }
  function savePins(pins) {
    try { localStorage.setItem(PIN_KEY, JSON.stringify(pins)); } catch (_) { /* quota; ignore */ }
  }

  // A worktree pin is the PAIR, because the same branch name exists in two
  // repos and the Pinned group has to be able to say which one it means.
  const wtPinId = (projectKey, path) => `${projectKey} ${path}`;
  function loadWtPins() {
    try {
      const v = JSON.parse(localStorage.getItem(WT_PIN_KEY) || '[]');
      return Array.isArray(v) ? v.filter((s) => typeof s === 'string') : [];
    } catch (_) { return []; }
  }
  function saveWtPins(pins) {
    try { localStorage.setItem(WT_PIN_KEY, JSON.stringify(pins)); } catch (_) { /* quota; ignore */ }
  }

  // ---------- icons ----------
  const SVG_ATTRS = 'viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"';
  const ICONS = {
    sessions: `<svg ${SVG_ATTRS}><rect x="2" y="3" width="12" height="10" rx="1.5"/><path d="M4.5 6.5l2 1.5-2 1.5M8 9.5h3"/></svg>`,
    search: `<svg ${SVG_ATTRS}><circle cx="7" cy="7" r="4"/><line x1="10" y1="10" x2="13.5" y2="13.5"/></svg>`,
    // Three nodes and the edges between them: the mesh, not a mailbox. The
    // envelope moved to Inbox, which is the thing that actually holds letters.
    mesh: `<svg ${SVG_ATTRS}><circle cx="8" cy="3.4" r="1.7"/><circle cx="3.4" cy="11.6" r="1.7"/><circle cx="12.6" cy="11.6" r="1.7"/><path d="M6.7 5L4.7 10M9.3 5l2 5M5.2 11.6h5.6"/></svg>`,
    automations: `<svg ${SVG_ATTRS}><path d="M8.5 2L4 9h3.5L7 14l5-7H8.5l.5-5z"/></svg>`,
    observatory: `<svg ${SVG_ATTRS}><circle cx="8" cy="8" r="5.5"/><circle cx="8" cy="8" r="2"/><path d="M8 2.5V1M8 15v-1.5M2.5 8H1M15 8h-1.5"/></svg>`,
    // A tray with the lip open: what is waiting for you.
    inbox: `<svg ${SVG_ATTRS}><path d="M2 8.6l1.8-5.1h8.4L14 8.6v3.4a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1z"/><path d="M2 8.6h3.3l.8 1.7h3.8l.8-1.7H14"/></svg>`,
    more: `<svg ${SVG_ATTRS}><circle cx="3" cy="8" r="1"/><circle cx="8" cy="8" r="1"/><circle cx="13" cy="8" r="1"/></svg>`,
    gear: `<svg ${SVG_ATTRS}><circle cx="8" cy="8" r="2.2"/><path d="M8 1.8v1.6M8 12.6v1.6M2.3 8h1.6M12.1 8h1.6M4 4l1.1 1.1M10.9 10.9L12 12M12 4l-1.1 1.1M5.1 10.9L4 12"/></svg>`,
    plus: `<svg ${SVG_ATTRS}><line x1="8" y1="3" x2="8" y2="13"/><line x1="3" y1="8" x2="13" y2="8"/></svg>`,
    refresh: `<svg ${SVG_ATTRS}><path d="M13 8a5 5 0 1 1-1.5-3.5"/><path d="M13 2v3h-3"/></svg>`,
    // A folder in front of every project row and a pin on the Pinned header;
    // the state dot stays, the folder says "project".
    folder: `<svg ${SVG_ATTRS}><path d="M2 4.5h4l1.5 1.5H14v7H2z"/></svg>`,
    pin: `<svg ${SVG_ATTRS}><path d="M6 2h4l-.5 4 2 2v1H4.5V8l2-2z"/><line x1="8" y1="9" x2="8" y2="14"/></svg>`,
    star: `<svg ${SVG_ATTRS}><path d="M8 2.2l1.7 3.6 3.9.5-2.9 2.7.8 3.9L8 11l-3.5 1.9.8-3.9L2.4 6.3l3.9-.5z"/></svg>`,
    close: `<svg ${SVG_ATTRS}><line x1="4" y1="4" x2="12" y2="12"/><line x1="12" y1="4" x2="4" y2="12"/></svg>`,
  };

  // The rail. Five icons, no labels, because these five are the only surfaces
  // that are not about one project: search, the Mesh, automations, the
  // observatory, and what is waiting on you.
  const RAIL_ITEMS = [
    // Order is André's (2026-09-15): what he checks first sits first.
    { key: 'observatory', label: 'Observatory' },
    // No dedicated Inbox SURFACE exists: open asks and approvals live in the
    // Mesh panel and, answerable in place, in the right pane's flow view
    // (right-pane-flowwatch.js). This opens the latter, so Inbox and Mesh are
    // two destinations rather than one destination behind two icons.
    { key: 'inbox', label: 'Inbox', open: openInbox },
    // Every live zellij session in one place, its own list rather than a
    // section among the projects ("less full"). The toggle lives on the
    // instance (assigned below), so the item names the global the way the
    // Memory entry does; typeof-guarded in openItem.
    { key: 'sessions', label: 'Sessions', global: 'xnautSidebarToggleSessions' },
    { key: 'search', label: 'Search' },
    { key: 'mesh', label: 'Mesh' },
    { key: 'automations', label: 'Automations' },
  ];

  // Everything the twelve rows used to reach that the rail does not. Each entry
  // calls exactly what its row called, so no destination is lost by this change
  // (XNAUT-337 and XNAUT-342 fold them into the workspace; that is not here).
  const MORE_ITEMS = [
    { key: 'agents', label: 'Agent Space' },
    { key: 'skills', label: 'Skills' },
    { key: 'plugins', label: 'Plugins' },
    { key: 'tasks', label: 'Tasks' },
    // No 'pm' entry: the standalone Projects panel is gone (XNAUT-342). Its
    // nine surfaces are tabs of a project's workspace, and the tree below is
    // how a project is chosen — a menu entry here would be a second selector
    // for a question this list has already answered.
    { key: 'delivery', label: 'Delivery' },
    // What xNAUT remembers (XNAUT-333). It opens its own panel rather than a
    // nav key, so the Delivery panel's Memory tab and this entry are one path.
    { key: 'memory', label: 'Memory', global: 'xnautOpenMemoryPanel' },
    { key: 'vault', label: 'Vault' },
  ];

  // The right pane has to exist before a view can be shown in it; both globals
  // are assigned (tasks-mode-glue.js, right-pane.js) and both are guarded,
  // because an unassigned window.* is a silent no-op and not a crash.
  function openInbox() {
    // The Mesh surface is where the open asks and approvals are answered; the
    // right pane's flow view is the same items in place. Both, so a click on
    // the badge always shows something even when the right pane was already
    // on that view (André, 2026-09-15: "the second icon with the 3 does not do
    // anything on click").
    navigate('mesh');
    if (typeof window.xnautEnsureRightPane === 'function') window.xnautEnsureRightPane();
    if (typeof window.xnautRightPaneShow !== 'function' || !window.xnautRightPaneShow('flowwatch')) {
      console.warn('[sidebar] right pane not mounted; cannot open the Inbox');
    }
  }

  // ---------- styles ----------
  function injectStyles() {
    if (document.getElementById('sidebar-styles')) return;
    const style = document.createElement('style');
    style.id = 'sidebar-styles';
    style.textContent = `
      .sbar-root { display: flex; flex-direction: column; height: 100%; min-height: 0; overflow: hidden;
        background: var(--editor-surface, #1b1b1f); color: var(--text-primary, #ddd);
        border-right: 1px solid var(--border-color, #333); font-size: 13px; user-select: none; }
      .sbar-nav { display: flex; flex-direction: column; padding: 8px 6px 4px; gap: 1px; }
      .sbar-master-toggle { align-self: flex-end; display:flex; align-items:center; justify-content:center; width:26px; height:26px;
        margin:0 2px 5px; border:0; border-radius:6px; background:transparent; color:var(--text-secondary,#aaa); cursor:pointer; font-size:16px; }
      .sbar-master-toggle:hover { background:var(--hover-bg,rgba(255,255,255,.08)); color:var(--text-primary,#fff); }
      .sbar-root[data-collapsed="1"] .sbar-nav { padding-left:5px; padding-right:5px; }
      .sbar-root[data-collapsed="1"] .sbar-master-toggle { align-self:center; margin-left:0; margin-right:0; }
      .sbar-root[data-collapsed="1"] .sbar-section-head,
      .sbar-root[data-collapsed="1"] .sbar-projects,
      .sbar-root[data-collapsed="1"] .sbar-usage-rows { display:none; }
      /* The rail. Icons only, always: no label appears at any width, which is
         what lets it stay one row deep next to a tree that needs the height. */
      .sbar-rail { display: flex; flex-wrap: wrap; align-items: center; gap: 2px; }
      .sbar-rail-btn { position: relative; display: flex; align-items: center; justify-content: center;
        width: 30px; height: 30px; padding: 0; border: 0; border-radius: 7px;
        background: transparent; color: var(--text-secondary, #aaa); cursor: pointer; }
      .sbar-rail-btn:hover { background: var(--hover-bg, rgba(255,255,255,0.08)); color: var(--text-primary, #fff); }
      .sbar-rail-btn.sbar-active { background: var(--active-bg, rgba(255,255,255,0.1)); color: var(--text-primary, #fff); }
      .sbar-rail-btn svg { width: 16px; height: 16px; }
      .sbar-rail-badge { position: absolute; top: 1px; right: 0; min-width: 15px; padding: 0 4px;
        border-radius: 999px; background: #f5b840; color: #0a0a0f; font-size: 9px; font-weight: 700;
        line-height: 14px; text-align: center; pointer-events: none; }
      .sbar-rail-badge[hidden] { display: none; }
      .sbar-root[data-collapsed="1"] .sbar-usage { justify-content:center; padding-left:5px; padding-right:5px; }
      .sbar-submenu { display:flex; flex-direction:column; min-height:0; height:100%; }
      .sbar-submenu[hidden] { display:none; }
      .sbar-root > [hidden] { display:none !important; }
      .sbar-submenu-head { display:flex; align-items:center; gap:7px; flex:0 0 40px; padding:0 8px;
        border-bottom:1px solid var(--border-color,#333); font-size:12px; font-weight:650; }
      .sbar-submenu-back { display:flex; align-items:center; justify-content:center; width:26px; height:26px; border:0;
        border-radius:6px; background:transparent; color:var(--text-secondary,#aaa); cursor:pointer; font-size:18px; }
      .sbar-submenu-back:hover { background:var(--hover-bg,rgba(255,255,255,.08)); color:var(--text-primary,#fff); }
      .sbar-submenu-body { display:flex; flex:1 1 0%; min-height:0; overflow:hidden; }
      .sbar-submenu-body > .vp-rail { flex:1 1 0% !important; width:100%; border-right:0; }
      .sbar-submenu-body .vp-collapse { display:none; }
      .sbar-icon-btn svg { width: 15px; height: 15px; flex: 0 0 auto; }
      .sbar-section-head { display: flex; align-items: center; justify-content: space-between;
        padding: 10px 14px 4px 14px; font-size: 11px; font-weight: 600; letter-spacing: 0.06em;
        text-transform: uppercase; color: var(--text-muted, #777); }
      .sbar-section-head.sbar-collapsible { cursor: pointer; }
      .sbar-section-head.sbar-collapsible:hover { color: var(--text-secondary, #9aa0aa); }
      .sbar-head-label { display: flex; align-items: center; gap: 4px; }
      .sbar-caret { display: inline-block; width: 9px; font-size: 9px; line-height: 1; opacity: .7; transition: transform .15s ease; }
      .sbar-section-head.sbar-collapsed .sbar-caret { transform: rotate(-90deg); }
      .sbar-icon-btn { display: flex; align-items: center; justify-content: center; width: 22px; height: 22px;
        border: none; border-radius: 5px; background: transparent; color: var(--text-secondary, #aaa); cursor: pointer; padding: 0; }
      .sbar-icon-btn:hover { background: var(--hover-bg, rgba(255,255,255,0.08)); color: var(--text-primary, #fff); }
      .sbar-head-actions { display: flex; align-items: center; gap: 2px; }
      .sbar-projects { flex: 1 1 0%; min-height: 0; overflow-y: auto; padding: 2px 6px 8px; }
      /* ---- the tree ---- */
      .sbar-children[hidden] { display: none; }
      .sbar-twist { flex: 0 0 auto; width: 14px; margin-top: 2px; padding: 0; border: 0; background: transparent;
        color: inherit; font: inherit; font-size: 9px; line-height: 1; opacity: .65; cursor: pointer; text-align: left; }
      .sbar-twist:hover { opacity: 1; }
      .sbar-twist-spacer { flex: 0 0 auto; width: 14px; }
      .sbar-group > .sbar-row { padding-left: 8px; }
      .sbar-wt { align-items: center; padding-left: 22px; }
      .sbar-wt .sbar-name { font-size: 12px; }
      .sbar-word { flex: 0 0 auto; font-size: 10px; color: var(--text-muted, #777); }
      .sbar-star { flex: 0 0 auto; display: flex; align-items: center; justify-content: center; width: 18px; height: 18px;
        padding: 0; border: 0; border-radius: 4px; background: transparent; color: var(--text-muted, #666);
        cursor: pointer; opacity: 0; }
      .sbar-row:hover .sbar-star, .sbar-star[aria-pressed="true"] { opacity: 1; }
      .sbar-star:hover { color: var(--text-primary, #fff); }
      .sbar-star svg { width: 12px; height: 12px; }
      .sbar-more-proj { flex: 0 0 auto; width: 18px; height: 18px; padding: 0; border: 0; border-radius: 4px;
        background: transparent; color: var(--text-secondary, #7e838d); font: inherit; line-height: 1;
        cursor: pointer; opacity: 0; }
      .sbar-row:hover .sbar-more-proj, .sbar-more-proj:focus-visible { opacity: 1; }
      .sbar-more-proj:hover { background: var(--hover-bg, rgba(255,255,255,.08)); color: var(--text-primary, #fff); }
      .sbar-modal { position: fixed; inset: 0; z-index: 10001; display: flex; align-items: center;
        justify-content: center; background: rgba(0,0,0,.55); }
      .sbar-sheet { width: min(460px, 92vw); max-height: 88vh; overflow: auto; padding: 20px 22px;
        border-radius: 12px; border: 1px solid var(--border-color, #2f323a);
        background: var(--bg-secondary, #161a21); color: var(--text-primary, #e7eaf0);
        box-shadow: 0 18px 50px rgba(0,0,0,.5); }
      .sbar-sheet h2 { margin: 0 0 2px; font-size: 17px; font-weight: 650; }
      .sbar-sheet-sub { margin: 0 0 16px; font-size: 12.5px; color: var(--text-secondary, #8f949e); }
      .sbar-lbl { display: block; margin-bottom: 13px; font-size: 11.5px; letter-spacing: .05em;
        text-transform: uppercase; color: var(--text-secondary, #8f949e); }
      .sbar-in { display: block; width: 100%; margin-top: 5px; padding: 8px 10px; border-radius: 7px;
        border: 1px solid var(--border-color, #2f323a); background: var(--bg-primary, #0f1216);
        color: var(--text-primary, #e7eaf0); font: inherit; font-size: 13px; text-transform: none;
        letter-spacing: normal; }
      .sbar-in:focus { outline: 2px solid var(--accent, #4f8cff); outline-offset: 1px; }
      .sbar-ta { min-height: 76px; resize: vertical; }
      .sbar-check { display: flex; align-items: center; gap: 8px; margin: 4px 0 12px; font-size: 13px;
        color: var(--text-primary, #e7eaf0); cursor: pointer; }
      .sbar-branch-row[hidden] { display: none; }
      .sbar-hint { margin: -7px 0 13px; font-size: 11.5px; color: var(--text-secondary, #7e838d); }
      .sbar-sheet-actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 4px; }
      .sbar-btn { padding: 7px 14px; border-radius: 7px; border: 1px solid var(--border-color, #2f323a);
        background: transparent; color: var(--text-primary, #e7eaf0); font: inherit; font-size: 13px;
        cursor: pointer; }
      .sbar-btn:hover { background: var(--hover-bg, rgba(255,255,255,.07)); }
      .sbar-btn-primary { background: var(--accent, #4f8cff); border-color: transparent; color: #fff; }
      .sbar-btn-primary:disabled { opacity: .6; cursor: default; }
      .sbar-err { margin: 12px 0 0; font-size: 12.5px; color: var(--danger, #e5534b); }
      .sbar-star[aria-pressed="true"] { color: #f5b840; }
      .sbar-star[aria-pressed="true"] svg { fill: currentColor; }
      /* Uncommitted work: amber, the same amber the row hairlines use. */
      .sbar-dot.sbar-dirty { border-radius: 2px; background: #f5b840; }
      /* Nothing to say: present, hollow, silent. */
      .sbar-dot.sbar-clean { background: transparent; box-shadow: inset 0 0 0 1.5px #4a4f58; }
      .sbar-wt-more { display: flex; align-items: center; gap: 6px; margin: 1px 0 2px 22px; padding: 5px 8px;
        border-radius: 6px; color: var(--text-muted, #666); font-size: 11px; cursor: pointer; }
      .sbar-wt-more:hover { background: rgba(255,255,255,.05); color: var(--text-secondary, #a0a5af); }
      .sbar-wt-more .sbar-star { opacity: 1; }
      .sbar-kind { flex: 0 0 auto; display: flex; align-items: center; margin-top: 2px; color: var(--text-muted, #7a808a); }
      .sbar-kind svg { width: 13px; height: 13px; }
      .sbar-row-active .sbar-kind { color: var(--text-primary, #fff); }
      .sbar-pinned-head { display: flex; align-items: center; gap: 8px; width: 100%; padding: 6px 8px; border: 0;
        background: transparent; color: var(--text-secondary, #a0a5af); font: inherit; font-weight: 600; cursor: pointer;
        text-align: left; border-radius: 6px; }
      .sbar-pinned-head:hover { background: rgba(255,255,255,.05); color: var(--text-primary, #fff); }
      .sbar-pinned-head .sbar-kind { margin-top: 0; }
      .sbar-pinned-head[aria-expanded="false"] { opacity: .7; }
      /* Indented under its header, and a gap before the rest instead of a
         label: the gap is the divider. */
      .sbar-pinned { padding-left: 14px; margin-bottom: 10px; }
      .sbar-sub-label { padding: 6px 8px 2px; font-size: 10px; letter-spacing: 0.05em; text-transform: uppercase;
        color: var(--text-muted, #666); }
      .sbar-row { display: flex; align-items: flex-start; gap: 8px; padding: 6px 8px; border-radius: 6px; cursor: pointer; }
      .sbar-row:hover { background: var(--hover-bg, rgba(255,255,255,0.06)); }
      /* The bracket is the brand yellow, the same one the star, the release
         list and the status bar use; the mint it had was the agent-thinking
         colour and the only place it appeared (Andre, 2026-09-14). */
      .sbar-row-active { background: var(--active-bg, rgba(255,255,255,0.1)); box-shadow: inset 2px 0 0 #f5b840; }
      .sbar-dot { flex: 0 0 auto; width: 7px; height: 7px; margin-top: 5px; border-radius: 50%;
        background: var(--dot-off, #555); }
      .sbar-dot.sbar-on { background: var(--dot-on, #3fb950); }
      .sbar-sessions { flex: 1 1 auto; min-height: 0; overflow-y: auto; padding: 0 6px 4px; }
      .sbar-section-head[hidden], .sbar-projects[hidden] { display: none; }
      .sbar-sess-fold { margin-top: 6px; }
      .sbar-sess-fold > summary { cursor: pointer; list-style: none; padding: 6px 8px 2px; font-size: 10px; letter-spacing: 0.05em; text-transform: uppercase; color: var(--text-muted, #7a808a); }
      .sbar-sess-fold > summary::-webkit-details-marker { display: none; }
      .sbar-sessions[hidden] { display: none; }
      .sbar-sess-state { flex: 0 0 auto; margin-left: auto; font-size: 10px; color: var(--text-muted, #7a808a); text-transform: lowercase; }
      .sbar-sess.sbar-exited .sbar-name { color: var(--text-muted, #7a808a); }
      .sbar-sess .sbar-name { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
      .sbar-sess .sbar-text { flex: 1 1 auto; min-width: 0; display: flex; flex-direction: column; }
      .sbar-sess .sbar-sub { font-size: 11px; color: var(--text-muted, #777); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
      .sbar-head-count { margin-left: 6px; font-size: 10px; color: var(--text-muted, #7a808a); }
      .sbar-sess-auto .sbar-name { color: #6ea8fe; }
      .sbar-sess-manual .sbar-name { color: #f5b840; }
      .sbar-row-main { flex: 1 1 auto; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
      .sbar-row-top { display: flex; align-items: center; gap: 6px; min-width: 0; }
      .sbar-name { flex: 1 1 auto; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
      .sbar-chip { flex: 0 0 auto; font-size: 10px; padding: 1px 6px; border-radius: 8px;
        background: var(--chip-bg, rgba(255,255,255,0.08)); color: var(--text-secondary, #999); }
      .sbar-branch { font-size: 11px; color: var(--text-muted, #777); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
      /* Separated by a dot only when there is a branch to separate it FROM. */
      .sbar-ago { color: var(--text-muted, #666); }
      .sbar-branch-name:not(:empty) + .sbar-ago:not(:empty)::before { content: ' · '; }
      .sbar-empty { padding: 10px 8px; color: var(--text-muted, #666); font-size: 12px; }
      .sbar-chip.sbar-sess { background: rgba(120,180,255,.12); color: #7fb2ff; border-color: transparent; }
      /* State as a hairline around the row. One pixel on purpose: it should be
         readable in peripheral vision without competing with the text. Only the
         states worth reacting to get one — idle projects stay unmarked, or the
         whole rail turns into noise you learn to ignore. */
      .sbar-row[data-state] { box-shadow: inset 0 0 0 1px var(--sbar-state, transparent); }
      /* Needs attention: the row outlines itself in yellow and pulses. A pulse
         reads as "still waiting" — it repeats without implying progress, which
         is what a travelling snake implies. Same 1px weight as the other states. */
      .sbar-row { position: relative; }
      .sbar-row[data-state="attention"] { --sbar-state: transparent; }
      .sbar-row[data-state="attention"]::after {
        content: ''; position: absolute; inset: 0; border-radius: 6px;
        pointer-events: none; box-sizing: border-box;
        box-shadow: inset 0 0 0 1px #ffd166;
        animation: sbar-pulse 1.6s ease-in-out infinite; }
      @keyframes sbar-pulse { 0%, 100% { opacity: 1; } 50% { opacity: .28; } }
      @media (prefers-reduced-motion: reduce) {
        .sbar-row[data-state="attention"]::after { animation: none; }
      }
      .sbar-row[data-state="working"]   { --sbar-state: rgba(245,184,64,.75); }
      .sbar-row[data-state="waiting"]   { --sbar-state: rgba(245,184,64,.40); }
      .sbar-row[data-state="done"]      { --sbar-state: rgba(63,185,80,.55); }
      .sbar-row[data-state="live"]      { --sbar-state: rgba(140,146,158,.28); }
      .sbar-row[data-state="exited"]    { --sbar-state: rgba(140,146,158,.22); }
      /* Session rows: no fill, a hairline in the kind's colour (André,
         2026-09-15): yellow around his own terminals, blue around the bots.
         The active one gets the same line, two pixels. */
      .sbar-sess.sbar-sess-auto { --sbar-state: #6ea8fe; }
      .sbar-sess.sbar-sess-manual { --sbar-state: #f5b840; }
      .sbar-sess.sbar-exited { --sbar-state: transparent; }
      .sbar-sess.sbar-row-active { background: transparent; box-shadow: inset 0 0 0 2px var(--sbar-state, transparent); }
      /* A conic-gradient sweeps by ANGLE, so it rotates like a pie no matter what
         shape you mask it into — that is why the last version still read as a
         circle. This moves an actual block around the four edges instead: eight
         positions, stepped, so it hops corner to corner along a square path. */
      .sbar-dot.sbar-run { position: relative; width: 12px; height: 12px; margin-top: 3px;
        border-radius: 2px; background: transparent;
        box-shadow: inset 0 0 0 1px rgba(77,163,255,.22); animation: none; }
      .sbar-dot.sbar-run::before, .sbar-dot.sbar-run::after {
        content: ''; position: absolute; width: 3px; height: 3px; border-radius: 1px;
        background: #4da3ff; animation: sbar-square 1.2s steps(1) infinite; }
      /* the tail: same path, one step behind, dimmer */
      .sbar-dot.sbar-run::after { background: rgba(77,163,255,.35); animation-delay: -0.15s; }
      /* The same snake in the owner's colour on his own rows. */
      .sbar-sess-manual .sbar-dot.sbar-run { box-shadow: inset 0 0 0 1px rgba(245,184,64,.22); }
      .sbar-sess-manual .sbar-dot.sbar-run::before { background: #f5b840; }
      .sbar-sess-manual .sbar-dot.sbar-run::after { background: rgba(245,184,64,.35); }
      @keyframes sbar-square {
        0%    { top: 0;     left: 0; }
        12.5% { top: 0;     left: 4.5px; }
        25%   { top: 0;     left: 9px; }
        37.5% { top: 4.5px; left: 9px; }
        50%   { top: 9px;   left: 9px; }
        62.5% { top: 9px;   left: 4.5px; }
        75%   { top: 9px;   left: 0; }
        87.5% { top: 4.5px; left: 0; }
        100%  { top: 0;     left: 0; }
      }
      /* Alive but nothing known to be happening — present, quiet, no motion. */
      .sbar-dot.sbar-live { border-radius: 2px; background: #8c929e; }
      /* Needs a human. The only state allowed to be loud. */
      .sbar-dot.sbar-attention { border-radius: 2px; background: #ff5f56;
        box-shadow: 0 0 0 3px rgba(255,95,86,.18); }
      /* Resurrectable but not running — a hollow square, no motion. */
      .sbar-dot.sbar-exited { width: 10px; height: 10px; margin-top: 4px; border-radius: 2px;
        background: transparent; box-shadow: inset 0 0 0 1.5px #5c626c; }
      @media (prefers-reduced-motion: reduce) {
        .sbar-dot.sbar-run { animation: none; background-image: none; background: #4da3ff; }
      }
      .sbar-hidden-toggle { padding: 7px 8px; margin-top: 2px; color: var(--text-muted, #666);
        font-size: 11px; cursor: pointer; border-radius: 6px; user-select: none; }
      .sbar-hidden-toggle:hover { background: rgba(255,255,255,.05); color: var(--text-secondary, #a0a5af); }
      .sbar-usage { flex: 0 0 auto; display: flex; align-items: center; gap: 6px; padding: 8px 10px;
        border-top: 1px solid var(--border-color, #333); }
      .sbar-usage-rows { flex: 1 1 auto; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
      .sbar-usage-row { font-size: 11px; color: var(--text-secondary, #999); overflow: hidden;
        text-overflow: ellipsis; white-space: nowrap; }
      .sbar-usage-row.sbar-muted { color: var(--text-muted, #666); }
      .sbar-menu { position: fixed; z-index: 10000; min-width: 160px; padding: 4px;
        background: var(--menu-bg, #222227); border: 1px solid var(--border-color, #3a3a40);
        border-radius: 8px; box-shadow: 0 8px 24px rgba(0,0,0,0.45); font-size: 13px; }
      .sbar-menu-item { padding: 6px 10px; border-radius: 5px; cursor: pointer; color: var(--text-primary, #ddd); }
      .sbar-menu-item:hover { background: var(--hover-bg, rgba(255,255,255,0.08)); }
      .sbar-menu-item.sbar-danger { color: var(--danger, #e5534b); }
    `;
    document.head.appendChild(style);
  }

  // ---------- context menu ----------
  let menuEl = null;
  function closeMenu() {
    if (menuEl && menuEl.parentNode) menuEl.parentNode.removeChild(menuEl);
    menuEl = null;
  }
  function openMenu(x, y, items) {
    closeMenu();
    menuEl = document.createElement('div');
    menuEl.className = 'sbar-menu';
    for (const it of items) {
      const row = document.createElement('div');
      row.className = 'sbar-menu-item' + (it.danger ? ' sbar-danger' : '');
      row.textContent = it.label;
      row.addEventListener('click', (e) => { e.stopPropagation(); closeMenu(); it.action(); });
      menuEl.appendChild(row);
    }
    document.body.appendChild(menuEl);
    // Keep on-screen.
    window.xnautPlaceAtClick(menuEl, x, y);
  }
  function onDocMouseDown(e) {
    if (menuEl && !menuEl.contains(e.target)) closeMenu();
  }

  // Shared so the Projects panel gets the same right-click menu instead of a
  // second implementation that looks almost but not quite the same.
  window.xnautContextMenu = (x, y, items) => openMenu(x, y, items);

  // The usage-shape normaliser that used to live here parsed ~/.flowai/usage.json,
  // a file nothing writes; it went with that read (XNAUT-257).

  // ---------- controller ----------
  let current = null; // last-mounted controller internals

  function mountSidebar(host) {
    if (!host) throw new Error('xnautMountSidebar: host element required');
    if (current) current.destroy(); // calling twice re-renders
    injectStyles();

    const state = {
      activeNav: 'mesh',
      destroyed: false,
      tasks: [],
      projects: [],
      runs: [],
      agentSessions: [],
      // repo path -> its worktrees, read once per repo and kept for the session.
      worktrees: new Map(),
      openGroups: new Set(),
      showAllWt: new Set(),
      scope: null,
    };
    // The selected project/worktree pair survives a reload, because a reload is
    // not a change of mind about which branch you are working in.
    try {
      const saved = JSON.parse(localStorage.getItem(SCOPE_KEY) || 'null');
      if (saved && typeof saved === 'object') state.scope = saved;
    } catch (_) { /* unreadable; start with no scope */ }
    window.xnautActiveScope = state.scope;
    // Two shapes of the same fact, both assigned, so a caller cannot reach for
    // the value and get a function object (which is truthy, and then wrong).
    window.xnautGetActiveScope = () => state.scope;
    host.innerHTML = '';

    const root = document.createElement('div');
    root.className = 'sbar-root';

    const submenu = document.createElement('section');
    submenu.className = 'sbar-submenu';
    submenu.hidden = true;
    submenu.innerHTML = '<header class="sbar-submenu-head"><button class="sbar-submenu-back" title="Back to main menu" aria-label="Back to main menu">‹</button><span></span></header><div class="sbar-submenu-body"></div>';

    // The icon rail.
    const nav = document.createElement('div');
    nav.className = 'sbar-nav';
    const navEls = {};
    const masterToggle = document.createElement('button');
    masterToggle.className = 'sbar-master-toggle';
    const applyMasterCollapsed = (collapsed) => {
      root.dataset.collapsed = collapsed ? '1' : '0';
      host.style.width = collapsed ? '52px' : `${Number(localStorage.getItem('xnaut-sidebar-expanded-width')) || 240}px`;
      host.style.minWidth = collapsed ? '52px' : '200px';
      host.style.maxWidth = collapsed ? '52px' : '320px';
      masterToggle.textContent = collapsed ? '›' : '‹';
      masterToggle.title = collapsed ? 'Expand main menu' : 'Collapse main menu';
      masterToggle.setAttribute('aria-label', masterToggle.title);
      localStorage.setItem('xnaut-sidebar-collapsed', collapsed ? '1' : '0');
    };
    masterToggle.addEventListener('click', () => {
      const collapsed = root.dataset.collapsed === '1';
      if (!collapsed) localStorage.setItem('xnaut-sidebar-expanded-width', String(Math.round(host.getBoundingClientRect().width) || 240));
      applyMasterCollapsed(!collapsed);
    });
    nav.appendChild(masterToggle);

    // Open a destination the way its old row opened it. An item may name the
    // global that opens it instead of routing through xnautSidebarNavigate,
    // whose switch would warn on an unknown key. The typeof guard is the
    // point: an unassigned window.* is a silent no-op, not a crash.
    function openItem(item) {
      if (typeof item.open === 'function') return item.open();
      if (item.global) {
        if (typeof window[item.global] === 'function') return window[item.global]();
        console.warn('[sidebar] ' + item.global + ' is not assigned; cannot open', item.label);
        return undefined;
      }
      return navigate(item.key);
    }

    const rail = document.createElement('div');
    rail.className = 'sbar-rail';
    for (const item of RAIL_ITEMS) {
      const btn = document.createElement('button');
      btn.className = 'sbar-rail-btn';
      btn.dataset.rail = item.key;
      btn.title = item.label;
      btn.setAttribute('aria-label', item.label);
      // No <span> label, at any width: the rail is icons, and the name lives
      // in the accessible name and the tooltip.
      btn.innerHTML = `${ICONS[item.key] || ''}<span class="sbar-rail-badge" data-badge hidden></span>`;
      btn.addEventListener('click', () => {
        state.activeNav = item.key;
        for (const k of Object.keys(navEls)) navEls[k].classList.toggle('sbar-active', k === state.activeNav);
        openItem(item);
      });
      navEls[item.key] = btn;
      rail.appendChild(btn);
    }

    // The surfaces the rail does not carry. They are not gone; they are one
    // click deeper, and each still calls exactly what its row called.
    const moreBtn = document.createElement('button');
    moreBtn.className = 'sbar-rail-btn sbar-rail-more';
    moreBtn.title = 'More surfaces';
    moreBtn.setAttribute('aria-label', 'More surfaces');
    moreBtn.innerHTML = ICONS.more;
    moreBtn.addEventListener('click', (event) => {
      const box = moreBtn.getBoundingClientRect();
      openMenu(event.clientX || box.left, event.clientY || box.bottom, MORE_ITEMS.map((item) => ({
        label: item.label,
        action: () => {
          state.activeNav = null;
          for (const k of Object.keys(navEls)) navEls[k].classList.remove('sbar-active');
          openItem(item);
        },
      })));
    });
    rail.appendChild(moreBtn);
    nav.appendChild(rail);
    if (navEls[state.activeNav]) navEls[state.activeNav].classList.add('sbar-active');
    root.appendChild(nav);
    applyMasterCollapsed(localStorage.getItem('xnaut-sidebar-collapsed') === '1');

    // Inbox badge: how many items are actually waiting on André. It reads the
    // same store the panel reads and refreshes on inbox-changed, so the count
    // can never drift from the list it claims to summarise. It sits on Inbox
    // rather than Mesh now, because the count is of things waiting for a human
    // and Inbox is the icon that says so.
    async function refreshMeshBadge() {
      const row = navEls.inbox;
      if (!row || state.destroyed) return;
      const badge = row.querySelector('[data-badge]');
      if (!badge) return;
      let count = 0;
      try {
        const open = (await invoke('inbox_list', { project: null, status: 'open' })) || [];
        count = open.length;
      } catch (_) { count = 0; }
      badge.textContent = count > 99 ? '99+' : String(count);
      badge.hidden = count === 0;
    }
    refreshMeshBadge();
    let meshBadgeOff = null;
    try {
      Promise.resolve(window.__TAURI__.event.listen('inbox-changed', refreshMeshBadge))
        .then((off) => { meshBadgeOff = off; if (state.destroyed) { try { off(); } catch (_) {} } })
        .catch(() => {});
    } catch (_) { /* event API missing — the badge just stays static */ }
    state.disposeMeshBadge = () => { if (meshBadgeOff) { try { meshBadgeOff(); } catch (_) {} } };

    // Active-project highlight (Orca/CMUX): called by app.js setActiveProject.
    // id null → Home/global (restore nav highlight, clear project highlight).
    window.xnautSidebarSetActiveProject = (id) => {
      state.activeProjectId = id || null;
      list.querySelectorAll('.sbar-row').forEach((r) => {
        r.classList.toggle('sbar-row-active', !!id && r.dataset.taskId === id);
      });
      for (const k of Object.keys(navEls)) {
        navEls[k].classList.toggle('sbar-active', !id && k === state.activeNav);
      }
    };

    // ─── Sessions (2026-09-15) ───────────────────────────────────────────────
    // André: "I have x sessions open and work in parallel on stuff." Every live
    // zellij session on this machine, in one place above the projects: the ones
    // the app launched carry their agent's status word, the owner's own (cx-*,
    // cl-*) carry only their age, because nothing reports on them (XNAUT-402).
    // Click opens or focuses the tab; right-click closes. Never automatic.
    const sessHead = document.createElement('div');
    sessHead.className = 'sbar-section-head';
    sessHead.innerHTML = `<span class="sbar-head-label"><span>Sessions</span><span class="sbar-head-count" data-sess-count></span></span>`
      + `<span class="sbar-head-actions"><button class="sbar-icon-btn" data-sess-plus title="New terminal" aria-label="New terminal">${ICONS.plus}</button></span>`;
    // The + opens a plain terminal tab. Not a zellij session: attaching one from
    // inside another nests them, which nobody wants, and a shell is what a new
    // tab should be. The list only tracks zellij sessions, so nothing to refresh.
    sessHead.querySelector('[data-sess-plus]').addEventListener('click', (event) => {
      event.stopPropagation();
      if (typeof window.createNewTab !== 'function') { console.warn('[sidebar] createNewTab is not assigned; cannot open a terminal'); return; }
      window.createNewTab();
    });
    root.appendChild(sessHead);
    const sessList = document.createElement('div');
    sessList.className = 'sbar-sessions';
    root.appendChild(sessList);
    // The body shows one list at a time. `applyView` runs once the projects
    // header and list exist (below), and again on every rail click.
    state.busyUntil = new Map();
    state.view = localStorage.getItem(SIDEBAR_VIEW_KEY) === 'sessions' ? 'sessions' : 'projects';
    function applyView() {
      const sessions = state.view === 'sessions';
      sessHead.hidden = !sessions;
      sessList.hidden = !sessions;
      if (state.projectsHead) state.projectsHead.hidden = sessions;
      if (state.projectsList) state.projectsList.hidden = sessions;
      if (navEls.sessions) navEls.sessions.classList.toggle('sbar-active', sessions);
    }
    window.xnautSidebarToggleSessions = toggleSessionsView;
    function toggleSessionsView() {
      state.view = state.view === 'sessions' ? 'projects' : 'sessions';
      localStorage.setItem(SIDEBAR_VIEW_KEY, state.view);
      if (state.view === 'projects') {
        state.activeNav = null;
        for (const k of Object.keys(navEls)) navEls[k].classList.remove('sbar-active');
      }
      applyView();
    }

    // Open the session's tab: the one already attached if there is one, else
    // a fresh `zellij attach`. Both globals live in app.js and are guarded, so
    // in the stub page a missing one is a console warning, not a throw.
    function openSession(name) {
      state.activeSession = name;
      sessList.querySelectorAll('.sbar-sess').forEach((r) => r.classList.toggle('sbar-row-active', r.dataset.session === name));
      // The host tab is the one place a session shows; the list is the switcher.
      if (typeof window.xnautShowSessionInHost === 'function') return window.xnautShowSessionInHost(name);
      if (typeof window.xnautOpenZellijSession === 'function') return window.xnautOpenZellijSession(name, { focus: true });
      console.warn('[sidebar] xnautShowSessionInHost is not assigned; cannot open', name);
    }
    function renderSessions() {
      sessList.innerHTML = '';
      const owned = new Map();
      for (const a of state.agentSessions || []) {
        if (a && a.zellij_session) owned.set(a.zellij_session, a);
      }
      // The app's launches carry the xnaut- prefix; everything else is the
      // owner's, adopted or not, so his rename and his colour hold on it.
      const isAuto = (s) => /^xnaut-/.test(s.name || '');
      const rank = (s) => (s.exited ? 2 : (isAuto(s) ? 0 : 1));
      // An exited xnaut-* session is a dead name: the app prunes it within a
      // minute and nothing can bring the run back through it. The owner's own
      // exited sessions stay, folded, because those are resurrected on purpose.
      const sessions = (state.sessions || []).filter((s) => !(s.exited && /^xnaut-/.test(s.name || ''))).sort((a, b) => rank(a) - rank(b) || (b.last_active_ms || 0) - (a.last_active_ms || 0));
      const liveCount = sessions.filter((s) => !s.exited).length;
      sessHead.querySelector('[data-sess-count]').textContent = liveCount ? String(liveCount) : '';
      const badge = navEls.sessions && navEls.sessions.querySelector('[data-badge]');
      if (badge) { badge.textContent = liveCount > 99 ? '99+' : String(liveCount); badge.hidden = liveCount === 0; }
      if (!sessions.length) {
        const empty = document.createElement('div');
        empty.className = 'sbar-empty';
        empty.textContent = 'No sessions';
        sessList.appendChild(empty);
        return;
      }
      // Exited sessions can be resurrected, so they are one click away rather
      // than gone, but they are not what this list is for: folded, and last.
      const fold = document.createElement('details');
      fold.className = 'sbar-sess-fold';
      const exitedCount = sessions.length - liveCount;
      fold.innerHTML = `<summary>Exited · ${exitedCount}</summary>`;
      for (const s of sessions) {
        const agent = owned.get(s.name);
        const row = document.createElement('div');
        row.className = 'sbar-row sbar-sess' + (s.exited ? ' sbar-exited' : (isAuto(s) ? ' sbar-sess-auto' : ' sbar-sess-manual'));
        row.dataset.session = s.name;
        if (state.activeSession === s.name) row.classList.add('sbar-row-active');
        // The state word is the agent's own status when the app launched the
        // session; a session it only sees says "live" or "exited", nothing more.
        // His own sessions have no hooks; the server's CPU says whether the
        // agent inside is working (zellij.rs busy_sessions).
        // CPU is sampled every five seconds and a working agent idles between
        // turns, so a plain read flapped live/working. Busy sticks for 15 s.
        const now = Date.now();
        if (s.busy) state.busyUntil.set(s.name, now + 15000);
        const busy = s.busy || (state.busyUntil.get(s.name) || 0) > now;
        const word = s.exited ? 'exited' : (agent ? String(agent.status || 'live') : (busy ? 'working' : 'live'));
        row.dataset.state = word;
        let dot = ' sbar-live';
        if (s.exited) dot = ' sbar-exited';
        else if (word === 'permission' || word === 'blocked') dot = ' sbar-attention';
        else if (word === 'working') dot = ' sbar-run';
        const alias = typeof window.xnautSessionAlias === 'function' ? window.xnautSessionAlias(s.name) : '';
        // The owner's name for a session wins over the agent's label: the app
        // adopts his own sessions too, and a rename must show on the row.
        const label = alias || (agent && agent.label) || s.name;
        const sub = (agent || alias) ? s.name : (s.created ? `since ${s.created}` : '');
        row.innerHTML = `<span class="sbar-dot${dot}"></span>`
          + `<span class="sbar-text"><span class="sbar-name"></span><span class="sbar-sub"></span></span>`
          + `<span class="sbar-sess-state">${word}</span>`;
        row.querySelector('.sbar-name').textContent = label;
        row.querySelector('.sbar-sub').textContent = sub;
        row.title = s.exited ? `${s.name}: exited, attach to resurrect` : s.name;
        row.addEventListener('click', () => openSession(s.name));
        // Double-click renames, like the tab name at the top.
        row.addEventListener('dblclick', async (event) => {
          event.preventDefault();
          if (typeof window.xnautPromptDialog !== 'function' || typeof window.xnautRenameSession !== 'function') return;
          const answer = await window.xnautPromptDialog(`Name for ${s.name}`, alias || '', 'Rename');
          if (answer === null || answer === undefined || answer === false) return;
          window.xnautRenameSession(s.name, String(answer));
        });
        row.addEventListener('contextmenu', (event) => {
          event.preventDefault();
          openMenu(event.clientX, event.clientY, [
            { label: s.exited ? 'Resurrect' : 'Open', action: () => openSession(s.name) },
            {
              label: 'Rename',
              action: async () => {
                // window.prompt is a no-op in this webview; the app's own dialog answers.
                if (typeof window.xnautPromptDialog !== 'function' || typeof window.xnautRenameSession !== 'function') return;
                const answer = await window.xnautPromptDialog(`Name for ${s.name}`, alias || '', 'Rename');
                if (answer === null || answer === undefined || answer === false) return;
                window.xnautRenameSession(s.name, String(answer));
              },
            },
            {
              label: 'Close session', danger: true,
              action: async () => {
                // The app's own sessions have a reaper; a person's session is
                // closed only by a person, and only after the question, which
                // window.confirm cannot ask here (it is a no-op in this webview).
                if (!s.exited) {
                  if (typeof window.xnautConfirmDialog !== 'function') return;
                  const yes = await window.xnautConfirmDialog(`Close zellij session ${s.name}?`, 'Close');
                  if (!yes) return;
                }
                invoke('zellij_delete_session', { name: s.name }).then(() => refresh()).catch((e) => console.error('[sidebar] close session failed:', e));
              },
            },
          ]);
        });
        (s.exited ? fold : sessList).appendChild(row);
      }
      if (exitedCount) sessList.appendChild(fold);
    }

    // Projects header (collapsible), with a gear and a plus.
    const head = document.createElement('div');
    head.className = 'sbar-section-head sbar-collapsible';
    head.innerHTML = `<span class="sbar-head-label"><span class="sbar-caret">▾</span><span>Projects</span></span>`
      + `<span class="sbar-head-actions">`
      + `<button class="sbar-icon-btn" data-head-gear title="Project list options" aria-label="Project list options">${ICONS.gear}</button>`
      + `<button class="sbar-icon-btn" data-head-plus title="New project" aria-label="New project">${ICONS.plus}</button>`
      + `</span>`;
    head.querySelector('[data-head-plus]').addEventListener('click', (event) => {
      event.stopPropagation();
      navigate('new-project');
    });
    head.querySelector('[data-head-gear]').addEventListener('click', (event) => {
      event.stopPropagation();
      // "Manage projects" opened the standalone Projects panel and is gone with
      // it (XNAUT-342). What is left here is about THIS list — what it shows and
      // how fresh it is. Managing one project happens in its workspace.
      openMenu(event.clientX, event.clientY, [
        {
          label: state.onlyPinned ? 'Show all projects' : 'Show only pinned',
          action: () => { setOnlyPinned(!state.onlyPinned); },
        },
        // The only way back from hiding a project, kept from the row that used
        // to carry it: without it, Hide is a one-way door.
        {
          label: state.showHidden ? 'Hide hidden projects again' : 'Show hidden projects',
          action: () => { state.showHidden = !state.showHidden; renderProjects(); },
        },
        { label: 'Refresh', action: () => refresh() },
      ]);
    });
    root.appendChild(head);

    // Scrolling project list.
    const list = document.createElement('div');
    list.className = 'sbar-projects';
    root.appendChild(list);
    state.projectsHead = head;
    state.projectsList = list;
    applyView();

    // Collapse the Projects list (persisted, toggled by clicking the header).
    const PROJECTS_COLLAPSE_KEY = 'xnaut-projects-collapsed';
    const applyProjectsCollapsed = (c) => {
      list.style.display = c ? 'none' : '';
      head.classList.toggle('sbar-collapsed', c);
    };
    let projectsCollapsed = localStorage.getItem(PROJECTS_COLLAPSE_KEY) === '1';
    applyProjectsCollapsed(projectsCollapsed);
    head.addEventListener('click', () => {
      projectsCollapsed = !projectsCollapsed;
      localStorage.setItem(PROJECTS_COLLAPSE_KEY, projectsCollapsed ? '1' : '0');
      applyProjectsCollapsed(projectsCollapsed);
    });
    state.onlyPinned = localStorage.getItem(ONLY_PINNED_KEY) === '1';
    function setOnlyPinned(on) {
      state.onlyPinned = !!on;
      try { localStorage.setItem(ONLY_PINNED_KEY, on ? '1' : '0'); } catch (_) { /* quota; ignore */ }
      renderProjects();
    }

    // Usage strip.
    const usage = document.createElement('div');
    usage.className = 'sbar-usage';
    const usageRows = document.createElement('div');
    usageRows.className = 'sbar-usage-rows';
    const usageBtn = document.createElement('button');
    usageBtn.className = 'sbar-icon-btn';
    usageBtn.title = 'Refresh usage';
    usageBtn.setAttribute('aria-label', 'Refresh plan usage');
    usageBtn.innerHTML = ICONS.refresh;
    usageBtn.addEventListener('click', () => loadUsage());
    usage.appendChild(usageRows);
    usage.appendChild(usageBtn);
    root.appendChild(usage);
    root.appendChild(submenu);

    const mainSections = [nav, head, list, usage];
    let collapsedBeforeSubmenu = false;
    const closeSubmenu = () => {
      submenu.hidden = true;
      mainSections.forEach((section) => { section.hidden = false; });
      applyMasterCollapsed(collapsedBeforeSubmenu);
    };
    submenu.querySelector('.sbar-submenu-back').onclick = closeSubmenu;
    window.xnautSidebarShowSubmenu = (title, element) => {
      if (!element) return false;
      collapsedBeforeSubmenu = root.dataset.collapsed === '1';
      root.dataset.collapsed = '0';
      host.style.width = `${Number(localStorage.getItem('xnaut-sidebar-expanded-width')) || 240}px`;
      host.style.minWidth = '200px';
      host.style.maxWidth = '320px';
      mainSections.forEach((section) => { section.hidden = true; });
      submenu.querySelector('.sbar-submenu-head span').textContent = title || 'Menu';
      const body = submenu.querySelector('.sbar-submenu-body');
      body.replaceChildren(element);
      submenu.hidden = false;
      return true;
    };
    window.xnautSidebarShowMain = closeSubmenu;

    host.appendChild(root);
    document.addEventListener('mousedown', onDocMouseDown);

    // Derived on its own so the 3-second status poll can update a row in place
    // instead of rebuilding it. It used to call renderProjects(), which wiped
    // and re-created every row three times a minute — that is what made the
    // project list flash, and it restarted the snake animation each time.
    function ago(ms) {
      if (!ms) return '';
      const mins = Math.max(0, Math.round((Date.now() - ms) / 60000));
      if (mins < 60) return mins <= 1 ? 'just now' : mins + 'm ago';
      const hrs = Math.round(mins / 60);
      if (hrs < 48) return hrs + 'h ago';
      return Math.round(hrs / 24) + 'd ago';
    }

    function dotStateFor(task) {
      // Dot lights when the project has open tabs in this session.
      const sessions = sessionsFor(task);
      const running = sessions.filter((s) => !s.exited);
      const exited = sessions.filter((s) => s.exited);
      // The dot used to mean "has tabs open in this window", which you already
      // know. A running session is the thing worth seeing at a glance.
      const live = running.length > 0
        || !!(window.xnautProjectHasTabs && window.xnautProjectHasTabs(task.id));
      // Motion has to mean something. The snake runs only when an agent hook
      // actually reports Working; a session that merely exists gets a steady
      // dot. Zellij cannot tell us the difference — its resurrection cache is
      // rewritten about once a second whether the agent is thinking or idle —
      // so absent a real status we say "alive", not "busy".
      const agentState = window.xnautProjectAgentStatus
        ? window.xnautProjectAgentStatus(task.id)
        : null;
      let dotClass = '';
      let rowState = '';
      if (agentState === 'permission' || agentState === 'blocked') { dotClass = ' sbar-attention'; rowState = 'attention'; }
      else if (agentState === 'working') { dotClass = ' sbar-run'; rowState = 'working'; }
      else if (agentState === 'waiting') { dotClass = ' sbar-live'; rowState = 'waiting'; }
      else if (agentState === 'done') { dotClass = ' sbar-live'; rowState = 'done'; }
      else if (running.length) { dotClass = ' sbar-live'; rowState = 'live'; }
      else if (exited.length) { dotClass = ' sbar-exited'; rowState = 'exited'; }
      else if (live) dotClass = ' sbar-on';
      const title = running.length ? 'session running' : (exited.length ? 'session can be resurrected' : '');
      return { dotClass, rowState, title, sessions };
    }

    // ---------- the worktree layer ----------
    const normPath = (p) => String(p || '').replace(/\/+$/, '');
    const baseName = (p) => normPath(p).split('/').pop() || String(p || '');

    // The zellij session name the backend derives from a directory. Mirrored
    // from zellij.rs::session_name the way observatory-panel.js:334 mirrors it;
    // a run's session is named after its worktree, so this is how an agent row
    // and a worktree row find each other.
    function zellijNameFor(path) {
      return ('cl-' + baseName(path)).toLowerCase().replace(/[^a-z0-9]+/g, '-')
        .replace(/^-+|-+$/g, '').slice(0, 24).replace(/-+$/, '');
    }

    // How many agents are running ON THIS WORKTREE.
    //
    // Two sources, because neither alone sees every run. A loom run records the
    // directory it executes in (RunRecord.cwd), which is an exact match. An
    // agent session records no path at all, only the zellij session hosting it,
    // so it is matched through the name that session gets from the directory.
    function runningFor(path) {
      const p = normPath(path);
      if (!p) return 0;
      let n = (state.runs || []).filter((r) => normPath(r && r.cwd) === p).length;
      const sess = zellijNameFor(p);
      n += (state.agentSessions || [])
        .filter((s) => s && s.status !== 'done' && s.zellij_session === sess).length;
      return n;
    }

    // Three states and no fourth. `changes` comes back inside git_worktree_list
    // already (gitops.rs counts `git status --porcelain` per worktree), so the
    // dirty signal costs no extra call: fourteen worktrees would otherwise mean
    // fourteen more round trips to learn what the first answer already said.
    function worktreeState(wt) {
      const agents = runningFor(wt.path);
      if (agents > 0) {
        return { key: 'running', dot: 'sbar-run', word: agents === 1 ? '1 agent' : `${agents} agents` };
      }
      if (Number(wt.changes) > 0) {
        return { key: 'dirty', dot: 'sbar-dirty', word: 'dirty', title: `${wt.changes} uncommitted files` };
      }
      return { key: 'clean', dot: 'sbar-clean', word: 'clean' };
    }

    // Running first, then dirty, then clean, each block keeping git's own order
    // (which puts the main worktree first). Without this the cap is arbitrary:
    // the one worktree with an agent on it could be the one that gets hidden.
    const WT_RANK = { running: 0, dirty: 1, clean: 2 };
    function orderWorktrees(wts) {
      return wts
        .map((wt, i) => ({ wt, i, rank: WT_RANK[worktreeState(wt).key] })).sort((a, b) => a.rank - b.rank || a.i - b.i)
        .map((x) => x.wt);
    }

    async function loadWorktrees(entry) {
      if (!entry.repo) return [];
      if (state.worktrees.has(entry.repo)) return state.worktrees.get(entry.repo);
      let wts = [];
      try { wts = await invoke('git_worktree_list', { repo: entry.repo }); }
      catch (_) { wts = []; /* not a git repo, or git failed; say "no worktrees" */ }
      const out = Array.isArray(wts) ? wts.filter((w) => w && w.path) : [];
      state.worktrees.set(entry.repo, out);
      return out;
    }

    // The selected pair. Read as a VALUE (window.xnautActiveScope) or through
    // the getter; both are assigned here so a caller cannot pick the wrong one
    // and get a truthy function where it wanted an object.
    function setScope(scope) {
      state.scope = scope;
      window.xnautActiveScope = scope;
      try { localStorage.setItem(SCOPE_KEY, JSON.stringify(scope)); } catch (_) { /* quota; ignore */ }
      window.dispatchEvent(new CustomEvent('xnaut-scope-changed', { detail: scope }));
    }

    // Start something new: a ticket, and the worktree to do it in.
    //
    // Andre, 2026-09-12: "I want to start something new, a feature, a project,
    // that should always ask .worktree? if yes then it creates one for me."
    //
    // Dispatch already makes a worktree when it sends an agent
    // (dispatch.rs:241), so this is the same act done a step earlier, for work
    // he is going to pick up himself. The branch follows the type rather than
    // the agent convention (`agent/<handle>/<id>`), because nobody is
    // dispatched yet and `feat/xnaut-354` is the name he already uses.
    const BRANCH_PREFIX = { feature: 'feat', bug: 'fix', task: 'chore', chore: 'chore' };

    function newWorkDialog(entry) {
      const key = entry && (entry.projectKey || entry.name);
      if (!key) return;
      const host = document.createElement('div');
      host.className = 'sbar-modal';
      host.innerHTML = `
        <div class="sbar-sheet" role="dialog" aria-modal="true" aria-label="Start something new">
          <h2>Start something new</h2>
          <p class="sbar-sheet-sub">in <b>${escapeText(entry.name)}</b></p>
          <label class="sbar-lbl">What is it<input class="sbar-in" data-title
            placeholder="Describe the outcome, not the task" autocomplete="off"></label>
          <label class="sbar-lbl">Kind<select class="sbar-in" data-kind>
            <option value="feature">feature</option>
            <option value="bug">bug</option>
            <option value="task">task</option>
          </select></label>
          <label class="sbar-lbl">Detail, optional<textarea class="sbar-in sbar-ta" data-body
            placeholder="What done means. What must not break."></textarea></label>
          <label class="sbar-check"><input type="checkbox" data-wt checked>
            Create a worktree for it</label>
          <div class="sbar-branch-row" data-branch-row>
            <label class="sbar-lbl">Branch<input class="sbar-in" data-branch autocomplete="off"></label>
            <p class="sbar-hint" data-hint>The ticket id is added once it exists.</p>
          </div>
          <div class="sbar-sheet-actions">
            <button class="sbar-btn" data-cancel>Cancel</button>
            <button class="sbar-btn sbar-btn-primary" data-go>Create</button>
          </div>
          <p class="sbar-err" data-err hidden></p>
        </div>`;
      document.body.appendChild(host);

      const q = (sel) => host.querySelector(sel);
      const titleEl = q('[data-title]');
      const kindEl = q('[data-kind]');
      const wtEl = q('[data-wt]');
      const branchEl = q('[data-branch]');
      const errEl = q('[data-err]');
      const close = () => host.remove();

      // The branch follows the kind until the owner types over it; after that
      // it is theirs and the kind stops rewriting it.
      let branchTouched = false;
      const suggest = () => {
        if (branchTouched) return;
        const slug = titleEl.value.trim().toLowerCase()
          .replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 32).replace(/-+$/, '');
        branchEl.value = `${BRANCH_PREFIX[kindEl.value] || 'chore'}/${slug || 'new'}`;
      };
      titleEl.addEventListener('input', suggest);
      kindEl.addEventListener('change', suggest);
      branchEl.addEventListener('input', () => { branchTouched = true; });
      const syncWt = () => { q('[data-branch-row]').hidden = !wtEl.checked; };
      wtEl.addEventListener('change', syncWt);
      suggest();
      syncWt();

      q('[data-cancel]').onclick = close;
      host.addEventListener('mousedown', (e) => { if (e.target === host) close(); });
      host.addEventListener('keydown', (e) => { if (e.key === 'Escape') close(); });

      q('[data-go]').onclick = async () => {
        const title = titleEl.value.trim();
        if (!title) { titleEl.focus(); return; }
        const go = q('[data-go]');
        go.disabled = true;
        go.textContent = 'Creating...';
        errEl.hidden = true;
        try {
          const ticket = await invoke('pm_ticket_create', {
            request: {
              project: key,
              title,
              ticket_type: kindEl.value,
              status: 'ready',
              priority: 'medium',
              body: q('[data-body]').value.trim(),
            },
          });
          let worktree = '';
          if (wtEl.checked && entry.repo) {
            // The id belongs in the branch: a branch named only after a slug
            // cannot be traced back to the work it is for.
            const base = branchEl.value.trim().replace(/\/+$/, '') || 'chore/new';
            const branch = base.includes(ticket.id.toLowerCase())
              ? base
              : `${base}-${ticket.id.toLowerCase()}`;
            const path = await invoke('worktree_suggest_path', { repoPath: entry.repo, branch });
            const add = (checkout_existing) => invoke('worktree_add', {
              repoPath: entry.repo,
              worktreePath: path,
              opts: { branch, base: null, checkout_existing, no_auto_setup_remote: false },
            });
            // A branch can outlive the worktree it was made for: a reclaimed
            // worktree, or a second go at the same name. Check it out rather
            // than failing on "already exists", which is what dispatch.rs does
            // and what build-sandbox.js's runSlice already does here.
            try {
              await add(false);
            } catch (_first) {
              await add(true);
            }
            worktree = path;
          }
          close();
          if (typeof window.xnautToast === 'function') {
            window.xnautToast(worktree ? `${ticket.id} created, with a worktree` : `${ticket.id} created`);
          }
          refresh();
          if (typeof window.xnautOpenWorkspace === 'function') {
            window.xnautOpenWorkspace({ project: key, worktree, tab: worktree ? 'code' : 'work' });
          }
        } catch (error) {
          // The ticket may exist while the worktree failed. Say which, rather
          // than leaving the owner to guess whether to try again.
          errEl.textContent = String((error && error.message) || error);
          errEl.hidden = false;
          go.disabled = false;
          go.textContent = 'Create';
        }
      };
      titleEl.focus();
    }

    // The one way into the workspace from this tree. `xnautOpenWorkspace` is
    // assigned in workspace.js:793; grepped, because an undefined global here
    // would be a silent no-op and the row would look dead, which is exactly
    // the bug this function exists to fix.
    function openWorkspaceFor(entry, wt) {
      if (typeof window.xnautOpenWorkspace !== 'function') {
        console.error('[sidebar] xnautOpenWorkspace is not loaded, so a project cannot open');
        return;
      }
      const key = entry.projectKey || entry.name;
      window.xnautOpenWorkspace({
        project: key,
        worktree: wt ? normPath(wt.path) : '',
        tab: 'code',
      });
      // The selection is re-asserted AFTER the tab exists, never before.
      // `xnautOpenWorkspace` opens with `xnautHomeContext()` (app.js:3903),
      // which calls `xnautSidebarSetActiveProject(null)` and clears the row
      // that was just lit. That is right for a surface which is not about a
      // project and wrong for this one, which is about nothing else.
      if (wt) selectWorktree(entry, wt);
    }

    function selectWorktree(entry, wt) {
      setScope({
        project: entry.projectKey || null,
        projectId: entry.id,
        projectName: entry.name,
        repo: entry.repo,
        worktree: normPath(wt.path),
        branch: wt.branch || baseName(wt.path),
      });
      // Selecting a worktree selects the project too: the group row lights up
      // with the leaf, so the tree never shows a chosen branch under no project.
      list.querySelectorAll('.sbar-row').forEach((r) => {
        r.classList.toggle('sbar-row-active',
          (r.dataset.wt && r.dataset.wt === normPath(wt.path))
          || (!r.dataset.wt && r.dataset.entryId === entry.id));
      });
      if (typeof window.xnautRightPaneSetRoot === 'function') window.xnautRightPaneSetRoot(wt.path);
    }

    function buildWorktreeRow(entry, wt, opts) {
      const st = worktreeState(wt);
      const label = wt.branch || baseName(wt.path);
      const shownName = (opts && opts.withProject) ? `${entry.name} / ${label}` : label;
      const pins = loadWtPins();
      const pinId = wtPinId(entry.key, normPath(wt.path));
      const pinned = pins.includes(pinId);
      const row = document.createElement('div');
      row.className = 'sbar-row sbar-wt';
      row.dataset.wt = normPath(wt.path);
      row.dataset.wtState = st.key;
      if (state.scope && state.scope.worktree === normPath(wt.path)) row.classList.add('sbar-row-active');
      row.innerHTML = `
        <span class="sbar-twist-spacer"></span>
        <span class="sbar-dot ${st.dot}" title="${escapeText(st.title || st.word)}"></span>
        <span class="sbar-name" title="${escapeText(wt.path)}">${escapeText(shownName)}</span>
        <span class="sbar-word">${escapeText(st.word)}</span>
        <button class="sbar-star" data-star aria-pressed="${pinned ? 'true' : 'false'}"
          title="${pinned ? 'Unpin' : 'Pin to the top'}"
          aria-label="${escapeText(`${pinned ? 'Unpin' : 'Pin'} ${label} in ${entry.name}`)}">${ICONS.star}</button>
      `;
      row.querySelector('[data-star]').addEventListener('click', (event) => {
        event.stopPropagation();
        const now = loadWtPins();
        saveWtPins(now.includes(pinId) ? now.filter((p) => p !== pinId) : now.concat([pinId]));
        renderProjects();
      });
      row.addEventListener('click', () => openWorkspaceFor(entry, wt));
      row.addEventListener('contextmenu', (event) => {
        event.preventDefault();
        event.stopPropagation();
        openMenu(event.clientX, event.clientY, [
          {
            label: pinned ? 'Unpin' : 'Pin',
            action: () => {
              const now = loadWtPins();
              saveWtPins(now.includes(pinId) ? now.filter((p) => p !== pinId) : now.concat([pinId]));
              renderProjects();
            },
          },
          {
            // A worktree is its own workspace, so it gets its own id rather
            // than borrowing the project's; sharing one would make the project
            // and the worktree fight over the same tab bucket.
            label: 'Open terminal here',
            action: () => navigate('open-task', { id: `${entry.id}:${normPath(wt.path)}`, name: shownName, path: wt.path }),
          },
        ]);
      });
      return row;
    }

    function renderChildren(entry, box) {
      const all = state.worktrees.get(entry.repo);
      box.innerHTML = '';
      if (!all) {
        const wait = document.createElement('div');
        wait.className = 'sbar-wt-more';
        wait.textContent = 'Reading worktrees…';
        box.appendChild(wait);
        return;
      }
      if (!all.length) {
        const none = document.createElement('div');
        none.className = 'sbar-wt-more';
        none.textContent = 'No worktrees';
        box.appendChild(none);
        return;
      }
      const ordered = orderWorktrees(all);
      const showAll = state.showAllWt.has(entry.key);
      const shown = showAll ? ordered : ordered.slice(0, WORKTREE_CAP);
      for (const wt of shown) box.appendChild(buildWorktreeRow(entry, wt));
      const hidden = ordered.length - shown.length;
      // Orca's hidden-worktree row. This repo has fourteen; without it a single
      // project fills the sidebar and every other project is below the fold.
      if (hidden > 0 || showAll) {
        const more = document.createElement('div');
        more.className = 'sbar-wt-more';
        more.dataset.wtMore = entry.key;
        const text = hidden > 0
          ? `Hiding ${hidden} worktree${hidden === 1 ? '' : 's'}`
          : 'Show fewer worktrees';
        more.innerHTML = `<span>${escapeText(text)}</span>`
          + `<button class="sbar-star" data-wt-more-btn aria-pressed="${showAll ? 'true' : 'false'}"
              title="${hidden > 0 ? 'Show them all' : 'Show fewer'}"
              aria-label="${escapeText((hidden > 0 ? 'Show all worktrees in ' : 'Show fewer worktrees in ') + entry.name)}">${ICONS.close}</button>`;
        const toggle = () => {
          if (state.showAllWt.has(entry.key)) state.showAllWt.delete(entry.key);
          else state.showAllWt.add(entry.key);
          renderChildren(entry, box);
        };
        more.addEventListener('click', toggle);
        box.appendChild(more);
      }
    }

    function buildRow(entry) {
      const task = entry.task;
      const row = document.createElement('div');
      row.className = 'sbar-row';
      row.dataset.entryId = entry.id;
      if (task) row.dataset.taskId = task.id;
      const { dotClass, rowState, title, sessions } = task
        ? dotStateFor(task)
        : { dotClass: '', rowState: '', title: '', sessions: [] };
      if (rowState) row.dataset.state = rowState;
      const badge = task && task.kind === 'task' ? 'task' : ((task && task.project_type) || '');
      const agents = sessions
        .map((s) => (/^([a-z]{2,4})-/.exec(String(s.name || '')) || [])[1])
        .filter(Boolean);
      const open = entry.repo ? state.openGroups.has(entry.key) : false;
      const isPinned = loadPins().includes(entry.id);
      row.innerHTML = `
        <span class="sbar-dot${dotClass}" title="${title}"></span>
        <span class="sbar-kind" aria-hidden="true">${ICONS.folder}</span>
        <div class="sbar-row-main">
          <div class="sbar-row-top">
            <span class="sbar-name" title="${escapeText(entry.repo || '')}">${escapeText(entry.name)}</span>
            ${agents.map((a) => `<span class="sbar-chip sbar-sess" title="${escapeText(a)} session — click to attach">${escapeText(a)}</span>`).join('')}
            ${badge ? `<span class="sbar-chip">${escapeText(badge)}</span>` : ''}
          </div>
          <div class="sbar-branch" hidden><span class="sbar-branch-name"></span><span class="sbar-ago"></span></div>
        </div>
        <button class="sbar-star" data-star aria-pressed="${isPinned ? 'true' : 'false'}"
          title="${isPinned ? 'Unpin' : 'Pin to the top'}"
          aria-label="${escapeText(`${isPinned ? 'Unpin' : 'Pin'} ${entry.name}`)}">${ICONS.star}</button>
        ${entry.projectKey
          ? `<button class="sbar-more-proj" data-proj-menu
              aria-label="${escapeText(entry.name)} actions" title="${escapeText(entry.name)} actions">···</button>`
          : ''}
      `;
      row.querySelector('[data-star]').addEventListener('click', (event) => {
        event.stopPropagation();
        const now = loadPins();
        savePins(now.includes(entry.id) ? now.filter((p) => p !== entry.id) : now.concat([entry.id]));
        renderProjects();
      });
      // The project's own menu. Orca puts the things you configure once behind
      // a three-dot beside the name rather than in the row, and the workspace
      // already hosts every one of them as a sheet (XNAUT-342), so this opens
      // the workspace straight onto the one that was picked.
      // Only a row the PM board knows gets one: `entry.key` is this tree's own
      // id (`pm:XNAUT`, or a task id), and `entry.projectKey` is the project.
      // A local task with no project behind it has nothing for these sheets to
      // read, so it gets no menu rather than a menu that opens empty.
      const projMenu = row.querySelector('[data-proj-menu]');
      if (projMenu) {
        projMenu.addEventListener('click', (event) => {
          event.stopPropagation();
          const key = entry.projectKey;
          const open = (opts) => () => {
            if (typeof window.xnautOpenWorkspace !== 'function') return;
            window.xnautOpenWorkspace({ project: key, ...opts });
          };
          const pins = loadPins();
          const pinned = pins.includes(entry.id);
          openMenu(event.clientX, event.clientY, [
            { label: 'Start something new...', action: () => newWorkDialog(entry) },
            {
              label: pinned ? 'Unpin' : 'Pin to the top',
              action: () => {
                savePins(pinned ? pins.filter((p) => p !== entry.id) : pins.concat([entry.id]));
                renderProjects();
              },
            },
            { label: 'Open workspace', action: open({ tab: 'code' }) },
            { label: 'Delivery', action: open({ tab: 'delivery' }) },
            { label: 'Work', action: open({ tab: 'work' }) },
            { label: 'Settings', action: open({ sheet: 'settings' }) },
            { label: 'Designer', action: open({ sheet: 'designer' }) },
            { label: 'Artifacts', action: open({ sheet: 'artifacts' }) },
            { label: 'Project details', action: open({ sheet: 'details' }) },
          ]);
        });
      }
      row.querySelectorAll('.sbar-sess').forEach((chip, i) => {
        chip.addEventListener('click', (event) => {
          event.stopPropagation();          // the row opens the workspace
          const named = sessions[i];
          if (task && named) navigate('open-task', { ...task, zellij_session: named.name });
        });
      });
      // No chevron (Andre, 2026-09-14: "do we need the chevron on the far
      // left? If I click the name it could open close?"). The name does
      // both: a click opens the workspace and shows the worktrees; a second
      // click on the project that is already active and open folds them.
      row.addEventListener('click', (e) => {
        if (!task) {
          // A project the local registry does not know: there is no session to
          // open and no tab bucket to enter, so the click expands its tree.
          if (entry.repo) setGroupOpen(entry, !state.openGroups.has(entry.key));
          return undefined;
        }
        // Clicking a project opens its WORKSPACE: the code, and the surfaces
        // as tabs beside it (XNAUT-336). Until this line the click opened a
        // terminal instead and the workspace could only be reached from the
        // three-dot menu, so the whole spine was built and unreachable.
        //
        // The running session did not lose its way in: it is on the session
        // chips in this row, and in the context menu. A terminal is one thing
        // a project has, not the thing a project IS.
        if (entry.repo) {
          const isOpen = state.openGroups.has(entry.key);
          if (isOpen && row.classList.contains('sbar-row-active')) { setGroupOpen(entry, false); return undefined; }
          if (!isOpen) setGroupOpen(entry, true);
        }
        openWorkspaceFor(entry);
        return undefined;
      });
      row.addEventListener('contextmenu', (e) => {
        e.preventDefault();
        e.stopPropagation();
        const pins = loadPins();
        const pinned = pins.includes(entry.id);
        const items = [{
          label: pinned ? 'Unpin' : 'Pin',
          action: () => {
            savePins(pinned ? pins.filter((p) => p !== entry.id) : pins.concat([entry.id]));
            renderProjects();
          },
        }];
        items.push({
          label: window.xnautHiddenProjects.isHidden('sidebar', entry.id) ? 'Unhide' : 'Hide',
          action: () => {
            window.xnautHiddenProjects.toggle('sidebar', entry.id);
            renderProjects();
          },
        });
        if (task && task.kind === 'task') {
          items.push({ label: 'Promote to Project', action: () => navigate('promote-task', task) });
        }
        if (task) {
          items.push({
          label: 'Remove from list',
          danger: true,
          action: async () => {
            // Sits directly under "Hide", so a mis-click drops the project.
            // task_remove only rewrites the registry entry, but a menu item
            // labelled "Remove" cannot say that for itself; the dialog does.
            const ok = await window.xnautConfirmDialog(
              `Remove \u201c${escapeText(task.name || task.id)}\u201d from the list?`,
              'Remove',
              'Removes it from the list. The folder, its tickets and any running session stay. '
              + 'You can add it back with \u201cOpen as project\u201d.',
            );
            if (!ok) return;
            invoke('task_remove', { id: task.id })
              .then(() => refresh())
              .catch((err) => console.error('[sidebar] task_remove failed:', err));
          },
          });
        }
        openMenu(e.clientX, e.clientY, items);
      });
      // Branch — best effort, fills in async.
      if (task && task.path) {
        invoke('get_git_info', { path: task.path }).then((info) => {
          const branch = info && (info.branch || info.current_branch || null);
          if (!branch || !row.isConnected) return;
          const el = row.querySelector('.sbar-branch');
          el.querySelector('.sbar-branch-name').textContent = String(branch);
          el.hidden = false;
        }).catch(() => { /* not a git repo / command failed — show nothing */ });
      }
      return row;
    }

    // Expand or collapse one project group, remembering the answer. The fetch
    // is lazy on purpose: forty-four projects times one `git worktree list`
    // each is forty-four git invocations to paint a tree nobody has opened.
    function setGroupOpen(entry, open) {
      if (open) state.openGroups.add(entry.key); else state.openGroups.delete(entry.key);
      try { localStorage.setItem(`xnaut-sbar-wt-open:${entry.key}`, open ? '1' : '0'); } catch (_) {}
      const group = list.querySelector(`.sbar-group[data-group="${cssAttr(entry.key)}"]`);
      if (!group) return;
      const twist = group.querySelector('[data-twist]');
      if (twist) {
        twist.textContent = open ? '▾' : '▸';
        twist.setAttribute('aria-expanded', open ? 'true' : 'false');
        twist.setAttribute('aria-label', (open ? 'Collapse ' : 'Expand ') + entry.name);
      }
      const box = group.querySelector('.sbar-children');
      box.hidden = !open;
      if (!open) return;
      renderChildren(entry, box);
      if (state.worktrees.has(entry.repo)) return;
      loadWorktrees(entry).then(() => {
        if (state.destroyed || !box.isConnected || box.hidden) return;
        renderChildren(entry, box);
        // A pinned worktree only becomes hoistable once its repo has been read,
        // so the Pinned group is rebuilt after a load. Asking whether ANY pin
        // exists rather than parsing this entry's out of the id: the id is a
        // storage key, and a render is cheap enough not to earn a parser.
        if (loadWtPins().length) renderProjects();
      });
    }

    // Two sources, one list. The PM control repo knows the projects; the local
    // registry knows which of them this machine has open, with its sessions and
    // its context menu. Matched on path, so a project that both know is one row
    // carrying both, and a project only one of them knows is still a row.
    function buildEntries() {
      const tasks = state.tasks || [];
      const projects = state.projects || [];
      const byPath = new Map();
      for (const p of projects) {
        if (p && p.source_path) byPath.set(normPath(p.source_path), p);
      }
      const claimed = new Set();
      const entries = [];
      for (const t of tasks) {
        const project = t.path ? byPath.get(normPath(t.path)) : undefined;
        if (project) claimed.add(project.key);
        entries.push({
          id: String(t.id),
          key: String(t.id),
          task: t,
          projectKey: project ? project.key : null,
          // The registry's name wins: it is the one the owner typed.
          name: t.name || (project && project.name) || String(t.id),
          repo: normPath(t.path || (project && project.source_path) || ''),
        });
      }
      for (const p of projects) {
        if (!p || claimed.has(p.key)) continue;
        entries.push({
          id: `pm:${p.key}`,
          key: `pm:${p.key}`,
          task: null,
          projectKey: p.key,
          name: p.name || p.key,
          repo: normPath(p.source_path || ''),
        });
      }
      // A group that was open last time is open again. Seeded here rather than
      // at mount because the keys are not known until the two lists are in.
      for (const entry of entries) {
        try {
          if (localStorage.getItem(`xnaut-sbar-wt-open:${entry.key}`) === '1') state.openGroups.add(entry.key);
        } catch (_) { /* unreadable; the group starts collapsed */ }
      }
      return entries;
    }

    function renderProjects() {
      list.innerHTML = '';
      const entries = buildEntries();
      const pins = loadPins();
      // Hidden projects stay in the registry and on disk — they are only kept
      // out of the list, so a demo does not show client work. state.showHidden
      // reveals them temporarily so they can be unhidden again.
      const hiddenIds = window.xnautHiddenProjects.list('sidebar');
      const visible = state.showHidden
        ? entries
        : entries.filter((e) => !hiddenIds.includes(e.id));
      const hiddenCount = entries.length - visible.length;
      if (!entries.length) {
        const empty = document.createElement('div');
        empty.className = 'sbar-empty';
        empty.textContent = 'No projects yet';
        list.appendChild(empty);
        return;
      }

      // The Pinned group: pinned projects as whole groups, worktrees and all,
      // moved out of the list rather than copied (a group's key must stay
      // unique, setGroupOpen finds it by key), and pinned project/worktree
      // pairs hoisted out of unpinned groups so the branch you live in is at
      // the top.
      const pinnedEntries = visible.filter((e) => pins.includes(e.id));
      const rest = visible.filter((e) => !pins.includes(e.id));
      const wtPins = loadWtPins();
      const pinnedPairs = [];
      for (const entry of rest) {
        for (const wt of state.worktrees.get(entry.repo) || []) {
          if (wtPins.includes(wtPinId(entry.key, normPath(wt.path)))) pinnedPairs.push({ entry, wt });
        }
      }
      if (pinnedEntries.length || pinnedPairs.length) {
        const lbl = document.createElement('button');
        lbl.className = 'sbar-pinned-head';
        lbl.type = 'button';
        const pinnedCollapsed = localStorage.getItem(PINNED_COLLAPSE_KEY) === '1';
        lbl.setAttribute('aria-expanded', pinnedCollapsed ? 'false' : 'true');
        lbl.innerHTML = `<span class="sbar-kind">${ICONS.pin}</span><span>Pinned</span>`;
        list.appendChild(lbl);
        const box = document.createElement('div');
        box.className = 'sbar-pinned';
        box.hidden = pinnedCollapsed;
        lbl.addEventListener('click', () => {
          box.hidden = !box.hidden;
          lbl.setAttribute('aria-expanded', box.hidden ? 'false' : 'true');
          try { localStorage.setItem(PINNED_COLLAPSE_KEY, box.hidden ? '1' : '0'); } catch (_) { /* quota; ignore */ }
        });
        for (const entry of pinnedEntries) {
          // "Pinned incl. subs": a pinned project shows its worktrees unless
          // it was folded on purpose (its own key, so the choice sticks).
          if (entry.repo && localStorage.getItem(`xnaut-sbar-wt-open:${entry.key}`) === null) state.openGroups.add(entry.key);
          box.appendChild(buildGroup(entry));
        }
        for (const pair of pinnedPairs) {
          box.appendChild(buildWorktreeRow(pair.entry, pair.wt, { withProject: true }));
        }
        list.appendChild(box);
      }

      if (state.onlyPinned) {
        // Hide all except pinned. The way back sits where the hidden rows
        // would have been, same as the hidden-projects toggle below.
        const toggle = document.createElement('div');
        toggle.className = 'sbar-hidden-toggle';
        toggle.dataset.onlyPinned = '1';
        toggle.textContent = rest.length ? `${rest.length} more — show all` : 'Nothing pinned yet — show all';
        toggle.addEventListener('click', () => setOnlyPinned(false));
        list.appendChild(toggle);
      } else {
        for (const entry of rest) list.appendChild(buildGroup(entry));
      }

      // The only way back: without this, hiding is a one-way door.
      if (hiddenCount > 0 || state.showHidden) {
        const toggle = document.createElement('div');
        toggle.className = 'sbar-hidden-toggle';
        toggle.textContent = state.showHidden
          ? 'Hide hidden again'
          : `${hiddenCount} hidden — show`;
        toggle.title = 'Right-click a revealed project and choose Hide to unhide it';
        toggle.addEventListener('click', () => {
          state.showHidden = !state.showHidden;
          renderProjects();
        });
        list.appendChild(toggle);
      }
      // Re-apply the active-project highlight after rebuilding rows.
      if (window.xnautSidebarSetActiveProject) window.xnautSidebarSetActiveProject(state.activeProjectId || null);
      // Async: one git call per project, so it must not hold up the render.
      fillActivity(visible.filter((e) => e.task).map((e) => e.task));
    }

    function buildGroup(entry) {
      const group = document.createElement('div');
      group.className = 'sbar-group';
      group.dataset.group = entry.key;
      group.appendChild(buildRow(entry));
      const box = document.createElement('div');
      box.className = 'sbar-children';
      box.hidden = true;
      group.appendChild(box);
      if (entry.repo && state.openGroups.has(entry.key)) {
        // Re-open on the next frame: setGroupOpen looks the group up in the
        // list, and it is not in the DOM until renderProjects appends it.
        Promise.resolve().then(() => { if (!state.destroyed && group.isConnected) setGroupOpen(entry, true); });
      }
      return group;
    }

    // Last activity, for every row in ONE call. The obvious source — zellij's
    // last_active_ms — is useless here: it comes from the mtime of the
    // resurrection cache, which zellij rewrites about once a second whether the
    // agent is thinking or asleep, so every project would read "just now".
    // The last COMMIT is a thing that actually happened.
    async function fillActivity(tasks) {
      const rows = tasks.filter((t) => t.path);
      if (!rows.length) return;
      let times = [];
      try { times = await invoke('projects_activity', { paths: rows.map((t) => t.path) }) || []; }
      catch (_) { return; }
      rows.forEach((task, i) => {
        const row = list.querySelector(`.sbar-row[data-task-id="${String(task.id).replace(/"/g, '\\"')}"]`);
        if (!row) return;
        const text = ago(times[i]);
        if (!text) return;
        const line = row.querySelector('.sbar-branch');
        const el = row.querySelector('.sbar-ago');
        if (!line || !el) return;
        el.textContent = text;
        el.title = 'last commit';
        line.hidden = false; // may be the only thing on the line, if there is no branch
      });
    }

    // app.js polls agent_sessions_list and calls this when a status changes.
    // Without it the dots only updated on a full refresh, so an agent could go
    // from working to needing you and the rail would not move.
    // In place, and only where something actually changed: re-assigning an
    // unchanged className restarts the CSS animation, so the snake would stutter
    // back to its first step on every poll.
    window.xnautSidebarRefreshDots = () => {
      if (!state.tasks) return;
      for (const task of state.tasks) {
        const row = list.querySelector(`.sbar-row[data-task-id="${String(task.id).replace(/"/g, '\\"')}"]`);
        if (!row) continue;
        const { dotClass, rowState, title } = dotStateFor(task);
        const dot = row.querySelector('.sbar-dot');
        if (dot) {
          const next = 'sbar-dot' + dotClass;
          if (dot.className !== next) dot.className = next;
          if (dot.title !== title) dot.title = title;
        }
        if ((row.dataset.state || '') !== rowState) {
          if (rowState) row.dataset.state = rowState;
          else delete row.dataset.state;
        }
      }
    };

    const listOr = async (cmd, args) => {
      try {
        const v = await invoke(cmd, args);
        return Array.isArray(v) ? v : [];
      } catch (_) { return []; }
    };

    async function refresh() {
      if (state.destroyed) return;
      // Live Zellij sessions, so a project row can open the session that is
      // already running instead of a fresh shell in the same folder.
      state.sessions = await listOr('zellij_sessions_info');
      // The two run registries behind the worktree dot. Both are read here so
      // the tree paints from one snapshot rather than a call per worktree.
      const runs = await listOr('loom_runs_list', { limit: 200 });
      state.runs = runs.filter((r) => r && r.status === 'started');
      state.agentSessions = await listOr('agent_sessions_list');
      // The PM control repo's projects. It is not configured on every machine,
      // so an empty answer is a normal one and the registry carries the list.
      state.projects = await listOr('pm_project_list');
      try {
        const tasks = await invoke('tasks_list');
        state.tasks = Array.isArray(tasks) ? tasks : [];
      } catch (e) {
        console.error('[sidebar] tasks_list failed:', e);
        state.tasks = [];
      }
      if (state.destroyed) return;
      // Only an OPEN group's worktrees are re-read. Dropping the whole cache
      // would blank every collapsed group's pinned rows and make each refresh
      // flash "Reading worktrees…" in the ones that are open.
      for (const entry of buildEntries()) {
        if (state.openGroups.has(entry.key)) state.worktrees.delete(entry.repo);
      }
      renderSessions();
      renderProjects();
    }

    // Sessions are named <agent>-<project> by the shell wrappers: cl-Bucky and
    // cx-Bucky both belong to Bucky. Zellij truncates long names (cl-nautflow-
    // incident-loo), so the project side is matched as a prefix.
    function sessionsFor(task) {
      const name = String(task.name || task.id || '');
      if (!name) return [];
      return (state.sessions || []).filter((s) => {
        // zellij keeps EXITED sessions listed as "attach to resurrect", and
        // zellij_sessions_info reports them with exited: true. A row that
        // offers to open one is offering a session that is not running, which
        // is exactly what this list is for. Killing NautGate in zellij left it
        // in the sidebar until this filter existed (2026-08-18).
        if (s.exited) return false;
        const m = /^([a-z]{2,4})-(.+)$/.exec(String(s.name || ''));
        if (!m) return false;
        const proj = m[2];
        return name === proj || name.startsWith(proj) || proj.startsWith(name);
      });
    }

    // Reads the same `max_usage` command as the footer, the Observatory and the
    // agent pane. It used to read ~/.flowai/usage.json, a path NOTHING in this
    // repo has ever written, so this strip could only ever say "usage: n/a"
    // (XNAUT-257) — a surface wired at one end only. A failure now shows its
    // reason rather than the same "n/a" an empty file would produce.
    async function loadUsage() {
      const fail = (reason) => {
        usageRows.innerHTML = `<div class="sbar-usage-row sbar-muted" title="${escapeText(reason || 'no usage data')}">`
          + `usage: ${escapeText(reason ? String(reason).slice(0, 40) : 'n/a')}</div>`;
      };
      try {
        const u = await invoke('max_usage', { account: null });
        if (state.destroyed) return;
        if (!u) return fail('no usage returned');
        const rows = [`<div class="sbar-usage-row">Claude ${Math.round(u.five_hour_pct)}% 5h · ${Math.round(u.seven_day_pct)}% wk</div>`];
        // Extra-usage credits: the only real money here, and only when billed.
        if (u.spend && u.spend.used > 0) {
          rows.push(`<div class="sbar-usage-row" title="Extra-usage credits billed beyond your plan limits this period.">`
            + `extra ${u.spend.currency === 'USD' ? '$' : ''}${u.spend.used.toFixed(2)}`
            + `${u.spend.currency === 'USD' ? '' : ' ' + escapeText(u.spend.currency)}</div>`);
        }
        usageRows.innerHTML = rows.join('');
      } catch (e) {
        if (!state.destroyed) fail(e);
      }
    }

    // The Sessions view repaints its rows every few seconds while it is open:
    // busy comes from a ps scan, so nothing pushes it. Two reads, no tree walk.
    const sessionTimer = setInterval(async () => {
      if (state.destroyed || state.view !== 'sessions') return;
      state.sessions = await listOr('zellij_sessions_info');
      state.agentSessions = await listOr('agent_sessions_list');
      if (!state.destroyed) renderSessions();
    }, 5000);

    function destroy() {
      clearInterval(sessionTimer);
      if (state.destroyed) return;
      state.destroyed = true;
      if (state.disposeMeshBadge) state.disposeMeshBadge();
      if (window.xnautSidebarShowSubmenu) delete window.xnautSidebarShowSubmenu;
      if (window.xnautSidebarShowMain) delete window.xnautSidebarShowMain;
      closeMenu();
      document.removeEventListener('mousedown', onDocMouseDown);
      if (root.parentNode) root.parentNode.removeChild(root);
      if (current && current.destroy === destroy) current = null;
    }

    refresh();
    loadUsage();

    const controller = { refresh, destroy };
    current = controller;
    return controller;
  }

  // Public API.
  window.xnautMountSidebar = mountSidebar;
  window.xnautSidebarRefresh = function () {
    if (current) current.refresh();
  };
})();
