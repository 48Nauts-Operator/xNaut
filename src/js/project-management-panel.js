// Optional Git-backed Project Management workspace.
(function () {
  'use strict';

  const invoke = (...args) => window.__TAURI__.core.invoke(...args);
  const panes = new Map();
  let counter = 0;
  const STATUSES = ['inbox', 'ready', 'in_progress', 'review', 'blocked', 'done'];
  const TYPES = ['idea', 'feature', 'bug', 'incident', 'task'];
  const PRIORITIES = ['low', 'medium', 'high', 'critical'];
  const LABELS = { inbox: 'Inbox', ready: 'Ready', in_progress: 'In progress', review: 'Review', blocked: 'Blocked', done: 'Done' };
  const STANDARD_STAGES = [
    ['idea', 'Discover', 'Idea', 'Analyst'],
    ['concept', 'Discover', 'Concept', 'Analyst'],
    ['business_case', 'Discover', 'Business case', 'Analyst'],
    ['prd', 'Define', 'Product requirements', 'PM'],
    ['architecture', 'Define', 'Architecture', 'Architect'],
    ['data_model', 'Define', 'Data model', 'Architect'],
    ['api_design', 'Define', 'API design', 'Architect'],
    ['security_review', 'Define', 'Security review', 'Security'],
    ['development_plan', 'Plan', 'Development plan', 'Planner'],
    ['sprint_stories', 'Plan', 'Sprint stories', 'Planner'],
    ['tickets', 'Plan', 'Executable tickets', 'PM'],
    ['build', 'Deliver', 'Build', 'Builder'],
    ['test_review', 'Deliver', 'Test and review', 'Reviewer'],
    ['release', 'Deliver', 'Release', 'Builder'],
    ['learning', 'Deliver', 'Engram learning', 'Reviewer'],
  ];
  const INCIDENT_STAGES = [
    ['intake', 'Resolve', 'Incident intake', 'Analyst'],
    ['rca', 'Resolve', 'Root-cause analysis', 'Analyst'],
    ['action_plan', 'Resolve', 'Action plan', 'Planner'],
    ['ticket', 'Execute', 'Implementation ticket', 'PM'],
    ['build', 'Execute', 'Build', 'Builder'],
    ['test_review', 'Execute', 'Test and review', 'Reviewer'],
    ['release', 'Close', 'Release', 'Builder'],
    ['learning', 'Close', 'Engram learning', 'Reviewer'],
  ];
  const ICON = {
    refresh: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M13 8a5 5 0 1 1-1.5-3.5"/><path d="M13 2v3h-3"/></svg>',
    sync: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M3 5h8l-2-2M13 11H5l2 2"/></svg>',
    close: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M4 4l8 8M12 4l-8 8"/></svg>',
    doc: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3"><path d="M4 1.5h5l3 3v10H4z"/><path d="M9 1.5v3h3"/></svg>',
    eye: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M1.8 8s2.3-4 6.2-4 6.2 4 6.2 4-2.3 4-6.2 4-6.2-4-6.2-4z"/><circle cx="8" cy="8" r="2"/></svg>',
    pencil: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M3 13l1-3 6.8-6.8a1.4 1.4 0 0 1 2 2L6 12z"/><path d="M9.8 4.2l2 2"/></svg>',
    plus: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M8 3v10M3 8h10"/></svg>',
  };

  function esc(value) {
    return String(value == null ? '' : value).replace(/[&<>"']/g, (ch) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[ch]));
  }

  function relativeTime(iso) {
    const at = Date.parse(iso);
    if (!Number.isFinite(at)) return '';
    const seconds = Math.max(0, Math.floor((Date.now() - at) / 1000));
    for (const [unit, size] of [['year', 31536000], ['month', 2592000], ['day', 86400], ['hour', 3600], ['minute', 60]]) {
      if (seconds >= size) {
        const amount = Math.floor(seconds / size);
        return `${amount} ${unit}${amount === 1 ? '' : 's'} ago`;
      }
    }
    return 'just now';
  }

  function injectStyles() {
    if (document.getElementById('pmw-styles')) return;
    const style = document.createElement('style');
    style.id = 'pmw-styles';
    style.textContent = `
.pmw { position:relative; display:flex; flex-direction:column; width:100%; height:100%; min-width:0; min-height:0; overflow:hidden; background:var(--editor-surface,#1b1d23); color:var(--text-primary,#d8dbe2); font-size:13px; }
.pmw-head { display:flex; align-items:center; gap:8px; min-height:48px; padding:7px 12px; border-bottom:1px solid var(--border-color,#34363d); }
.pmw-title { font-size:14px; font-weight:650; margin-right:4px; }
.pmw-project-select,.pmw-filter,.pmw-input,.pmw-select,.pmw-textarea { background:var(--input-bg,rgba(255,255,255,.05)); border:1px solid var(--border-color,#3a3d45); border-radius:6px; color:inherit; font:inherit; outline:none; }
.pmw-project-select,.pmw-filter,.pmw-input,.pmw-select { min-height:30px; padding:4px 8px; }
.pmw-project-select { width:190px; }
.pmw-filter { flex:1 1 180px; max-width:360px; }
.pmw-input:focus,.pmw-select:focus,.pmw-textarea:focus,.pmw-filter:focus { border-color:var(--accent,#4f8cff); }
.pmw-spacer { flex:1 1 auto; }
.pmw-icon { display:flex; align-items:center; justify-content:center; width:30px; height:30px; padding:0; border:1px solid transparent; border-radius:6px; background:transparent; color:var(--text-secondary,#9a9faa); cursor:pointer; }
.pmw-icon:hover { color:var(--text-primary,#fff); background:var(--hover-bg,rgba(255,255,255,.06)); border-color:var(--border-color,#3a3d45); }
.pmw-icon:disabled { opacity:.45; cursor:default; }
.pmw-icon svg { width:15px; height:15px; }
.pmw-btn { min-height:30px; padding:4px 10px; border:1px solid var(--border-color,#3a3d45); border-radius:6px; background:transparent; color:inherit; font:inherit; cursor:pointer; white-space:nowrap; }
.pmw-btn:hover { border-color:var(--accent,#4f8cff); }
.pmw-btn-primary { background:var(--accent,#4f8cff); border-color:var(--accent,#4f8cff); color:var(--accent-foreground,#fff); }
.pmw-btn-danger { color:#f87171; border-color:rgba(248,113,113,.4); }
.pmw-segment { display:flex; border:1px solid var(--border-color,#3a3d45); border-radius:6px; overflow:hidden; }
.pmw-segment button { min-height:28px; padding:3px 9px; border:0; background:transparent; color:var(--text-secondary,#9a9faa); font:inherit; cursor:pointer; }
.pmw-segment button+button { border-left:1px solid var(--border-color,#3a3d45); }
.pmw-segment button.active { color:var(--text-primary,#fff); background:var(--active-bg,rgba(79,140,255,.17)); }
.pmw-sync-state { max-width:230px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; color:var(--text-secondary,#8f949e); font-size:11px; }
.pmw-main { display:flex; flex:1 1 auto; min-width:0; min-height:0; overflow:hidden; }
.pmw-rail { flex:0 0 210px; min-width:170px; border-right:1px solid var(--border-color,#34363d); display:flex; flex-direction:column; overflow:hidden; }
.pmw-rail-head { display:flex; align-items:center; min-height:40px; padding:6px 9px 4px 12px; color:var(--text-secondary,#9297a1); font-size:11px; font-weight:650; text-transform:uppercase; }
.pmw-projects { flex:1 1 auto; min-height:0; overflow:auto; padding:3px 6px 10px; }
.pmw-project { display:flex; align-items:center; gap:8px; width:100%; padding:7px 8px; border:0; border-radius:6px; background:transparent; color:var(--text-secondary,#a0a5af); font:inherit; text-align:left; cursor:pointer; }
.pmw-project:hover { background:var(--hover-bg,rgba(255,255,255,.05)); color:var(--text-primary,#fff); }
.pmw-project.active { background:var(--active-bg,rgba(79,140,255,.15)); color:var(--text-primary,#fff); }
.pmw-project-key { width:38px; flex:0 0 auto; color:var(--text-muted,#737985); font-size:10px; font-weight:700; }
.pmw-project-name { flex:1 1 auto; min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.pmw-count { flex:0 0 auto; color:var(--text-muted,#737985); font-size:11px; }
.pmw-work { flex:1 1 auto; min-width:0; min-height:0; overflow:hidden; display:flex; }
.pmw-content { flex:1 1 auto; min-width:320px; min-height:0; overflow:auto; }
.pmw-project-shell { container-type:inline-size; display:flex; flex-direction:column; width:100%; height:100%; min-height:0; color:var(--text-primary,#e4e6eb); }
.pmw-project-nav { position:sticky; top:0; z-index:4; display:flex; align-items:center; min-height:44px; padding:0 20px; gap:22px; border-bottom:1px solid var(--border-color,#34363d); background:var(--editor-surface,#1b1d23); }
.pmw-project-nav button { align-self:stretch; padding:0; border:0; border-bottom:2px solid transparent; background:transparent; color:var(--text-secondary,#9a9faa); font:inherit; font-size:12px; cursor:pointer; }
.pmw-project-nav button.active { border-bottom-color:var(--accent,#4f8cff); color:var(--text-primary,#fff); font-weight:650; }
.pmw-project-page { display:flex; flex-direction:column; flex:1 1 auto; min-height:0; padding:22px; gap:18px; }
.pmw-project-hero { display:flex; align-items:flex-start; gap:18px; }
.pmw-project-heading { flex:1 1 auto; min-width:0; }
.pmw-project-heading h2 { margin:0; color:var(--text-primary,#fff); font-size:22px; line-height:1.25; }
.pmw-project-heading p { max-width:760px; margin:6px 0 0; color:var(--text-secondary,#9a9faa); line-height:1.5; }
.pmw-stage-badge { flex:0 0 auto; padding:4px 8px; border:1px solid rgba(251,191,36,.34); border-radius:4px; background:rgba(251,191,36,.1); color:#fbbf24; font-size:10px; font-weight:700; text-transform:uppercase; }
.pmw-flow-rail { display:flex; min-height:76px; overflow:hidden; border:1px solid var(--border-color,#34363d); border-radius:6px; background:var(--bg-secondary,#202229); }
.pmw-flow-phase { display:flex; flex:1 1 0; flex-direction:column; justify-content:center; min-width:0; padding:12px 14px; border-right:1px solid var(--border-color,#34363d); }
.pmw-flow-phase:last-child { border-right:0; }.pmw-flow-phase.current { background:rgba(74,222,128,.055); }
.pmw-flow-phase-label { color:var(--text-muted,#7f8590); font-size:10px; font-weight:700; text-transform:uppercase; }.pmw-flow-phase.current .pmw-flow-phase-label { color:#86c7a5; }
.pmw-flow-phase-stages { margin-top:5px; overflow:hidden; color:var(--text-secondary,#a0a5af); font-size:12px; line-height:1.35; text-overflow:ellipsis; }.pmw-flow-phase.current .pmw-flow-phase-stages { color:var(--text-primary,#e4e6eb); }
.pmw-project-grid { display:grid; grid-template-columns:minmax(0,1fr) minmax(260px,32%); gap:16px; }
.pmw-project-page-nautflow { flex:1 1 auto; min-height:0; padding:0; gap:0; overflow:hidden; }
.pmw-flow-stage-nav { display:flex; flex:0 0 45px; min-height:45px; padding:0 12px; overflow-x:auto; overflow-y:hidden; border-bottom:1px solid var(--border-color,#34363d); background:var(--editor-surface,#1b1d23); scrollbar-width:thin; }
.pmw-flow-stage-nav button { flex:0 0 auto; padding:0 11px; border:0; border-bottom:2px solid transparent; background:transparent; color:var(--text-secondary,#9a9faa); font:inherit; font-size:11px; cursor:pointer; white-space:nowrap; }
.pmw-flow-stage-nav button:hover { color:var(--text-primary,#fff); }.pmw-flow-stage-nav button.active { border-bottom-color:var(--accent,#4f8cff); color:var(--text-primary,#fff); font-weight:650; }.pmw-flow-stage-nav button.current:not(.active)::after { content:''; display:inline-block; width:5px; height:5px; margin-left:6px; border-radius:50%; background:#fbbf24; vertical-align:middle; }
.pmw-nautflow { display:grid; grid-template-columns:230px minmax(0,1fr); flex:1 1 auto; min-height:0; overflow:hidden; background:var(--bg-secondary,#202229); }
.pmw-document-rail { display:flex; flex-direction:column; min-width:0; min-height:0; border-right:1px solid var(--border-color,#34363d); background:var(--editor-surface,#1b1d23); }.pmw-document-rail-head { display:flex; align-items:center; gap:8px; flex:0 0 auto; min-height:49px; padding:8px 9px 8px 13px; border-bottom:1px solid var(--border-color,#34363d); }.pmw-document-rail-head span { flex:1 1 auto; color:var(--text-muted,#7f8590); font-size:10px; font-weight:700; text-transform:uppercase; }.pmw-stage-files { flex:1 1 auto; min-height:0; overflow:auto; padding:7px; }.pmw-stage-file { display:flex; align-items:center; gap:8px; width:100%; min-height:46px; padding:6px 7px; border:1px solid transparent; border-radius:5px; background:transparent; color:var(--text-secondary,#9a9faa); font:inherit; text-align:left; cursor:pointer; }.pmw-stage-file:hover { background:var(--hover-bg,rgba(255,255,255,.05)); color:var(--text-primary,#fff); }.pmw-stage-file.active { border-color:var(--border-color,#3a3d45); background:var(--active-bg,rgba(79,140,255,.14)); color:var(--text-primary,#fff); }.pmw-stage-file svg { width:15px; height:15px; flex:0 0 auto; color:var(--accent,#4f8cff); }.pmw-stage-file-copy { min-width:0; flex:1 1 auto; }.pmw-stage-file-title { display:block; color:inherit; font-size:12px; }.pmw-stage-file-name { display:block; margin-top:2px; overflow:hidden; color:var(--text-muted,#7f8590); font-size:9px; text-overflow:ellipsis; white-space:nowrap; }.pmw-stage-file-empty { padding:14px 8px; color:var(--text-muted,#7f8590); font-size:11px; line-height:1.45; }
.pmw-stage-workspace { display:flex; flex-direction:column; min-width:0; min-height:0; }.pmw-stage-head { display:flex; align-items:flex-start; gap:12px; padding:18px 20px; border-bottom:1px solid var(--border-color,#34363d); }.pmw-stage-head h2 { margin:0; color:var(--text-primary,#fff); font-size:19px; }.pmw-stage-head p { margin:5px 0 0; color:var(--text-secondary,#9a9faa); font-size:12px; line-height:1.45; }.pmw-stage-body { display:flex; flex:1 1 auto; min-height:0; }.pmw-stage-document { display:flex; flex:1 1 auto; flex-direction:column; min-width:0; min-height:0; padding:18px; }.pmw-stage-toolbar { display:flex; align-items:center; flex-wrap:wrap; gap:8px; margin-bottom:10px; }.pmw-stage-ref { flex:1 1 auto; min-width:100px; overflow:hidden; color:var(--text-muted,#7f8590); font-size:10px; text-overflow:ellipsis; white-space:nowrap; }.pmw-promote-stage { margin-left:auto; }.pmw-stage-editor { flex:1 1 auto; width:100%; min-height:0; padding:14px; resize:none; border:1px solid var(--border-color,#3a3d45); border-radius:5px; background:var(--bg-primary,#17191f); color:var(--text-primary,#e4e6eb); font:12px/1.6 "SF Mono",Menlo,monospace; outline:none; }.pmw-stage-editor[hidden] { display:none; }.pmw-stage-editor:focus { border-color:var(--accent,#4f8cff); }.pmw-stage-preview { flex:1 1 auto; min-height:0; overflow:auto; padding:24px 30px; border:1px solid var(--border-color,#3a3d45); border-radius:5px; background:var(--bg-primary,#17191f); }.pmw-stage-preview[hidden] { display:none; }.pmw-stage-preview-toggle[data-active="1"] { border-color:var(--accent,#4f8cff); background:var(--active-bg,rgba(79,140,255,.14)); color:var(--accent,#4f8cff); }
.pmw-overview-layout { display:grid; grid-template-columns:minmax(0,1fr) 310px; gap:18px; min-height:0; }.pmw-overview-main,.pmw-overview-rail { display:flex; flex-direction:column; gap:16px; }.pmw-overview-band { padding:16px 0; border-top:1px solid var(--border-color,#34363d); }.pmw-overview-band:first-child { padding-top:0; border-top:0; }.pmw-overview-band-head { display:flex; align-items:center; gap:10px; margin-bottom:11px; }.pmw-overview-band-head h3 { margin:0; color:var(--text-primary,#fff); font-size:13px; }.pmw-overview-band-head span { margin-left:auto; color:var(--text-muted,#7f8590); font-size:10px; }.pmw-artifact-row,.pmw-contributor-row,.pmw-system-row { display:flex; align-items:center; gap:10px; min-height:36px; }.pmw-artifact-icon,.pmw-contributor-avatar { display:flex; align-items:center; justify-content:center; width:30px; height:30px; flex:0 0 auto; border-radius:5px; background:var(--bg-tertiary,#292c33); color:var(--accent,#4f8cff); font-size:10px; font-weight:700; }.pmw-artifact-icon svg { width:15px; height:15px; }.pmw-row-copy { min-width:0; flex:1 1 auto; }.pmw-row-title { color:var(--text-primary,#fff); font-size:12px; }.pmw-row-meta { margin-top:2px; color:var(--text-muted,#7f8590); font-size:10px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }.pmw-system-mark { width:20px; flex:0 0 auto; color:var(--accent,#4f8cff); font-size:10px; font-weight:700; }.pmw-system-state { color:#9BC5B0; font-size:10px; }.pmw-readiness { height:5px; overflow:hidden; border-radius:3px; background:var(--bg-tertiary,#292c33); }.pmw-readiness span { display:block; width:0%; height:100%; background:var(--accent,#4f8cff); }.pmw-ticket-lock { padding:12px; border:1px dashed var(--border-color,#3a3d45); border-radius:6px; color:var(--text-secondary,#9a9faa); font-size:11px; }
.pmw-active-work-wrap { overflow-x:auto; border:1px solid var(--border-color,#34363d); border-radius:6px; background:var(--bg-secondary,#202229); }.pmw-active-work-table { width:100%; min-width:690px; border-collapse:collapse; table-layout:fixed; }.pmw-active-work-table th { padding:8px 10px; border-bottom:1px solid var(--border-color,#34363d); color:var(--text-muted,#7f8590); font-size:9px; font-weight:700; text-align:left; text-transform:uppercase; }.pmw-active-work-table td { height:43px; padding:7px 10px; border-bottom:1px solid var(--border-color,#303239); color:var(--text-secondary,#a0a5af); font-size:11px; vertical-align:middle; }.pmw-active-work-table tbody tr:last-child td { border-bottom:0; }.pmw-active-work-table tr[data-overview-ticket] { cursor:pointer; outline:none; }.pmw-active-work-table tr[data-overview-ticket]:hover,.pmw-active-work-table tr[data-overview-ticket]:focus { background:var(--hover-bg,rgba(255,255,255,.045)); }.pmw-active-state { display:flex; align-items:center; gap:7px; color:var(--text-primary,#e4e6eb); font-weight:650; }.pmw-work-indicator { width:9px; height:9px; flex:0 0 auto; border-radius:50%; background:#737985; }.pmw-work-indicator[data-state="running"] { border:2px solid rgba(96,165,250,.28); border-top-color:#60a5fa; background:transparent; animation:pmw-work-spin .8s linear infinite; }.pmw-work-indicator[data-state="completed"] { background:#34d399; box-shadow:0 0 0 3px rgba(52,211,153,.1); }.pmw-work-indicator[data-state="blocked"],.pmw-work-indicator[data-state="failed"] { background:#f87171; box-shadow:0 0 0 3px rgba(248,113,113,.1); }.pmw-work-indicator[data-state="review"] { background:#fbbf24; }.pmw-work-indicator[data-state="ready"] { background:#a78bfa; }.pmw-active-item { min-width:0; }.pmw-active-item strong { display:block; overflow:hidden; color:var(--text-primary,#e4e6eb); font-size:11px; text-overflow:ellipsis; white-space:nowrap; }.pmw-active-item span { display:block; margin-top:2px; color:var(--text-muted,#7f8590); font-size:9px; }.pmw-active-activity { overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }.pmw-active-artifacts { color:var(--accent,#60a5fa); }.pmw-active-empty { padding:18px!important; color:var(--text-muted,#7f8590)!important; text-align:center; }.pmw-active-work-table th:nth-child(1){width:104px}.pmw-active-work-table th:nth-child(2){width:31%}.pmw-active-work-table th:nth-child(4){width:76px}.pmw-active-work-table th:nth-child(5){width:95px}.pmw-active-work-table th:nth-child(6){width:88px}@keyframes pmw-work-spin{to{transform:rotate(360deg)}}
.pmw-settings-form { display:flex; flex-direction:column; max-width:920px; gap:18px; }.pmw-settings-section { padding:17px; border:1px solid var(--border-color,#34363d); border-radius:6px; background:var(--bg-secondary,#202229); }.pmw-settings-section h3 { margin:0 0 13px; color:var(--text-primary,#fff); font-size:13px; }.pmw-settings-actions { position:sticky; bottom:0; display:flex; align-items:center; gap:8px; padding:12px 0; background:var(--editor-surface,#1b1d23); }
.pmw-changes { display:grid; grid-template-columns:280px minmax(0,1fr); min-height:540px; border-top:1px solid var(--border-color,#34363d); }.pmw-change-list { border-right:1px solid var(--border-color,#34363d); padding:10px; }.pmw-change-list-head { display:flex; align-items:center; min-height:36px; margin-bottom:7px; }.pmw-change-list-head strong { font-size:11px; text-transform:uppercase; }.pmw-change-item { display:block; width:100%; padding:9px; margin-bottom:4px; border:1px solid transparent; border-radius:6px; background:transparent; color:inherit; text-align:left; cursor:pointer; }.pmw-change-item:hover,.pmw-change-item.active { border-color:var(--border-color,#3a3d45); background:var(--hover-bg,rgba(255,255,255,.045)); }.pmw-change-item strong { display:block; overflow:hidden; font-size:11px; text-overflow:ellipsis; white-space:nowrap; }.pmw-change-item span { display:block; margin-top:3px; color:var(--text-muted,#7f8590); font-size:9px; }.pmw-change-workspace { min-width:0; padding:18px; overflow:auto; }.pmw-change-meta { display:flex; flex-wrap:wrap; gap:6px; margin:8px 0 16px; }.pmw-port { padding:3px 6px; border:1px solid var(--border-color,#3a3d45); border-radius:4px; color:var(--text-secondary,#a0a5af); font-size:9px; }.pmw-change-artifacts { display:grid; grid-template-columns:repeat(auto-fit,minmax(210px,1fr)); gap:8px; margin:10px 0 18px; }.pmw-change-artifact { padding:11px; border:1px solid var(--border-color,#34363d); border-radius:6px; background:var(--bg-secondary,#202229); }.pmw-change-artifact strong { display:block; font-size:11px; }.pmw-change-artifact span { display:block; margin:4px 0 9px; color:var(--text-muted,#7f8590); font-size:9px; overflow-wrap:anywhere; }.pmw-change-review { max-width:800px; padding-top:14px; border-top:1px solid var(--border-color,#34363d); }.pmw-change-actions { display:flex; flex-wrap:wrap; gap:7px; margin-top:11px; }
.pmw-surface { padding:16px; border:1px solid var(--border-color,#34363d); border-radius:6px; background:var(--bg-secondary,#202229); color:var(--text-primary,#e4e6eb); }
.pmw-surface h3 { margin:0 0 6px; color:var(--text-primary,#fff); font-size:15px; }.pmw-surface p { margin:0; color:var(--text-secondary,#9a9faa); line-height:1.5; }
.pmw-metric-row { display:grid; grid-template-columns:repeat(2,minmax(0,1fr)); gap:12px; margin-top:15px; }.pmw-metric label,.pmw-summary-label { display:block; margin-bottom:4px; color:var(--text-muted,#7f8590); font-size:10px; font-weight:650; text-transform:uppercase; }.pmw-metric strong { color:var(--text-primary,#fff); font-size:17px; }
.pmw-gate-list { display:flex; flex-direction:column; gap:9px; margin-top:13px; }.pmw-gate-item { display:flex; align-items:center; gap:9px; color:var(--text-secondary,#a0a5af); font-size:12px; }.pmw-gate-box { width:15px; height:15px; flex:0 0 auto; border:1px solid var(--border-color,#4a4e57); border-radius:3px; }
.pmw-project-empty { display:flex; flex-direction:column; align-items:flex-start; max-width:680px; padding:22px; border:1px dashed var(--border-color,#3a3d45); border-radius:6px; color:var(--text-secondary,#9a9faa); }
.pmw-board { display:flex; align-items:stretch; gap:0; min-width:max-content; height:100%; padding:0 8px 10px; }
.pmw-column { width:258px; min-width:258px; display:flex; flex-direction:column; border-right:1px solid var(--border-color,#303239); }
.pmw-column:last-child { border-right:0; }
.pmw-column-head { position:sticky; top:0; z-index:2; display:flex; align-items:center; gap:7px; min-height:42px; padding:8px 10px; background:var(--editor-surface,#1b1d23); color:var(--text-secondary,#a1a6b0); font-size:11px; font-weight:650; text-transform:uppercase; }
.pmw-status-dot { width:7px; height:7px; border-radius:50%; background:#717783; }
.pmw-status-dot[data-status="ready"] { background:#60a5fa; }.pmw-status-dot[data-status="in_progress"] { background:#fbbf24; }.pmw-status-dot[data-status="review"] { background:#a78bfa; }.pmw-status-dot[data-status="blocked"] { background:#f87171; }.pmw-status-dot[data-status="done"] { background:#34d399; }
.pmw-column-body { flex:1 1 auto; min-height:80px; padding:2px 8px 20px; }
.pmw-column-body.drag-over { background:rgba(79,140,255,.06); box-shadow:inset 0 0 0 1px rgba(79,140,255,.28); }
.pmw-card { display:flex; flex-direction:column; gap:7px; margin-bottom:7px; padding:9px 10px; border:1px solid var(--border-color,#383b43); border-radius:6px; background:var(--bg-secondary,#202229); cursor:pointer; }
.pmw-card:hover,.pmw-card.selected { border-color:var(--accent,#4f8cff); }.pmw-card.dragging { opacity:.45; }
.pmw-card-title { color:var(--text-primary,#e4e6eb); line-height:1.35; overflow-wrap:anywhere; }
.pmw-card-meta { display:flex; align-items:center; gap:6px; color:var(--text-muted,#7f8590); font-size:10px; }
.pmw-chip { padding:1px 6px; border:1px solid var(--border-color,#3a3d45); border-radius:999px; text-transform:capitalize; }
.pmw-priority-critical { color:#f87171; border-color:rgba(248,113,113,.4); }.pmw-priority-high { color:#fbbf24; border-color:rgba(251,191,36,.4); }
.pmw-list { width:100%; border-collapse:collapse; }.pmw-list th { position:sticky; top:0; z-index:2; padding:8px 10px; text-align:left; border-bottom:1px solid var(--border-color,#34363d); background:var(--editor-surface,#1b1d23); color:var(--text-muted,#858b96); font-size:10px; text-transform:uppercase; }.pmw-list td { padding:8px 10px; border-bottom:1px solid var(--border-color,#303239); vertical-align:middle; }.pmw-list tr[data-id] { cursor:pointer; }.pmw-list tr[data-id]:hover { background:var(--hover-bg,rgba(255,255,255,.04)); }
.pmw-detail { flex:0 0 clamp(360px,38%,520px); min-width:340px; display:flex; flex-direction:column; border-left:1px solid var(--border-color,#34363d); background:var(--bg-secondary,#181a20); }.pmw-detail[hidden] { display:none; }
.pmw-detail-head { display:flex; align-items:center; gap:8px; min-height:46px; padding:7px 10px 7px 14px; border-bottom:1px solid var(--border-color,#34363d); }.pmw-detail-id { color:var(--text-muted,#858b96); font-size:11px; font-weight:650; }
.pmw-detail-body { flex:1 1 auto; min-height:0; overflow:auto; padding:12px 14px 22px; }.pmw-field { display:flex; flex-direction:column; gap:5px; margin-bottom:11px; }.pmw-field>label { color:var(--text-muted,#858b96); font-size:10px; font-weight:650; text-transform:uppercase; }.pmw-field-grid { display:grid; grid-template-columns:repeat(3,minmax(0,1fr)); gap:8px; }.pmw-textarea { width:100%; min-height:130px; padding:8px 9px; resize:vertical; line-height:1.5; }.pmw-docs { min-height:72px; }
.pmw-detail-actions { display:flex; gap:7px; padding:9px 12px; border-top:1px solid var(--border-color,#34363d); }.pmw-activity { margin-top:18px; border-top:1px solid var(--border-color,#34363d); padding-top:12px; }.pmw-section-title { margin-bottom:8px; color:var(--text-muted,#858b96); font-size:10px; font-weight:650; text-transform:uppercase; }.pmw-event { display:grid; grid-template-columns:8px 1fr; gap:8px; padding:5px 0; }.pmw-event-dot { width:6px; height:6px; margin-top:5px; border-radius:50%; background:var(--accent,#4f8cff); }.pmw-event-name { font-size:12px; }.pmw-event-time { color:var(--text-muted,#7f8590); font-size:10px; }
.pmw-doc-links { display:flex; flex-wrap:wrap; gap:5px; }.pmw-doc-links .pmw-btn { display:flex; align-items:center; gap:5px; max-width:100%; overflow:hidden; text-overflow:ellipsis; }.pmw-doc-links svg { width:13px; height:13px; flex:0 0 auto; }
.pmw-empty { padding:28px; color:var(--text-secondary,#979ca6); }.pmw-error { color:#f87171; white-space:pre-wrap; }
.pmw-overlay { position:absolute; inset:0; z-index:20; display:flex; align-items:center; justify-content:center; padding:20px; background:rgba(5,7,10,.68); }.pmw-overlay[hidden] { display:none; }.pmw-dialog { width:min(520px,100%); max-height:calc(100% - 30px); overflow:auto; padding:16px; border:1px solid var(--border-color,#42454e); border-radius:7px; background:var(--bg-secondary,#202229); box-shadow:0 18px 50px rgba(0,0,0,.5); }.pmw-dialog-head { display:flex; align-items:center; margin-bottom:14px; }.pmw-dialog-title { font-size:15px; font-weight:650; }.pmw-dialog-actions { display:flex; justify-content:flex-end; gap:7px; margin-top:14px; }.pmw-toast { position:absolute; z-index:30; left:50%; bottom:16px; transform:translateX(-50%); max-width:80%; padding:7px 12px; border:1px solid var(--border-color,#444750); border-radius:6px; background:#262931; box-shadow:0 8px 25px rgba(0,0,0,.4); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }.pmw-toast.error { color:#f87171; }
.pmw-create-page { display:flex; flex-direction:column; width:min(980px,100%); max-height:calc(100% - 24px); overflow:hidden; border:1px solid var(--border-color,#42454e); border-radius:7px; background:var(--editor-surface,#1b1d23); color:var(--text-primary,#e4e6eb); box-shadow:0 18px 50px rgba(0,0,0,.5); }
.pmw-create-head { display:flex; align-items:flex-start; padding:20px 22px 16px; border-bottom:1px solid var(--border-color,#34363d); }.pmw-create-head h2 { margin:0; color:var(--text-primary,#fff); font-size:20px; }.pmw-create-head p { margin:5px 0 0; color:var(--text-secondary,#9a9faa); font-size:12px; }
.pmw-create-body { overflow:auto; padding:20px 22px 24px; }.pmw-create-section { padding-bottom:20px; }.pmw-create-section+.pmw-create-section { padding-top:18px; border-top:1px solid var(--border-color,#34363d); }.pmw-create-section h3 { margin:0 0 12px; color:var(--text-primary,#fff); font-size:13px; }.pmw-create-grid { display:grid; grid-template-columns:repeat(2,minmax(0,1fr)); gap:12px; }.pmw-create-grid-3 { grid-template-columns:repeat(3,minmax(0,1fr)); }.pmw-help { color:var(--text-muted,#7f8590); font-size:10px; line-height:1.4; }.pmw-flow-choice { display:grid; grid-template-columns:repeat(2,minmax(0,1fr)); gap:10px; }.pmw-flow-choice label { display:flex; gap:10px; padding:12px; border:1px solid var(--border-color,#3a3d45); border-radius:6px; cursor:pointer; }.pmw-flow-choice label:has(input:checked) { border-color:var(--accent,#4f8cff); background:var(--active-bg,rgba(79,140,255,.12)); }.pmw-flow-choice input { margin:2px 0 0; accent-color:var(--accent,#4f8cff); }.pmw-flow-choice strong { display:block; color:var(--text-primary,#fff); font-size:12px; }.pmw-flow-choice span { display:block; margin-top:3px; color:var(--text-secondary,#9a9faa); font-size:11px; line-height:1.4; }.pmw-create-actions { display:flex; align-items:center; gap:8px; padding:12px 22px; border-top:1px solid var(--border-color,#34363d); background:var(--bg-secondary,#202229); }
@media(max-width:1000px){.pmw-overview-layout{grid-template-columns:1fr}}
@media(max-width:900px){.pmw-rail{display:none}.pmw-detail{position:absolute;inset:0;z-index:8;min-width:0;flex-basis:auto}.pmw-work{position:relative}.pmw-sync-state{display:none}.pmw-project-grid{grid-template-columns:1fr}.pmw-create-grid-3{grid-template-columns:repeat(2,minmax(0,1fr))}}
@media(max-width:650px){.pmw-create-grid,.pmw-create-grid-3,.pmw-flow-choice{grid-template-columns:1fr}.pmw-flow-rail{flex-direction:column}.pmw-flow-phase{border-right:0;border-bottom:1px solid var(--border-color,#34363d)}.pmw-project-nav{gap:14px;overflow:auto}.pmw-create-actions .pmw-help{display:none}.pmw-nautflow{grid-template-columns:1fr}.pmw-document-rail{max-height:190px;border-right:0;border-bottom:1px solid var(--border-color,#34363d)}}
@media(max-width:800px){.pmw-changes{grid-template-columns:1fr}.pmw-change-list{max-height:220px;overflow:auto;border-right:0;border-bottom:1px solid var(--border-color,#34363d)}}
@container(max-width:760px){.pmw-nautflow{grid-template-columns:180px minmax(0,1fr)}.pmw-stage-ref{flex-basis:100%}.pmw-stage-document{padding:12px}.pmw-stage-head{padding:14px}.pmw-overview-layout{grid-template-columns:1fr}}
@container(max-width:520px){.pmw-nautflow{grid-template-columns:1fr}.pmw-document-rail{max-height:180px;border-right:0;border-bottom:1px solid var(--border-color,#34363d)}}
`;
    document.head.appendChild(style);
  }

  function createPanel(tabId, parent, opts) {
    opts = opts || {};
    injectStyles();
    const label = `pmw-${Date.now().toString(36)}-${++counter}`;
    const pane = document.createElement('div');
    pane.className = 'pmw';
    pane.innerHTML = `
      <header class="pmw-head">
        <span class="pmw-title">Projects</span>
        <select class="pmw-project-select" aria-label="Project filter"></select>
        <input class="pmw-filter" type="search" placeholder="Filter tickets" spellcheck="false">
        <div class="pmw-segment pmw-view-switch"><button data-view="board" class="active">Board</button><button data-view="list">List</button></div>
        <span class="pmw-spacer"></span><span class="pmw-sync-state"></span>
        <button class="pmw-icon pmw-refresh" title="Refresh" aria-label="Refresh">${ICON.refresh}</button>
        <button class="pmw-icon pmw-sync" title="Pull and push control repository" aria-label="Synchronize">${ICON.sync}</button>
        <button class="pmw-btn pmw-project-details" hidden>Project details</button>
        <button class="pmw-btn pmw-new-project">New project</button>
        <button class="pmw-btn pmw-btn-primary pmw-new-ticket">New ticket</button>
      </header>
      <div class="pmw-main">
        <aside class="pmw-rail"><div class="pmw-rail-head">Projects</div><div class="pmw-projects"></div></aside>
        <div class="pmw-work"><main class="pmw-content"></main><aside class="pmw-detail" hidden></aside></div>
      </div>
      <div class="pmw-overlay" hidden></div>`;
    parent.appendChild(pane);

    const $ = (selector) => pane.querySelector(selector);
    const state = { projects: [], tickets: [], changes: [], status: null, project: opts.project || '', section: opts.section || (opts.project ? 'overview' : 'work'), flowStage: opts.flowStage || '', view: 'board', selected: null, selectedChange: '', events: [], request: 0 };

    function toast(message, error) {
      const node = document.createElement('div');
      node.className = `pmw-toast${error ? ' error' : ''}`;
      node.textContent = String(message);
      pane.appendChild(node);
      setTimeout(() => node.remove(), 3500);
    }

    function projectName(key) {
      const project = state.projects.find((item) => item.key === key);
      return project ? project.name : key;
    }

    function projectKeySeed(name) {
      let key = String(name || '').replace(/[^a-z0-9]/gi, '').toUpperCase().slice(0, 12);
      if (key.length === 1) key += 'X';
      return key;
    }

    function money(value) {
      const amount = Number(value);
      return Number.isFinite(amount) ? `CHF ${amount.toLocaleString(undefined, { maximumFractionDigits: 0 })}` : 'Not set';
    }

    function projectTabs(active) {
      const tabs = [['overview', 'Overview'], ['nautflow', 'NAUT-Flow'], ['changes', 'Change Management'], ['artifacts', 'Artifacts'], ['work', 'Work'], ['delivery', 'Delivery'], ['settings', 'Settings']];
      return `<nav class="pmw-project-nav">${tabs.map(([section, label]) => `<button data-project-section="${section}" class="${active === section ? 'active' : ''}">${label}</button>`).join('')}</nav>`;
    }

    function stagesFor(project) {
      return project.flow_type === 'incident' ? INCIDENT_STAGES : STANDARD_STAGES;
    }

    function flowPhases(project) {
      if (project.flow_type === 'incident') {
        return [
          ['Resolve', 'Intake · RCA · Action plan'],
          ['Execute', 'Ticket · Build · Test'],
          ['Close', 'Release · Engram'],
        ];
      }
      return [
        ['Discover', 'Idea · Concept · Business case'],
        ['Define', 'PRD · Architecture · Data · API · Security'],
        ['Plan', 'Development plan · Stories · Tickets'],
        ['Deliver', 'Build · Test · Review · Release · Engram'],
      ];
    }

    function projectContext(project) {
      const legacy = project.client || {};
      return {
        purpose: project.purpose || legacy.scope || 'Define the project purpose and expected outcome.',
        client: project.client_name || legacy.client_company || '',
        budget: project.budget_chf == null ? legacy.offer_amount_chf : project.budget_chf,
        rate: project.hourly_rate_chf == null ? legacy.rate_chf_per_hour : project.hourly_rate_chf,
      };
    }

    function bindProjectTabs() {
      $('.pmw-content').querySelectorAll('[data-project-section]').forEach((button) => {
        button.onclick = () => {
          state.section = button.dataset.projectSection;
          state.selected = null;
          renderDetail();
          renderContent();
        };
      });
    }

    function visibleTickets() {
      const query = $('.pmw-filter').value.trim().toLowerCase();
      return state.tickets.filter((ticket) => {
        if (state.project && ticket.project !== state.project) return false;
        if (!query) return true;
        return `${ticket.id} ${ticket.title} ${ticket.body} ${ticket.owner || ''} ${ticket.ticket_type}`.toLowerCase().includes(query);
      });
    }

    function renderProjectFilters() {
      const counts = state.tickets.reduce((map, ticket) => map.set(ticket.project, (map.get(ticket.project) || 0) + 1), new Map());
      const options = ['<option value="">All projects</option>'].concat(state.projects.map((project) => `<option value="${esc(project.key)}">${esc(project.key)} - ${esc(project.name)}</option>`));
      $('.pmw-project-select').innerHTML = options.join('');
      $('.pmw-project-select').value = state.project;
      $('.pmw-project-details').hidden = !state.project;
      $('.pmw-projects').innerHTML = `<button class="pmw-project${state.project ? '' : ' active'}" data-project=""><span class="pmw-project-key">ALL</span><span class="pmw-project-name">All tickets</span><span class="pmw-count">${state.tickets.length}</span></button>` + state.projects.map((project) => `<button class="pmw-project${state.project === project.key ? ' active' : ''}" data-project="${esc(project.key)}"><span class="pmw-project-key">${esc(project.key)}</span><span class="pmw-project-name">${esc(project.name)}</span><span class="pmw-count">${counts.get(project.key) || 0}</span></button>`).join('');
      $('.pmw-projects').querySelectorAll('[data-project]').forEach((button) => {
        button.onclick = () => selectProject(button.dataset.project || '');
      });
    }

    function ticketCard(ticket) {
      return `<article class="pmw-card${state.selected && state.selected.id === ticket.id ? ' selected' : ''}" data-id="${esc(ticket.id)}" draggable="true"><div class="pmw-card-title">${esc(ticket.title)}</div><div class="pmw-card-meta"><span>${esc(ticket.id)}</span><span class="pmw-chip">${esc(ticket.ticket_type)}</span><span class="pmw-chip pmw-priority-${esc(ticket.priority)}">${esc(ticket.priority)}</span>${ticket.owner ? `<span>${esc(ticket.owner)}</span>` : ''}</div></article>`;
    }

    function bindTickets() {
      $('.pmw-content').querySelectorAll('[data-id]').forEach((node) => {
        node.onclick = () => openTicket(node.dataset.id);
        if (node.classList.contains('pmw-card')) {
          node.ondragstart = (event) => { node.classList.add('dragging'); event.dataTransfer.setData('text/plain', node.dataset.id); };
          node.ondragend = () => node.classList.remove('dragging');
        }
      });
      $('.pmw-content').querySelectorAll('[data-drop-status]').forEach((column) => {
        column.ondragover = (event) => { event.preventDefault(); column.classList.add('drag-over'); };
        column.ondragleave = () => column.classList.remove('drag-over');
        column.ondrop = async (event) => {
          event.preventDefault(); column.classList.remove('drag-over');
          const ticket = state.tickets.find((item) => item.id === event.dataTransfer.getData('text/plain'));
          const status = column.dataset.dropStatus;
          if (!ticket || ticket.status === status) return;
          await updateTicket(ticket, { status });
        };
      });
    }

    function ticketWorkspace(tickets) {
      if (state.view === 'list') {
        return `<table class="pmw-list"><thead><tr><th>ID</th><th>Title</th><th>Project</th><th>Type</th><th>Priority</th><th>Status</th><th>Owner</th><th>Updated</th></tr></thead><tbody>${tickets.map((ticket) => `<tr data-id="${esc(ticket.id)}"><td>${esc(ticket.id)}</td><td>${esc(ticket.title)}</td><td>${esc(ticket.project)}</td><td>${esc(ticket.ticket_type)}</td><td><span class="pmw-chip pmw-priority-${esc(ticket.priority)}">${esc(ticket.priority)}</span></td><td>${esc(LABELS[ticket.status] || ticket.status)}</td><td>${esc(ticket.owner || '')}</td><td>${esc(relativeTime(ticket.updated_at))}</td></tr>`).join('')}</tbody></table>`;
      }
      return `<div class="pmw-board">${STATUSES.map((status) => { const items = tickets.filter((ticket) => ticket.status === status); return `<section class="pmw-column"><header class="pmw-column-head"><span class="pmw-status-dot" data-status="${status}"></span><span>${esc(LABELS[status])}</span><span class="pmw-count">${items.length}</span></header><div class="pmw-column-body" data-drop-status="${status}">${items.map(ticketCard).join('')}</div></section>`; }).join('')}</div>`;
    }

    function stageDescription(key) {
      const descriptions = {
        idea: 'Capture the problem, target users, expected value, and initial boundaries.',
        concept: 'Write the project concept and turn the approved idea into a bounded solution direction.',
        business_case: 'Establish business value, costs, risks, assumptions, and success measures.',
        prd: 'Define functional requirements, non-functional requirements, scope, and acceptance criteria.',
        architecture: 'Define system boundaries, components, integrations, runtime choices, and trade-offs.',
        data_model: 'Define entities, relationships, ownership, retention, and migration requirements.',
        api_design: 'Define interfaces, contracts, authentication, errors, and versioning.',
        security_review: 'Identify threats, controls, data exposure, secrets, permissions, and residual risks.',
        development_plan: 'Sequence implementation into independently verifiable milestones and dependencies.',
        sprint_stories: 'Turn the plan into scoped stories with acceptance criteria and test expectations.',
        tickets: 'Create executable work items only after the implementation plan is approved.',
        build: 'Execute approved tickets and link branches, worktrees, commits, and pull requests.',
        test_review: 'Verify behavior independently and record evidence, regressions, and unresolved risks.',
        release: 'Prepare, approve, publish, and verify the release.',
        learning: 'Record verified learning and coding anti-patterns in Engram.',
        intake: 'Capture impact, symptoms, environment, timing, and available evidence.',
        rca: 'Establish the root cause and distinguish evidence from assumptions.',
        action_plan: 'Define remediation, verification, ownership, and rollback steps.',
        ticket: 'Create the approved implementation ticket for the incident remediation.',
      };
      return descriptions[key] || 'Create and review the artifact required to complete this stage.';
    }

    function stageDocumentRef(project, stage, index) {
      const folder = String(project.name || project.key).replace(/[\\/:*?"<>|]/g, '-').trim() || project.key;
      const file = stage[2].replace(/[^a-z0-9]+/gi, '-').replace(/^-|-$/g, '');
      return `Development/${folder}/NAUT-Flow/${String(index + 1).padStart(2, '0')}-${file}.md`;
    }

    function stageVersionRef(baseRel, version) {
      return Number(version) <= 1 ? baseRel : baseRel.replace(/\.md$/i, `_v${Number(version)}.md`);
    }

    async function stageVersionDocuments(baseRel) {
      let tree;
      try {
        tree = await invoke('vault_tree', { vault: 'work' });
      } catch (error) {
        if (!String(error).includes('vault not open')) throw error;
        await invoke('vault_open', { vault: 'work' });
        tree = await invoke('vault_tree', { vault: 'work' });
      }
      const stem = baseRel.replace(/\.md$/i, '');
      const documents = new Map();
      for (const note of tree?.notes || []) {
        if (note.rel === baseRel) documents.set(1, baseRel);
        const match = String(note.rel || '').match(new RegExp(`^${stem.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}_v(\\d+)\\.md$`, 'i'));
        if (match) documents.set(Number(match[1]), note.rel);
      }
      return Array.from(documents, ([version, rel]) => ({ version, rel })).filter((item) => Number.isFinite(item.version)).sort((a, b) => a.version - b.version);
    }

    function stageTemplate(project, stage) {
      return `# ${stage[2]}\n\n## Purpose\n\n${stageDescription(stage[0])}\n\n## Project context\n\n${project.purpose || ''}\n\n## Decisions\n\n\n## Open questions\n\n\n## Acceptance and review\n\n`;
    }

    function promotedStageTemplate(project, sourceStage, targetStage, sourceRel) {
      const sourceLink = sourceRel.replace(/\.md$/i, '');
      return `---\nnaut_flow: true\nproject: ${project.name}\nproject_key: ${project.key}\nstage: ${targetStage[0]}\nstatus: draft\npromoted_from: work:${sourceRel}\n---\n\n# ${targetStage[2]}\n\n> Promoted input: [[${sourceLink}|${sourceStage[2]}]]\n\n## Handoff validation\n\nPending validation by the ${targetStage[3]}.\n\n## Purpose\n\n${stageDescription(targetStage[0])}\n\n## Decisions\n\n\n## Open questions\n\n\n## Acceptance and review\n\n`;
    }

    function projectUpdatePayload(project, stage) {
      const context = projectContext(project);
      return {
        key: project.key,
        expected_revision: project.revision || 1,
        name: project.name,
        purpose: project.purpose || project.client?.scope || '',
        owner: project.owner || '',
        client_name: project.client_name || context.client,
        contact_name: project.contact_name || '',
        contact_email: project.contact_email || '',
        budget_chf: project.budget_chf == null ? context.budget : project.budget_chf,
        hourly_rate_chf: project.hourly_rate_chf == null ? context.rate : project.hourly_rate_chf,
        flow_type: project.flow_type || 'standard',
        source_repo: project.source_repo || '',
        stage,
      };
    }

    function renderNautFlow(project) {
      const stages = stagesFor(project);
      const currentKey = stages.some((stage) => stage[0] === project.stage) ? project.stage : stages[0][0];
      if (!state.flowStage || !stages.some((stage) => stage[0] === state.flowStage)) state.flowStage = currentKey;
      const selectedIndex = Math.max(0, stages.findIndex((stage) => stage[0] === state.flowStage));
      const selected = stages[selectedIndex];
      const stageNav = stages.map((stage) => `<button class="${stage[0] === selected[0] ? 'active' : ''}${stage[0] === currentKey ? ' current' : ''}" data-flow-stage="${esc(stage[0])}">${esc(stage[2])}</button>`).join('');
      const rel = stageDocumentRef(project, selected, selectedIndex);
      const next = stages[selectedIndex + 1];
      const currentIndex = Math.max(0, stages.findIndex((stage) => stage[0] === currentKey));
      const promote = next && selectedIndex >= currentIndex ? `<button class="pmw-btn pmw-btn-primary pmw-promote-stage">Promote to ${esc(next[2])}</button>` : '';
      return `<div class="pmw-project-page pmw-project-page-nautflow"><nav class="pmw-flow-stage-nav" aria-label="NAUT-Flow stages">${stageNav}</nav><div class="pmw-nautflow"><aside class="pmw-document-rail"><header class="pmw-document-rail-head"><span>${esc(selected[2])} documents</span><button class="pmw-icon pmw-stage-new-version" title="Create next version" aria-label="Create next version">${ICON.plus}</button></header><div class="pmw-stage-files"><div class="pmw-stage-file-empty">Loading documents...</div></div></aside><section class="pmw-stage-workspace"><header class="pmw-stage-head"><div><h2>${esc(selected[2])}</h2><p>${esc(stageDescription(selected[0]))}</p></div><span class="pmw-spacer"></span><span class="pmw-stage-badge">Draft</span></header><div class="pmw-stage-body"><div class="pmw-stage-document"><div class="pmw-stage-toolbar"><span class="pmw-stage-ref">work:${esc(rel)}</span><button class="pmw-icon pmw-stage-preview-toggle" title="Preview document" aria-label="Preview document">${ICON.eye}</button><button class="pmw-btn pmw-stage-open">Open in Vault</button><button class="pmw-btn pmw-stage-save">Save document</button><button class="pmw-btn pmw-ask-agent">Work with ${esc(selected[3])}</button><button class="pmw-btn pmw-request-review">Request review</button>${promote}</div><textarea class="pmw-stage-editor" spellcheck="true">${esc(stageTemplate(project, selected))}</textarea><div class="pmw-stage-preview xnaut-md" hidden></div></div></div></section></div></div>`;
    }

    function renderSettings(project) {
      const context = projectContext(project);
      const purpose = project.purpose || project.client?.scope || '';
      return `<div class="pmw-project-page"><div class="pmw-project-hero"><div class="pmw-project-heading"><h2>Project settings</h2><p>Editable project baselines and connections. The project key remains stable because it identifies tickets.</p></div><span class="pmw-stage-badge">Revision ${esc(project.revision || 1)}</span></div><form class="pmw-settings-form"><section class="pmw-settings-section"><h3>Basics</h3><div class="pmw-create-grid"><div class="pmw-field"><label>Project key</label><input class="pmw-input" value="${esc(project.key)}" disabled><span class="pmw-help">Used for ticket IDs and cannot be changed.</span></div><div class="pmw-field"><label>Name</label><input class="pmw-input pmw-settings-name" value="${esc(project.name)}" required></div></div><div class="pmw-field"><label>Purpose</label><textarea class="pmw-textarea pmw-settings-purpose" placeholder="What problem does this project solve, for whom, and what outcome should it achieve?" required>${esc(purpose)}</textarea></div><div class="pmw-field"><label>NAUT-Flow</label><select class="pmw-select pmw-settings-flow"><option value="standard"${project.flow_type !== 'incident' ? ' selected' : ''}>Standard project</option><option value="incident"${project.flow_type === 'incident' ? ' selected' : ''}>Incident fast track</option></select></div></section><section class="pmw-settings-section"><h3>Ownership</h3><div class="pmw-create-grid pmw-create-grid-3"><div class="pmw-field"><label>Project owner</label><input class="pmw-input pmw-settings-owner" value="${esc(project.owner || '')}"></div><div class="pmw-field"><label>Client</label><input class="pmw-input pmw-settings-client" value="${esc(context.client)}"></div><div class="pmw-field"><label>Primary contact</label><input class="pmw-input pmw-settings-contact" value="${esc(project.contact_name || '')}"></div></div><div class="pmw-field"><label>Contact email</label><input class="pmw-input pmw-settings-email" type="email" value="${esc(project.contact_email || '')}"></div></section><section class="pmw-settings-section"><h3>Repository and commercial baseline</h3><div class="pmw-field"><label>Source repository or local folder</label><input class="pmw-input pmw-settings-source" value="${esc(project.source_repo || '')}"></div><div class="pmw-create-grid"><div class="pmw-field"><label>Budget (CHF)</label><input class="pmw-input pmw-settings-budget" type="number" min="0" step="1" value="${context.budget == null ? '' : esc(context.budget)}"></div><div class="pmw-field"><label>Hourly rate (CHF)</label><input class="pmw-input pmw-settings-rate" type="number" min="0" step="0.01" value="${context.rate == null ? '' : esc(context.rate)}"></div></div></section><div class="pmw-settings-actions"><span class="pmw-settings-state pmw-help"></span><span class="pmw-spacer"></span><button type="submit" class="pmw-btn pmw-btn-primary pmw-settings-save">Save settings</button></div></form></div>`;
    }

    function activeWorkState(ticket) {
      const states = {
        in_progress: ['running', 'Running', 'Implementation in progress'],
        done: ['completed', 'Completed', 'Work completed'],
        blocked: ['blocked', 'Blocked', 'Blocked or requires attention'],
        failed: ['failed', 'Issue', 'Execution failed'],
        review: ['review', 'Review', 'Ready for independent review'],
        ready: ['ready', 'Ready', 'Ready to start'],
        inbox: ['planned', 'Planned', 'Awaiting prioritization'],
      };
      return states[ticket.status] || ['planned', LABELS[ticket.status] || ticket.status, 'Status pending'];
    }

    function renderActiveWork(tickets) {
      const rank = { in_progress: 0, blocked: 1, failed: 1, done: 2, review: 3, ready: 4, inbox: 5 };
      const items = tickets.slice().sort((a, b) => {
        const status = (rank[a.status] ?? 6) - (rank[b.status] ?? 6);
        return status || String(b.updated_at || '').localeCompare(String(a.updated_at || ''));
      }).slice(0, 8);
      const active = tickets.filter((ticket) => ticket.status === 'in_progress').length;
      const completed = tickets.filter((ticket) => ticket.status === 'done').length;
      const rows = items.map((ticket) => {
        const [stateKey, stateLabel, activity] = activeWorkState(ticket);
        const artifacts = Array.isArray(ticket.documentation) ? ticket.documentation.length : 0;
        return `<tr data-overview-ticket="${esc(ticket.id)}" tabindex="0"><td><span class="pmw-active-state"><span class="pmw-work-indicator" data-state="${esc(stateKey)}"></span>${esc(stateLabel)}</span></td><td><div class="pmw-active-item"><strong>${esc(ticket.title)}</strong><span>${esc(ticket.id)} · ${esc(ticket.ticket_type)}</span></div></td><td class="pmw-active-activity">${esc(activity)}</td><td class="pmw-active-artifacts">${artifacts ? `${artifacts} linked` : 'None'}</td><td>${esc(ticket.owner || 'Unassigned')}</td><td>${esc(relativeTime(ticket.updated_at))}</td></tr>`;
      }).join('');
      const body = rows || '<tr><td class="pmw-active-empty" colspan="6">No project work has been created yet.</td></tr>';
      return `<section class="pmw-overview-band"><div class="pmw-overview-band-head"><h3>Active work</h3><span>${active} running · ${completed} completed</span></div><div class="pmw-active-work-wrap"><table class="pmw-active-work-table"><thead><tr><th>State</th><th>Work item</th><th>Activity</th><th>Artifacts</th><th>Owner</th><th>Updated</th></tr></thead><tbody>${body}</tbody></table></div></section>`;
    }

    function changeForProject(project) {
      return state.changes.filter((change) => change.project === project.key);
    }

    function renderChanges(project) {
      const changes = changeForProject(project);
      if (!state.selectedChange || !changes.some((change) => change.id === state.selectedChange)) state.selectedChange = changes[0]?.id || '';
      const selected = changes.find((change) => change.id === state.selectedChange);
      const list = changes.length ? changes.map((change) => `<button class="pmw-change-item${change.id === state.selectedChange ? ' active' : ''}" data-change-id="${esc(change.id)}"><strong>${esc(change.title)}</strong><span>${esc(change.profile)} · ${esc(String(change.status).replaceAll('_', ' '))}</span></button>`).join('') : '<div class="pmw-empty">No Changes yet.</div>';
      let workspace = '<div class="pmw-project-empty"><h3>No Change selected</h3><p>Create a Feature, Bug, Incident, or Maintenance Change. Its artifacts remain separate from the approved project baseline.</p></div>';
      if (selected) {
        const artifacts = (selected.artifacts || []).map((artifact) => `<div class="pmw-change-artifact"><strong>${esc(String(artifact.kind).replaceAll('_', ' '))}</strong><span>${esc(artifact.vault_ref)} · ${esc(artifact.status)}</span><button class="pmw-btn pmw-change-open-artifact" data-ref="${esc(artifact.vault_ref)}">Open</button> <button class="pmw-btn pmw-change-artifact-ready" data-kind="${esc(artifact.kind)}" data-ready="${artifact.status !== 'ready'}">${artifact.status === 'ready' ? 'Return to draft' : 'Mark ready'}</button></div>`).join('');
        const latestReview = (selected.reviews || []).at(-1);
        const readiness = (selected.artifacts || []).filter((artifact) => artifact.status === 'ready').length;
        const canReview = selected.status === 'ready_for_review';
        const canApprove = selected.status === 'awaiting_approval';
        workspace = `<header class="pmw-project-hero"><div class="pmw-project-heading"><h2>${esc(selected.title)}</h2><p>${esc(selected.summary || 'No Change summary provided.')}</p></div><span class="pmw-stage-badge">${esc(String(selected.status).replaceAll('_', ' '))}</span></header><div class="pmw-change-meta"><span class="pmw-port">${esc(selected.id)}</span><span class="pmw-port">${esc(selected.profile)}</span><span class="pmw-port">Revision ${esc(selected.revision)}</span><span class="pmw-port">Artifacts ${readiness}/${(selected.artifacts || []).length}</span></div>${selected.source_ticket ? `<div class="pmw-field"><label>Source ticket</label><div>${esc(selected.source_ticket)}${selected.source_url ? ` · ${esc(selected.source_url)}` : ''}</div></div>` : ''}<div class="pmw-field"><label>Canonical baseline (read only)</label><div class="pmw-doc-links">${(selected.baseline_refs || []).length ? selected.baseline_refs.map((ref) => `<button class="pmw-doc-link pmw-change-open-artifact" data-ref="${esc(ref)}">${esc(ref)}</button>`).join('') : '<span class="pmw-help">No baseline documents found.</span>'}</div></div><div class="pmw-section-title">Change artifacts</div><div class="pmw-change-artifacts">${artifacts}</div><section class="pmw-change-review"><div class="pmw-section-title">Independent review and approval</div>${latestReview ? `<div class="pmw-field"><label>Latest review</label><div><strong>${esc(latestReview.verdict)}</strong> by ${esc(latestReview.reviewer)} · ${esc(latestReview.summary || '')}</div></div>` : ''}<div class="pmw-field-grid"><div class="pmw-field"><label>Reviewer</label><input class="pmw-input pmw-change-reviewer" value="Reviewer Agent"></div><div class="pmw-field"><label>Review summary</label><input class="pmw-input pmw-change-review-summary" placeholder="Evidence-bound review verdict"></div></div><div class="pmw-field"><label>Findings (one per line)</label><textarea class="pmw-textarea pmw-change-findings" placeholder="Finding and required correction"></textarea></div><div class="pmw-change-actions"><button class="pmw-btn pmw-change-refresh">Refresh readiness</button><button class="pmw-btn pmw-change-run">Open workflow run</button><button class="pmw-btn pmw-change-review-changes"${canReview ? '' : ' disabled'}>Changes required</button><button class="pmw-btn pmw-change-review-approve"${canReview ? '' : ' disabled'}>Approve review</button><span class="pmw-spacer"></span><button class="pmw-btn pmw-change-reject"${canApprove ? '' : ' disabled'}>Reject Change</button><button class="pmw-btn pmw-btn-primary pmw-change-approve"${canApprove ? '' : ' disabled'}>Approve for execution</button></div></section>`;
      }
      return `<div class="pmw-project-page pmw-project-page-changes"><div class="pmw-project-hero"><div class="pmw-project-heading"><h2>Change Management</h2><p>Proposed behavior stays isolated until artifacts, independent review, and human approval are complete.</p></div><button class="pmw-btn pmw-btn-primary pmw-new-change">New Change</button></div><div class="pmw-changes"><aside class="pmw-change-list"><div class="pmw-change-list-head"><strong>Changes</strong><span class="pmw-spacer"></span><span class="pmw-help">${changes.length}</span></div>${list}</aside><section class="pmw-change-workspace">${workspace}</section></div></div>`;
    }

    function showChangeDialog(project) {
      const overlay = $('.pmw-overlay');
      overlay.hidden = false;
      overlay.innerHTML = `<form class="pmw-dialog pmw-change-create"><div class="pmw-dialog-head"><span class="pmw-dialog-title">New Change · ${esc(project.key)}</span><span class="pmw-spacer"></span><button type="button" class="pmw-icon pmw-dialog-close">${ICON.close}</button></div><div class="pmw-field"><label>Title</label><input class="pmw-input pmw-change-title" required></div><div class="pmw-field"><label>Profile</label><select class="pmw-select pmw-change-profile"><option value="feature">Feature</option><option value="bug">Bug</option><option value="incident">Incident</option><option value="maintenance">Maintenance</option></select></div><div class="pmw-field"><label>Summary</label><textarea class="pmw-textarea pmw-change-summary" required></textarea></div><div class="pmw-field-grid"><div class="pmw-field"><label>Source ticket</label><input class="pmw-input pmw-change-source-ticket" placeholder="XNAUT-42 or repo#42"></div><div class="pmw-field"><label>Source URL</label><input class="pmw-input pmw-change-source-url" type="url"></div></div><div class="pmw-field"><label>Drafting Agent</label><input class="pmw-input pmw-change-agent" value="Analyst"></div><div class="pmw-dialog-actions"><button type="button" class="pmw-btn pmw-dialog-cancel">Cancel</button><button type="submit" class="pmw-btn pmw-btn-primary">Create Change</button></div></form>`;
      const close = () => { overlay.hidden = true; overlay.innerHTML = ''; };
      overlay.querySelectorAll('.pmw-dialog-close,.pmw-dialog-cancel').forEach((button) => { button.onclick = close; });
      overlay.querySelector('form').onsubmit = async (event) => {
        event.preventDefault();
        const submit = overlay.querySelector('[type="submit"]'); submit.disabled = true; submit.textContent = 'Creating...';
        try {
          const change = await invoke('pm_change_create', { request: { project: project.key, title: overlay.querySelector('.pmw-change-title').value, profile: overlay.querySelector('.pmw-change-profile').value, summary: overlay.querySelector('.pmw-change-summary').value, source_ticket: overlay.querySelector('.pmw-change-source-ticket').value, source_url: overlay.querySelector('.pmw-change-source-url').value, agents: [overlay.querySelector('.pmw-change-agent').value].filter(Boolean) } });
          state.changes.unshift(change); state.selectedChange = change.id; close(); renderContent(); toast('Change created');
        } catch (error) { toast(error, true); submit.disabled = false; submit.textContent = 'Create Change'; }
      };
    }

    function bindChanges(project) {
      pane.querySelectorAll('[data-change-id]').forEach((button) => { button.onclick = () => { state.selectedChange = button.dataset.changeId; renderContent(); }; });
      const selected = state.changes.find((change) => change.id === state.selectedChange);
      const create = $('.pmw-new-change'); if (create) create.onclick = () => showChangeDialog(project);
      if (!selected) return;
      pane.querySelectorAll('.pmw-change-open-artifact').forEach((button) => { button.onclick = () => openDocument(button.dataset.ref); });
      const update = (change) => { const index = state.changes.findIndex((item) => item.id === change.id); if (index >= 0) state.changes[index] = change; renderContent(); };
      pane.querySelectorAll('.pmw-change-artifact-ready').forEach((button) => { button.onclick = async () => { try { update(await invoke('pm_change_set_artifact_status', { request: { project: project.key, change_id: selected.id, expected_revision: selected.revision, kind: button.dataset.kind, ready: button.dataset.ready === 'true' } })); } catch (error) { toast(error, true); } }; });
      $('.pmw-change-refresh').onclick = async () => { try { update(await invoke('pm_change_refresh', { project: project.key, changeId: selected.id, expectedRevision: selected.revision })); toast('Artifact readiness refreshed'); } catch (error) { toast(error, true); } };
      $('.pmw-change-run').onclick = () => window.xnautAttachLoopsTab?.({ view: 'runs' });
      const review = async (verdict) => { try { update(await invoke('pm_change_review', { request: { project: project.key, change_id: selected.id, expected_revision: selected.revision, reviewer: $('.pmw-change-reviewer').value, verdict, summary: $('.pmw-change-review-summary').value, findings: $('.pmw-change-findings').value.split('\n').map((value) => value.trim()).filter(Boolean) } })); toast('Independent review recorded'); } catch (error) { toast(error, true); } };
      $('.pmw-change-review-changes').onclick = () => review('changes_required');
      $('.pmw-change-review-approve').onclick = () => review('approved');
      const approve = async (approved) => { try { update(await invoke('pm_change_approve', { request: { project: project.key, change_id: selected.id, expected_revision: selected.revision, actor: 'xNAUT user', approved, comment: '' } })); toast(approved ? 'Change approved for execution' : 'Change returned for changes'); } catch (error) { toast(error, true); } };
      $('.pmw-change-reject').onclick = () => approve(false);
      $('.pmw-change-approve').onclick = () => approve(true);
    }

    function renderProjectSection(project, tickets) {
      const context = projectContext(project);
      const stages = stagesFor(project);
      const stage = stages.some((item) => item[0] === project.stage) ? project.stage : stages[0][0];
      const title = `<div class="pmw-project-hero"><div class="pmw-project-heading"><h2>${esc(project.name)}</h2><p>${esc(context.purpose)}</p></div><span class="pmw-stage-badge">${esc(stage)}</span></div>`;
      if (state.section === 'work') return ticketWorkspace(tickets);
      if (state.section === 'nautflow') return renderNautFlow(project);
      if (state.section === 'changes') return renderChanges(project);
      if (state.section === 'settings') return renderSettings(project);
      if (state.section === 'artifacts') {
        return `<div class="pmw-project-page">${title}<section class="pmw-project-empty"><h3>NAUT-Flow artifacts</h3><p>Stage documents are stored in the work Vault under Development/${esc(project.name)}/NAUT-Flow and remain available outside the project workspace.</p><button class="pmw-btn pmw-open-stage-artifacts" style="margin-top:14px">Open current document</button></section></div>`;
      }
      if (state.section === 'delivery') {
        return `<div class="pmw-project-page">${title}<section class="pmw-project-empty"><h3>Delivery has not started</h3><p>Build sessions, reviews, tests, releases, and Engram learning become available after the Plan gate is approved.</p></section></div>`;
      }
      const phases = flowPhases(project);
      const stageIndex = Math.max(0, stages.findIndex((item) => item[0] === stage));
      const currentStage = stages[stageIndex];
      const artifact = stageDocumentRef(project, currentStage, stageIndex);
      const planIndex = stages.findIndex((item) => item[1] === 'Plan' || item[0] === 'ticket');
      const ticketsReady = stageIndex >= planIndex;
      const controlConnected = Boolean(state.status?.remote_url);
      const sourceConnected = Boolean(project.source_repo);
      const owner = project.owner || 'Unassigned';
      return `<div class="pmw-project-page">${title}<div class="pmw-flow-rail">${phases.map((phase, index) => `<div class="pmw-flow-phase${phase[0] === currentStage[1] ? ' current' : ''}"><span class="pmw-flow-phase-label">0${index + 1} · ${esc(phase[0])}</span><span class="pmw-flow-phase-stages">${esc(phase[1])}</span></div>`).join('')}</div><div class="pmw-overview-layout"><main class="pmw-overview-main"><section class="pmw-overview-band"><div class="pmw-overview-band-head"><h3>Current stage · ${esc(currentStage[2])}</h3><span>Quality gate · 0/3</span></div><p>${esc(stageDescription(stage))}</p><div class="pmw-gate-list"><div class="pmw-gate-item"><span class="pmw-gate-box"></span><span>Required artifact is written</span></div><div class="pmw-gate-item"><span class="pmw-gate-box"></span><span>Independent review is complete</span></div><div class="pmw-gate-item"><span class="pmw-gate-box"></span><span>Stage is approved for promotion</span></div></div><button class="pmw-btn pmw-btn-primary pmw-open-nautflow" style="margin-top:14px">Open stage workspace</button></section>${renderActiveWork(tickets)}<section class="pmw-overview-band"><div class="pmw-overview-band-head"><h3>Primary artifact</h3><span>Work Vault</span></div><div class="pmw-artifact-row"><span class="pmw-artifact-icon">${ICON.doc}</span><div class="pmw-row-copy"><div class="pmw-row-title">${esc(currentStage[2])}</div><div class="pmw-row-meta">work:${esc(artifact)}</div></div><button class="pmw-btn pmw-open-overview-artifact">Open</button></div></section><section class="pmw-overview-band"><div class="pmw-overview-band-head"><h3>Contributors</h3><span>Stage ownership</span></div><div class="pmw-contributor-row"><span class="pmw-contributor-avatar">${esc(currentStage[3].slice(0, 2).toUpperCase())}</span><div class="pmw-row-copy"><div class="pmw-row-title">${esc(currentStage[3])}</div><div class="pmw-row-meta">Responsible Agent · ${esc(currentStage[2])}</div></div></div><div class="pmw-contributor-row"><span class="pmw-contributor-avatar">${esc(owner.slice(0, 2).toUpperCase())}</span><div class="pmw-row-copy"><div class="pmw-row-title">${esc(owner)}</div><div class="pmw-row-meta">Project owner</div></div></div></section></main><aside class="pmw-overview-rail"><section class="pmw-surface"><h3>Project health</h3><div class="pmw-metric-row"><div class="pmw-metric"><label>Budget</label><strong>${esc(money(context.budget))}</strong></div><div class="pmw-metric"><label>Tickets</label><strong>${tickets.length}</strong></div></div><div class="pmw-metric-row"><div class="pmw-metric"><label>Rate</label><strong>${context.rate == null ? 'Not set' : esc(money(context.rate))}</strong></div><div class="pmw-metric"><label>Flow</label><strong>${project.flow_type === 'incident' ? 'Incident' : 'Standard'}</strong></div></div></section><section class="pmw-surface"><h3>Connected systems</h3><div class="pmw-system-row"><span class="pmw-system-mark">VA</span><div class="pmw-row-copy"><div class="pmw-row-title">Work Vault</div><div class="pmw-row-meta">NAUT-Flow artifacts</div></div><span class="pmw-system-state">Connected</span></div><div class="pmw-system-row"><span class="pmw-system-mark">CR</span><div class="pmw-row-copy"><div class="pmw-row-title">Control repository</div><div class="pmw-row-meta">Projects and tickets</div></div><span class="pmw-system-state">${controlConnected ? 'Connected' : 'Local'}</span></div><div class="pmw-system-row"><span class="pmw-system-mark">SC</span><div class="pmw-row-copy"><div class="pmw-row-title">Source repository</div><div class="pmw-row-meta">${sourceConnected ? esc(project.source_repo) : 'Configure in Settings'}</div></div><span class="pmw-system-state">${sourceConnected ? 'Linked' : 'Open'}</span></div><div class="pmw-system-row"><span class="pmw-system-mark">EN</span><div class="pmw-row-copy"><div class="pmw-row-title">Engram</div><div class="pmw-row-meta">Release learning and anti-patterns</div></div><span class="pmw-system-state">At release</span></div></section><section class="pmw-surface"><h3>Ticket readiness</h3><div class="pmw-readiness"><span style="width:${ticketsReady ? '100' : '0'}%"></span></div><div class="pmw-ticket-lock" style="margin-top:10px">${ticketsReady ? `${tickets.length} project ticket${tickets.length === 1 ? '' : 's'} available for execution.` : `Tickets unlock when ${esc(stages[Math.max(planIndex, 0)]?.[2] || 'planning')} is reached and approved.`}</div></section></aside></div></div>`;
    }

    async function writeStageDocument(rel, content) {
      try {
        await invoke('vault_note_write', { vault: 'work', rel, content });
      } catch (error) {
        if (!String(error).includes('vault not open')) throw error;
        await invoke('vault_open', { vault: 'work' });
        await invoke('vault_note_write', { vault: 'work', rel, content });
      }
    }

    async function readStageDocument(rel) {
      try {
        return await invoke('vault_note_read', { vault: 'work', rel });
      } catch (error) {
        if (!String(error).includes('vault not open')) throw error;
        await invoke('vault_open', { vault: 'work' });
        return invoke('vault_note_read', { vault: 'work', rel });
      }
    }

    function openAgentForStage(project, stage, rel, review) {
      const draft = $('.pmw-stage-editor')?.value || '';
      const role = review ? 'Reviewer' : stage[3];
      const task = review
        ? `Independently review the ${stage[2]} artifact for completeness, contradictions, risks, missing evidence, and stage readiness. Do not edit it. Return findings ordered by severity and a clear approve or changes-required verdict.`
        : `Help me create and improve the ${stage[2]} artifact. Read work:${rel}, discuss missing decisions with me, then use vault_write on ${rel} when I approve a revision.`;
      const opts = {
        title: `${role} · ${project.key} · ${stage[2]}`,
        chatKeyBase: `nautflow:${project.key}:${stage[0]}:${review ? 'review' : 'work'}`,
        preferredAgentRole: role,
        systemPromptAppend: `You are the ${role} for xNAUT project ${project.name}. The current NAUT-Flow stage is ${stage[2]}. The authoritative artifact is in the work Vault at relative path ${rel}. Vault tool rel/from/to values must be relative paths such as "${rel}"; never include a "work:" prefix. Keep the project purpose and stage gate in scope.`,
        prefill: `${task}\n\nCurrent draft:\n\n${draft}`,
        autoSend: true,
        vaultTools: { vault: () => 'work', entry: null },
      };
      if (typeof window.xnautShowRightPane === 'function') window.xnautShowRightPane();
      if (typeof window.xnautRightPaneOpenChat === 'function') window.xnautRightPaneOpenChat(opts);
      else if (typeof window.xnautAttachChatTab === 'function') window.xnautAttachChatTab(opts);
      else toast('Chat workspace is unavailable', true);
    }

    function openPromotionAgent(project, sourceStage, targetStage, sourceRel, targetRel) {
      const role = targetStage[3];
      const opts = {
        title: `${role} · ${project.key} · ${targetStage[2]}`,
        chatKeyBase: `nautflow:${project.key}:${targetStage[0]}:promotion`,
        preferredAgentRole: role,
        systemPromptAppend: `You are the ${role} for xNAUT project ${project.name}. ${sourceStage[2]} was promoted into ${targetStage[2]}. Both documents are in the work Vault. The approved source path is ${sourceRel}; do not modify it. The target path is ${targetRel}. Vault tool rel/from/to values must use these exact relative paths without a "work:" prefix. Preserve the source reference and make all new decisions in the target artifact.`,
        prefill: `Validate ${sourceRel} as input for the ${targetStage[2]} stage. Read the promoted source and ${targetRel} from the work Vault. Identify missing evidence, contradictions, risks, and questions before drafting. Discuss material gaps with me, then update only ${targetRel} when I approve.`,
        autoSend: true,
        vaultTools: { vault: () => 'work', entry: null },
      };
      if (typeof window.xnautShowRightPane === 'function') window.xnautShowRightPane();
      if (typeof window.xnautRightPaneOpenChat === 'function') window.xnautRightPaneOpenChat(opts);
      else if (typeof window.xnautAttachChatTab === 'function') window.xnautAttachChatTab(opts);
      else toast('Chat workspace is unavailable', true);
    }

    function bindNautFlow(project) {
      const stages = stagesFor(project);
      $('.pmw-content').querySelectorAll('[data-flow-stage]').forEach((button) => {
        button.onclick = () => { state.flowStage = button.dataset.flowStage; renderContent(); };
      });
      const selectedIndex = Math.max(0, stages.findIndex((stage) => stage[0] === state.flowStage));
      const stage = stages[selectedIndex];
      const baseRel = stageDocumentRef(project, stage, selectedIndex);
      let currentVersion = 1;
      let currentRel = baseRel;
      let versionDocuments = new Map([[1, baseRel]]);
      let documentRequest = 0;
      const editor = $('.pmw-stage-editor');
      const preview = $('.pmw-stage-preview');
      const previewToggle = $('.pmw-stage-preview-toggle');
      const versionCreate = $('.pmw-stage-new-version');
      const fileList = $('.pmw-stage-files');
      const ref = $('.pmw-stage-ref');
      let previewActive = false;
      const publishAgentContext = () => window.xnautSetAgentWorkspaceContext?.({
        owner: label,
        project: `${project.key} · ${project.name}`,
        stage: stage[2],
        vault: 'work',
        rel: currentRel,
        content: editor?.value || '',
        onWrite: (rel, content) => {
          if (rel !== currentRel || !editor?.isConnected) return;
          editor.value = content;
          if (previewActive) paintPreview();
          publishAgentContext();
        },
        isActive: () => pane.isConnected && pane.getClientRects().length > 0 && state.section === 'nautflow',
      });
      const paintPreview = () => {
        if (window.xnautMarkdown?.renderInto) window.xnautMarkdown.renderInto(preview, editor.value || '_Empty document._');
        else preview.textContent = editor.value || 'Empty document.';
      };
      previewToggle.onclick = () => {
        previewActive = !previewActive;
        if (previewActive) paintPreview();
        editor.hidden = previewActive;
        preview.hidden = !previewActive;
        previewToggle.dataset.active = previewActive ? '1' : '0';
        previewToggle.innerHTML = previewActive ? ICON.pencil : ICON.eye;
        previewToggle.title = previewActive ? 'Edit document' : 'Preview document';
        previewToggle.setAttribute('aria-label', previewToggle.title);
        if (!previewActive) editor.focus();
      };
      const loadVersion = async (version) => {
        const request = ++documentRequest;
        currentVersion = Number(version) || 1;
        currentRel = versionDocuments.get(currentVersion) || stageVersionRef(baseRel, currentVersion);
        ref.textContent = `work:${currentRel}`;
        let content;
        try { content = await readStageDocument(currentRel); }
        catch (_) { content = currentVersion === 1 ? stageTemplate(project, stage) : ''; }
        if (request !== documentRequest || state.section !== 'nautflow' || state.flowStage !== stage[0] || !editor?.isConnected) return;
        editor.value = content;
        publishAgentContext();
        fileList.querySelectorAll('[data-stage-version]').forEach((button) => button.classList.toggle('active', Number(button.dataset.stageVersion) === currentVersion));
        if (previewActive) paintPreview();
      };
      editor.addEventListener('input', publishAgentContext);
      const refreshVersions = async (selectedVersion) => {
        const documents = await stageVersionDocuments(baseRel);
        const versions = documents.map((item) => item.version);
        versionDocuments = new Map(documents.map((item) => [item.version, item.rel]));
        currentVersion = versions.includes(Number(selectedVersion)) ? Number(selectedVersion) : (versions[0] || 1);
        if (!versionDocuments.size) versionDocuments.set(1, baseRel);
        fileList.innerHTML = documents.length ? documents.map((item) => {
          const filename = item.rel.split('/').pop() || item.rel;
          return `<button class="pmw-stage-file${item.version === currentVersion ? ' active' : ''}" data-stage-version="${item.version}" title="${esc(item.rel)}">${ICON.doc}<span class="pmw-stage-file-copy"><span class="pmw-stage-file-title">${esc(stage[2])} V${item.version}</span><span class="pmw-stage-file-name">${esc(filename)}</span></span></button>`;
        }).join('') : '<div class="pmw-stage-file-empty">No documents yet. Save the draft or create the first version.</div>';
        fileList.querySelectorAll('[data-stage-version]').forEach((button) => { button.onclick = () => loadVersion(button.dataset.stageVersion); });
        await loadVersion(currentVersion);
      };
      versionCreate.onclick = async () => {
        versionCreate.disabled = true;
        try {
          const versions = (await stageVersionDocuments(baseRel)).map((item) => item.version);
          const next = versions.length ? Math.max(...versions) + 1 : 1;
          const nextRel = stageVersionRef(baseRel, next);
          await writeStageDocument(nextRel, editor.value);
          await refreshVersions(next);
          toast(`${stage[2]} V${next} created`);
        } catch (error) { toast(error, true); }
        finally { versionCreate.disabled = false; }
      };
      refreshVersions(1).catch((error) => toast(error, true));
      $('.pmw-stage-save').onclick = async (event) => {
        const button = event.currentTarget;
        button.disabled = true; button.textContent = 'Saving...';
        try { await writeStageDocument(currentRel, editor.value); await refreshVersions(currentVersion); toast(`${stage[2]} V${currentVersion} saved to Vault`); }
        catch (error) { toast(error, true); }
        finally { button.disabled = false; button.textContent = 'Save document'; }
      };
      $('.pmw-stage-open').onclick = () => openDocument(`work:${currentRel}`);
      $('.pmw-ask-agent').onclick = async () => {
        try { await writeStageDocument(currentRel, editor.value); } catch (error) { toast(error, true); return; }
        openAgentForStage(project, stage, currentRel, false);
      };
      $('.pmw-request-review').onclick = async () => {
        try { await writeStageDocument(currentRel, editor.value); } catch (error) { toast(error, true); return; }
        openAgentForStage(project, stage, currentRel, true);
      };
      const promote = $('.pmw-promote-stage');
      if (promote) promote.onclick = async () => {
        const targetStage = stages[selectedIndex + 1];
        const targetIndex = selectedIndex + 1;
        const targetBaseRel = stageDocumentRef(project, targetStage, targetIndex);
        promote.disabled = true;
        promote.textContent = 'Promoting...';
        try {
          await writeStageDocument(currentRel, editor.value);
          const targets = await stageVersionDocuments(targetBaseRel);
          const targetRel = targets[0]?.rel || targetBaseRel;
          if (!targets.length) await writeStageDocument(targetRel, promotedStageTemplate(project, stage, targetStage, currentRel));
          const updated = await invoke('pm_project_update', { request: projectUpdatePayload(project, targetStage[0]) });
          const index = state.projects.findIndex((item) => item.key === updated.key);
          if (index >= 0) state.projects[index] = updated;
          state.flowStage = targetStage[0];
          renderProjectFilters();
          renderContent();
          openPromotionAgent(updated, stage, targetStage, currentRel, targetRel);
          toast(`${stage[2]} promoted to ${targetStage[2]}`);
        } catch (error) {
          toast(error, true);
          if (promote.isConnected) { promote.disabled = false; promote.textContent = `Promote to ${targetStage[2]}`; }
        }
      };
    }

    function bindSettings(project) {
      const form = $('.pmw-settings-form');
      if (!form) return;
      form.onsubmit = async (event) => {
        event.preventDefault();
        if (!form.reportValidity()) return;
        const button = $('.pmw-settings-save');
        const status = $('.pmw-settings-state');
        const numberValue = (selector) => { const value = $(selector).value; return value === '' ? null : Number(value); };
        button.disabled = true; button.textContent = 'Saving...'; status.textContent = '';
        try {
          const updated = await invoke('pm_project_update', { request: { key: project.key, expected_revision: project.revision || 1, name: $('.pmw-settings-name').value, purpose: $('.pmw-settings-purpose').value, owner: $('.pmw-settings-owner').value, client_name: $('.pmw-settings-client').value, contact_name: $('.pmw-settings-contact').value, contact_email: $('.pmw-settings-email').value, budget_chf: numberValue('.pmw-settings-budget'), hourly_rate_chf: numberValue('.pmw-settings-rate'), flow_type: $('.pmw-settings-flow').value, source_repo: $('.pmw-settings-source').value } });
          const index = state.projects.findIndex((item) => item.key === updated.key);
          if (index >= 0) state.projects[index] = updated;
          status.textContent = 'Saved';
          renderProjectFilters(); renderContent();
          toast('Project settings saved');
        } catch (error) { status.textContent = String(error); toast(error, true); button.disabled = false; button.textContent = 'Save settings'; }
      };
    }

    function bindProjectSection(project) {
      if (state.section === 'nautflow') bindNautFlow(project);
      if (state.section === 'changes') bindChanges(project);
      if (state.section === 'settings') bindSettings(project);
      pane.querySelectorAll('[data-overview-ticket]').forEach((row) => {
        const open = () => openTicket(row.dataset.overviewTicket);
        row.onclick = open;
        row.onkeydown = (event) => {
          if (event.key === 'Enter' || event.key === ' ') {
            event.preventDefault();
            open();
          }
        };
      });
      const openFlow = $('.pmw-open-nautflow');
      if (openFlow) openFlow.onclick = () => { state.section = 'nautflow'; state.flowStage = project.stage || ''; renderContent(); };
      const overviewArtifact = $('.pmw-open-overview-artifact');
      if (overviewArtifact) {
        const stages = stagesFor(project);
        const index = Math.max(0, stages.findIndex((stage) => stage[0] === project.stage));
        overviewArtifact.onclick = () => openDocument(`work:${stageDocumentRef(project, stages[index], index)}`);
      }
      const openArtifacts = $('.pmw-open-stage-artifacts');
      if (openArtifacts) {
        const stages = stagesFor(project);
        const index = Math.max(0, stages.findIndex((stage) => stage[0] === project.stage));
        openArtifacts.onclick = () => openDocument(`work:${stageDocumentRef(project, stages[index], index)}`);
      }
    }

    function renderContent() {
      const tickets = visibleTickets();
      const project = state.projects.find((item) => item.key === state.project);
      const projectWork = Boolean(project && state.section === 'work');
      if (!project || state.section !== 'nautflow') window.xnautClearAgentWorkspaceContext?.(label);
      $('.pmw-view-switch').hidden = Boolean(project && !projectWork);
      $('.pmw-filter').hidden = Boolean(project && !projectWork);
      if (!state.projects.length) {
        $('.pmw-content').innerHTML = '<div class="pmw-empty"><strong>No projects yet.</strong><br>Create the first project to start the NAUT-Flow lifecycle.</div>';
        return;
      }
      if (project) {
        $('.pmw-content').innerHTML = `<div class="pmw-project-shell">${projectTabs(state.section)}${renderProjectSection(project, tickets)}</div>`;
        bindProjectTabs();
        bindProjectSection(project);
      } else {
        $('.pmw-content').innerHTML = ticketWorkspace(tickets);
      }
      if (!project || projectWork) bindTickets();
    }

    function selectProject(key) {
      state.project = key;
      state.section = key ? 'overview' : 'work';
      state.flowStage = '';
      state.selected = null;
      state.selectedChange = '';
      renderDetail();
      $('.pmw-project-select').value = key;
      renderProjectFilters();
      renderContent();
    }

    async function openTicket(id) {
      state.selected = state.tickets.find((item) => item.id === id) || null;
      if (!state.selected) return;
      renderContent();
      renderDetail();
      const selectedId = id;
      try {
        state.events = await invoke('pm_event_list', { subject: id, limit: 100 });
        if (state.selected && state.selected.id === selectedId) renderEvents();
      } catch (error) { toast(error, true); }
    }

    function renderDetail() {
      const ticket = state.selected;
      const detail = $('.pmw-detail');
      if (!ticket) { detail.hidden = true; detail.innerHTML = ''; return; }
      detail.hidden = false;
      detail.innerHTML = `<header class="pmw-detail-head"><span class="pmw-detail-id">${esc(ticket.id)} · revision ${ticket.revision}</span><span class="pmw-spacer"></span><button class="pmw-icon pmw-detail-close" title="Close">${ICON.close}</button></header><div class="pmw-detail-body"><div class="pmw-field"><label>Title</label><input class="pmw-input pmw-edit-title" value="${esc(ticket.title)}"></div><div class="pmw-field-grid"><div class="pmw-field"><label>Type</label><select class="pmw-select pmw-edit-type">${TYPES.map((value) => `<option${ticket.ticket_type === value ? ' selected' : ''}>${value}</option>`).join('')}</select></div><div class="pmw-field"><label>Priority</label><select class="pmw-select pmw-edit-priority">${PRIORITIES.map((value) => `<option${ticket.priority === value ? ' selected' : ''}>${value}</option>`).join('')}</select></div><div class="pmw-field"><label>Status</label><select class="pmw-select pmw-edit-status">${STATUSES.map((value) => `<option value="${value}"${ticket.status === value ? ' selected' : ''}>${LABELS[value]}</option>`).join('')}</select></div></div><div class="pmw-field"><label>Owner</label><input class="pmw-input pmw-edit-owner" value="${esc(ticket.owner || '')}" placeholder="Unassigned"></div><div class="pmw-field"><label>Description</label><textarea class="pmw-textarea pmw-edit-body">${esc(ticket.body)}</textarea></div><div class="pmw-field"><label>Vault documents (one reference per line)</label><textarea class="pmw-textarea pmw-docs pmw-edit-docs" placeholder="work:Development/project/document.md">${esc((ticket.documentation || []).join('\n'))}</textarea><div class="pmw-doc-links"></div></div><section class="pmw-activity"><div class="pmw-section-title">Activity</div><div class="pmw-events"><span class="pmw-event-time">Loading...</span></div></section></div><footer class="pmw-detail-actions"><button class="pmw-btn pmw-btn-danger pmw-delete">Delete</button><span class="pmw-spacer"></span><button class="pmw-btn pmw-save">Save changes</button><button class="pmw-btn pmw-btn-primary pmw-save-close">Save and close</button></footer>`;
      detail.querySelector('.pmw-detail-close').onclick = () => { state.selected = null; renderDetail(); renderContent(); };
      detail.querySelector('.pmw-save').onclick = () => saveDetail(false);
      detail.querySelector('.pmw-save-close').onclick = () => saveDetail(true);
      bindDelete(detail.querySelector('.pmw-delete'));
      renderDocLinks();
    }

    function renderDocLinks() {
      const host = $('.pmw-doc-links');
      if (!host || !state.selected) return;
      host.innerHTML = (state.selected.documentation || []).map((ref, index) => `<button class="pmw-btn" data-doc="${index}">${ICON.doc} ${esc(ref)}</button>`).join(' ');
      host.querySelectorAll('[data-doc]').forEach((button) => { button.onclick = () => openDocument(state.selected.documentation[Number(button.dataset.doc)]); });
    }

    function openDocument(reference) {
      let vault = 'work';
      let rel = String(reference || '').trim();
      if (rel.includes(':') && !rel.startsWith('/')) {
        const split = rel.split(':');
        if (split[0] === 'work' || split[0] === 'personal') { vault = split.shift(); rel = split.join(':'); }
      }
      if (!rel) return;
      if (window.xnautAttachVaultTab) window.xnautAttachVaultTab({ vault, openRel: rel.replace(/^\/+/, '') });
    }

    function renderEvents() {
      const host = $('.pmw-events');
      if (!host) return;
      host.innerHTML = state.events.length ? state.events.map((event) => `<div class="pmw-event"><span class="pmw-event-dot"></span><div><div class="pmw-event-name">${esc(event.event.replace(/\./g, ' '))}</div><div class="pmw-event-time">${esc(relativeTime(event.timestamp))}</div></div></div>`).join('') : '<span class="pmw-event-time">No activity recorded.</span>';
    }

    function detailPatch() {
      const owner = $('.pmw-edit-owner').value.trim();
      return {
        title: $('.pmw-edit-title').value.trim(),
        ticket_type: $('.pmw-edit-type').value,
        status: $('.pmw-edit-status').value,
        priority: $('.pmw-edit-priority').value,
        owner: owner || null,
        clear_owner: !owner,
        documentation: $('.pmw-edit-docs').value.split('\n').map((value) => value.trim()).filter(Boolean),
        body: $('.pmw-edit-body').value,
      };
    }

    async function updateTicket(ticket, patch) {
      try {
        const updated = await invoke('pm_ticket_update', { request: { id: ticket.id, expected_revision: ticket.revision, ...patch } });
        const index = state.tickets.findIndex((item) => item.id === updated.id);
        if (index >= 0) state.tickets[index] = updated;
        if (state.selected && state.selected.id === updated.id) state.selected = updated;
        renderProjectFilters(); renderContent(); if (state.selected) renderDetail();
        return updated;
      } catch (error) { toast(error, true); await load(); return null; }
    }

    async function saveDetail(close) {
      if (!state.selected) return;
      const updated = await updateTicket(state.selected, detailPatch());
      if (updated && close) { state.selected = null; renderDetail(); renderContent(); }
      else if (updated) { state.events = await invoke('pm_event_list', { subject: updated.id, limit: 100 }); renderEvents(); toast('Ticket saved'); }
    }

    function bindDelete(button) {
      let armed = false;
      button.onclick = async () => {
        if (!state.selected) return;
        if (!armed) { armed = true; button.textContent = 'Click again to delete'; setTimeout(() => { armed = false; if (button.isConnected) button.textContent = 'Delete'; }, 3000); return; }
        try {
          await invoke('pm_ticket_delete', { id: state.selected.id, expectedRevision: state.selected.revision });
          state.selected = null; await load(); toast('Ticket deleted');
        } catch (error) { toast(error, true); }
      };
    }

    function showProjectCreate() {
      const overlay = $('.pmw-overlay');
      overlay.hidden = false;
      overlay.innerHTML = `<form class="pmw-create-page"><header class="pmw-create-head"><div><h2>New project</h2><p>Start a project at the Idea stage and carry it through NAUT-Flow.</p></div><span class="pmw-spacer"></span><button type="button" class="pmw-icon pmw-dialog-close" aria-label="Close">${ICON.close}</button></header><div class="pmw-create-body"><section class="pmw-create-section"><h3>Basics</h3><div class="pmw-create-grid"><div class="pmw-field"><label>Name</label><input class="pmw-input pmw-new-project-name" placeholder="Project name" required></div><div class="pmw-field"><label>Project key</label><input class="pmw-input pmw-new-key" maxlength="12" pattern="[A-Za-z0-9]{2,12}" placeholder="PROJECT" required><span class="pmw-help">Used for ticket IDs, for example XNAUT-42. 2-12 letters or numbers.</span></div></div><div class="pmw-field"><label>Purpose</label><textarea class="pmw-textarea pmw-new-purpose" placeholder="What problem does this project solve, for whom, and what outcome should it achieve?" required></textarea></div></section><section class="pmw-create-section"><h3>NAUT-Flow</h3><div class="pmw-flow-choice"><label><input type="radio" name="pmw-flow-type" value="standard" checked><span><strong>Standard project</strong><span>Idea, concept, definition, architecture, planning, delivery, and learning.</span></span></label><label><input type="radio" name="pmw-flow-type" value="incident"><span><strong>Incident fast track</strong><span>Intake, root-cause analysis, action plan, implementation, verification, and learning.</span></span></label></div></section><section class="pmw-create-section"><h3>Ownership</h3><div class="pmw-create-grid pmw-create-grid-3"><div class="pmw-field"><label>Project owner</label><input class="pmw-input pmw-new-owner" placeholder="Owner"></div><div class="pmw-field"><label>Client</label><input class="pmw-input pmw-new-client" placeholder="Internal or company"></div><div class="pmw-field"><label>Primary contact</label><input class="pmw-input pmw-new-contact" placeholder="Contact name"></div></div><div class="pmw-field"><label>Contact email</label><input class="pmw-input pmw-new-contact-email" type="email" placeholder="name@example.com"></div></section><section class="pmw-create-section"><h3>Repository and commercial baseline</h3><div class="pmw-field"><label>Source repository or local folder</label><input class="pmw-input pmw-new-source" placeholder="/path/to/project or ssh://git@forge/team/project.git"><span class="pmw-help">Optional during discovery. The control repository already stores the project record.</span></div><div class="pmw-create-grid"><div class="pmw-field"><label>Budget (CHF)</label><input class="pmw-input pmw-new-budget" type="number" min="0" step="1" placeholder="Optional"></div><div class="pmw-field"><label>Hourly rate (CHF)</label><input class="pmw-input pmw-new-rate" type="number" min="0" step="0.01" placeholder="Optional"></div></div></section></div><footer class="pmw-create-actions"><span class="pmw-help">The project opens at Idea. Tickets become executable work during Plan.</span><span class="pmw-spacer"></span><button type="button" class="pmw-btn pmw-dialog-cancel">Cancel</button><button type="submit" class="pmw-btn pmw-btn-primary pmw-dialog-submit">Create project</button></footer></form>`;
      const close = () => { overlay.hidden = true; overlay.innerHTML = ''; };
      overlay.querySelector('.pmw-dialog-close').onclick = close;
      overlay.querySelector('.pmw-dialog-cancel').onclick = close;
      overlay.onclick = (event) => { if (event.target === overlay) close(); };
      const form = overlay.querySelector('form');
      const name = overlay.querySelector('.pmw-new-project-name');
      const key = overlay.querySelector('.pmw-new-key');
      let keyEdited = false;
      name.oninput = () => { if (!keyEdited) key.value = projectKeySeed(name.value); };
      key.oninput = () => { keyEdited = true; key.value = key.value.replace(/[^a-z0-9]/gi, '').toUpperCase().slice(0, 12); };
      form.onsubmit = async (event) => {
        event.preventDefault();
        if (!form.reportValidity()) return;
        const button = overlay.querySelector('.pmw-dialog-submit');
        button.disabled = true;
        button.textContent = 'Creating...';
        try {
          const numberValue = (selector) => { const value = overlay.querySelector(selector).value; return value === '' ? null : Number(value); };
          const project = await invoke('pm_project_create', { request: { key: key.value, name: name.value, purpose: overlay.querySelector('.pmw-new-purpose').value, owner: overlay.querySelector('.pmw-new-owner').value, client_name: overlay.querySelector('.pmw-new-client').value, contact_name: overlay.querySelector('.pmw-new-contact').value, contact_email: overlay.querySelector('.pmw-new-contact-email').value, budget_chf: numberValue('.pmw-new-budget'), hourly_rate_chf: numberValue('.pmw-new-rate'), flow_type: overlay.querySelector('[name="pmw-flow-type"]:checked').value, source_repo: overlay.querySelector('.pmw-new-source').value } });
          state.project = project.key;
          state.section = 'overview';
          close();
          await load();
          toast(`${project.name} created`);
        } catch (error) {
          toast(error, true);
          button.disabled = false;
          button.textContent = 'Create project';
        }
      };
      setTimeout(() => name.focus(), 0);
    }

    function showDialog(kind) {
      if (kind === 'project') { showProjectCreate(); return; }
      const overlay = $('.pmw-overlay');
      const projectOptions = state.projects.map((project) => `<option value="${esc(project.key)}"${state.project === project.key ? ' selected' : ''}>${esc(project.key)} - ${esc(project.name)}</option>`).join('');
      overlay.hidden = false;
      overlay.innerHTML = `<div class="pmw-dialog"><div class="pmw-dialog-head"><span class="pmw-dialog-title">New ticket</span><span class="pmw-spacer"></span><button class="pmw-icon pmw-dialog-close">${ICON.close}</button></div><div class="pmw-field"><label>Project</label><select class="pmw-select pmw-new-ticket-project">${projectOptions}</select></div><div class="pmw-field"><label>Title</label><input class="pmw-input pmw-new-ticket-title" placeholder="Describe the outcome"></div><div class="pmw-field-grid"><div class="pmw-field"><label>Type</label><select class="pmw-select pmw-new-type">${TYPES.map((value) => `<option${value === 'task' ? ' selected' : ''}>${value}</option>`).join('')}</select></div><div class="pmw-field"><label>Priority</label><select class="pmw-select pmw-new-priority">${PRIORITIES.map((value) => `<option${value === 'medium' ? ' selected' : ''}>${value}</option>`).join('')}</select></div><div class="pmw-field"><label>Status</label><select class="pmw-select pmw-new-status">${STATUSES.map((value) => `<option value="${value}">${LABELS[value]}</option>`).join('')}</select></div></div><div class="pmw-field"><label>Owner</label><input class="pmw-input pmw-new-owner" placeholder="Unassigned"></div><div class="pmw-field"><label>Description</label><textarea class="pmw-textarea pmw-new-body"></textarea></div><div class="pmw-field"><label>Vault documents</label><textarea class="pmw-textarea pmw-docs pmw-new-docs" placeholder="work:Development/project/document.md"></textarea></div><div class="pmw-dialog-actions"><button class="pmw-btn pmw-dialog-cancel">Cancel</button><button class="pmw-btn pmw-btn-primary pmw-dialog-submit">Create ticket</button></div></div>`;
      const close = () => { overlay.hidden = true; overlay.innerHTML = ''; };
      overlay.querySelector('.pmw-dialog-close').onclick = close;
      overlay.querySelector('.pmw-dialog-cancel').onclick = close;
      overlay.onclick = (event) => { if (event.target === overlay) close(); };
      overlay.querySelector('.pmw-dialog-submit').onclick = async () => {
        try {
          const ticket = await invoke('pm_ticket_create', { request: { project: overlay.querySelector('.pmw-new-ticket-project').value, title: overlay.querySelector('.pmw-new-ticket-title').value, ticket_type: overlay.querySelector('.pmw-new-type').value, status: overlay.querySelector('.pmw-new-status').value, priority: overlay.querySelector('.pmw-new-priority').value, owner: overlay.querySelector('.pmw-new-owner').value.trim() || null, body: overlay.querySelector('.pmw-new-body').value, documentation: overlay.querySelector('.pmw-new-docs').value.split('\n').map((value) => value.trim()).filter(Boolean) } });
          state.selected = ticket;
          state.project = ticket.project;
          state.section = 'work';
          close(); await load(); openTicket(ticket.id);
        } catch (error) { toast(error, true); }
      };
      setTimeout(() => overlay.querySelector('input,select,textarea')?.focus(), 0);
    }

    function showProjectDetails() {
      const project = state.projects.find((item) => item.key === state.project);
      if (!project) return;
      const context = projectContext(project);
      const overlay = $('.pmw-overlay');
      overlay.hidden = false;
      overlay.innerHTML = `<div class="pmw-dialog"><div class="pmw-dialog-head"><span class="pmw-dialog-title">${esc(project.key)} - ${esc(project.name)}</span><span class="pmw-spacer"></span><button class="pmw-icon pmw-dialog-close">${ICON.close}</button></div><div class="pmw-field"><label>Purpose</label><div>${esc(context.purpose)}</div></div><div class="pmw-field-grid"><div class="pmw-field"><label>Stage</label><div>${esc(project.stage || 'idea')}</div></div><div class="pmw-field"><label>Flow</label><div>${esc(project.flow_type || 'standard')}</div></div><div class="pmw-field"><label>Owner</label><div>${esc(project.owner || 'Unassigned')}</div></div></div><div class="pmw-field"><label>Source</label><div>${esc(project.source_path || project.forge_remote || project.source_repo || 'Not linked')}</div></div><div class="pmw-field-grid"><div class="pmw-field"><label>Client</label><div>${esc(context.client || 'Internal')}</div></div><div class="pmw-field"><label>Budget</label><div>${esc(money(context.budget))}</div></div><div class="pmw-field"><label>Rate</label><div>${context.rate == null ? 'Not set' : `${esc(money(context.rate))} / hour`}</div></div></div><div class="pmw-dialog-actions"><button class="pmw-btn pmw-dialog-close-action">Close</button></div></div>`;
      const close = () => { overlay.hidden = true; overlay.innerHTML = ''; };
      overlay.querySelector('.pmw-dialog-close').onclick = close;
      overlay.querySelector('.pmw-dialog-close-action').onclick = close;
      overlay.onclick = (event) => { if (event.target === overlay) close(); };
    }

    function paintStatus() {
      const status = state.status;
      if (!status) return;
      const parts = [];
      if (status.branch) parts.push(status.branch);
      if (status.ahead) parts.push(`${status.ahead} ahead`);
      if (status.behind) parts.push(`${status.behind} behind`);
      if (status.dirty) parts.push('local changes');
      if (status.last_commit) parts.push(status.last_commit);
      $('.pmw-sync-state').textContent = parts.join(' · ') || (status.remote_url ? 'Connected' : 'Local only');
      $('.pmw-sync').disabled = !status.remote_url;
    }

    async function load(importExisting = true) {
      const request = ++state.request;
      try {
        const projects = await invoke(importExisting ? 'pm_project_import_existing' : 'pm_project_list');
        const tickets = await invoke('pm_ticket_list', { project: null });
        const changes = (await Promise.all((projects || []).map((project) => invoke('pm_change_list', { project: project.key }).catch(() => [])))).flat();
        const status = await invoke('pm_module_status');
        if (request !== state.request) return;
        state.status = status; state.projects = projects || []; state.tickets = tickets || []; state.changes = changes || [];
        if (state.project && !state.projects.some((project) => project.key === state.project)) state.project = '';
        if (state.selected) state.selected = state.tickets.find((ticket) => ticket.id === state.selected.id) || null;
        paintStatus(); renderProjectFilters(); renderContent(); renderDetail();
      } catch (error) {
        $('.pmw-content').innerHTML = `<div class="pmw-empty pmw-error">${esc(error)}</div>`;
      }
    }

    $('.pmw-project-select').onchange = (event) => selectProject(event.target.value);
    $('.pmw-filter').oninput = renderContent;
    $('.pmw-segment').querySelectorAll('[data-view]').forEach((button) => { button.onclick = () => { state.view = button.dataset.view; $('.pmw-segment').querySelectorAll('button').forEach((node) => node.classList.toggle('active', node === button)); renderContent(); }; });
    $('.pmw-refresh').onclick = () => load();
    $('.pmw-sync').onclick = async (event) => { const button = event.currentTarget; button.disabled = true; $('.pmw-sync-state').textContent = 'Synchronizing...'; try { state.status = await invoke('pm_module_sync'); await load(); toast('Control repository synchronized'); } catch (error) { toast(error, true); paintStatus(); } finally { button.disabled = false; } };
    $('.pmw-new-project').onclick = () => showDialog('project');
    $('.pmw-new-ticket').onclick = () => state.projects.length ? showDialog('ticket') : showDialog('project');
    $('.pmw-project-details').onclick = showProjectDetails;

    const refreshWhenVisible = () => {
      if (!document.hidden && pane.isConnected) load(false);
    };
    const refreshTimer = window.setInterval(refreshWhenVisible, 15000);
    window.addEventListener('focus', refreshWhenVisible);
    document.addEventListener('visibilitychange', refreshWhenVisible);
    load();
    const entry = {
      kind: 'project-management',
      label,
      pane,
      refresh: load,
      dispose: () => {
        window.clearInterval(refreshTimer);
        window.removeEventListener('focus', refreshWhenVisible);
        document.removeEventListener('visibilitychange', refreshWhenVisible);
      },
    };
    panes.set(label, entry);
    return entry;
  }

  function destroyPanel(label) {
    const entry = panes.get(label);
    if (!entry) return;
    window.xnautClearAgentWorkspaceContext?.(label);
    entry.dispose?.();
    entry.pane.remove();
    panes.delete(label);
  }

  window.xnautCreateProjectManagementPanel = createPanel;
  window.xnautDestroyProjectManagementPanel = destroyPanel;
})();
