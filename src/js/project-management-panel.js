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
    save: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4"><path d="M2.5 2.5h8.4l2.6 2.6v8.4h-11z"/><path d="M5 2.5v3.6h5V2.5"/><rect x="5" y="9" width="6" height="4.5"/></svg>',
    open: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4"><path d="M9 2.5h4.5V7"/><path d="M13.5 2.5 7.3 8.7"/><path d="M11.5 9.5V13H3V4.5h3.5"/></svg>',
    load: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4"><path d="M8 2.5v6.6"/><path d="m5 6 3 3 3-3"/><path d="M3 12.5h10"/></svg>',
    kebab: '<svg viewBox="0 0 16 16" fill="currentColor"><circle cx="8" cy="3.2" r="1.3"/><circle cx="8" cy="8" r="1.3"/><circle cx="8" cy="12.8" r="1.3"/></svg>',
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
.pmw-project-key { width:46px; flex:0 0 auto; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; color:var(--text-muted,#737985); font-size:10px; font-weight:700; }
.pmw-project-name { flex:1 1 auto; min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.pmw-count { flex:0 0 auto; color:var(--text-muted,#737985); font-size:11px; }
.pmw-focus { flex:0 0 auto; padding:2px 8px; border:1px solid var(--border-color,#3a3d45); border-radius:5px; background:transparent; color:var(--text-secondary,#9297a1); font:inherit; font-size:10px; font-weight:700; text-transform:uppercase; letter-spacing:.02em; cursor:pointer; }
.pmw-focus:hover { color:var(--text-primary,#fff); border-color:var(--xnaut-yellow,#f5b840); }
.pmw-focus.active { background:var(--xnaut-yellow,#f5b840); border-color:var(--xnaut-yellow,#f5b840); color:#1a1400; }
.pmw-projects.focused .pmw-project:not(.active) { display:none; }
.pmw-rail-toggle{width:22px;height:22px;border:0;border-radius:5px;background:transparent;color:var(--text-muted,#7f8590);cursor:pointer;font-size:14px;line-height:1;flex:0 0 auto}
.pmw-rail-toggle:hover{background:var(--hover-bg,rgba(255,255,255,.06));color:var(--text-primary,#fff)}
.pmw-project-mono{display:none}
.pmw-rail-collapsed{flex-basis:58px!important;min-width:58px!important}
.pmw-rail-collapsed .pmw-rail-title,.pmw-rail-collapsed .pmw-project-key,.pmw-rail-collapsed .pmw-project-name,.pmw-rail-collapsed .pmw-count,.pmw-rail-collapsed .pmw-focus{display:none}
.pmw-rail-collapsed .pmw-rail-head{justify-content:center;padding:6px 0}
.pmw-rail-collapsed .pmw-projects{padding:6px 0}
.pmw-rail-collapsed .pmw-project{justify-content:center;padding:5px 0}
.pmw-rail-collapsed .pmw-project-mono{display:flex;align-items:center;justify-content:center;width:36px;height:36px;border-radius:9px;background:var(--bg-tertiary,#22252c);color:var(--text-secondary,#9a9faa);font:600 12px/1 "SF Mono",Menlo,monospace;text-transform:uppercase}
.pmw-rail-collapsed .pmw-project.active .pmw-project-mono{background:rgba(245,184,64,.16);color:#f5b840;box-shadow:inset 0 0 0 1.5px rgba(245,184,64,.5)}
.pmw-nf-toggle{width:20px;height:20px;border:0;border-radius:5px;background:transparent;color:var(--text-muted,#7f8590);cursor:pointer;font-size:13px;flex:0 0 auto}
.pmw-nf-toggle:hover{background:var(--hover-bg,rgba(255,255,255,.06));color:#fff}
.pmw-nf3.pmw-nf3-collapsed{grid-template-columns:52px minmax(0,1fr)}
.pmw-nf-rail-collapsed .pmw-nf-rail-head{justify-content:center;padding:0}
.pmw-nf-spine{display:flex;flex-direction:column;align-items:center;gap:15px;padding:20px 0;overflow:auto}
.pmw-vspine-dot{width:11px;height:11px;flex:0 0 auto;border:0;border-radius:50%;padding:0;font-size:0;background:transparent;box-shadow:inset 0 0 0 1.5px #3a3f47;cursor:pointer}
.pmw-vspine-dot.pmw-vsdot-done{background:#57b98a;box-shadow:none}
.pmw-vspine-dot.pmw-vsdot-current{width:13px;height:13px;background:#f5b840;box-shadow:none}
.pmw-vspine-dot.sel{outline:2px solid rgba(245,184,64,.5);outline-offset:2px}
.pmw-work { flex:1 1 auto; min-width:0; min-height:0; overflow:hidden; display:flex; }
.pmw-content { flex:1 1 auto; min-width:320px; min-height:0; overflow:auto; }
.pmw-project-shell { container-type:inline-size; display:flex; flex-direction:column; width:100%; height:100%; min-height:0; color:var(--text-primary,#e4e6eb); }
.pmw-project-nav { position:sticky; top:0; z-index:4; display:flex; align-items:center; min-height:44px; padding:0 20px; gap:22px; border-bottom:1px solid var(--border-color,#34363d); background:var(--editor-surface,#1b1d23); }
.pmw-project-nav button { align-self:stretch; padding:0; border:0; border-bottom:2px solid transparent; background:transparent; color:var(--text-secondary,#9a9faa); font:inherit; font-size:12px; cursor:pointer; }
.pmw-project-nav button.active { border-bottom-color:var(--accent,#4f8cff); color:var(--text-primary,#fff); font-weight:650; }
.pmw-project-page { display:flex; flex-direction:column; flex:1 1 auto; min-height:0; padding:22px; gap:18px; }
.pmw-project-docs { display:flex; flex:1 1 auto; min-width:0; min-height:0; overflow:hidden; }
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
.pmw-nf3 { display:grid; grid-template-columns:300px minmax(0,1fr); flex:1 1 auto; min-height:0; overflow:hidden; background:var(--bg-secondary,#202229); }
.pmw-nf-rail { display:flex; flex-direction:column; min-height:0; border-right:1px solid var(--border-color,#34363d); background:var(--editor-surface,#1b1d23); }
.pmw-nf-rail-head { display:flex; align-items:center; justify-content:space-between; flex:0 0 auto; min-height:49px; padding:0 18px; border-bottom:1px solid var(--border-color,#34363d); color:var(--text-muted,#7f8590); font-size:10px; font-weight:700; letter-spacing:.12em; }
.pmw-nf-rail-count { letter-spacing:0; font-weight:500; }
.pmw-nf-stages { flex:1 1 auto; min-height:0; overflow:auto; padding:8px 0; }
.pmw-vstage { display:flex; flex-direction:column; }
.pmw-vstage-row { display:flex; align-items:center; gap:12px; width:100%; padding:9px 18px; border:0; background:transparent; color:var(--text-secondary,#9a9faa); font:inherit; font-size:13.5px; text-align:left; cursor:pointer; }
.pmw-vstage-row:hover { color:var(--text-primary,#fff); }
.pmw-vstage-name { flex:1 1 auto; min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.pmw-vstage-dot { display:flex; align-items:center; justify-content:center; width:17px; height:17px; flex:0 0 auto; border-radius:4px; font-size:11px; font-weight:800; }
.pmw-vsdot-done { background:transparent; box-shadow:inset 0 0 0 1.5px #3d434c; color:#57b98a; }
.pmw-vsdot-current { background:rgba(245,184,64,.16); box-shadow:inset 0 0 0 1.5px #f5b840; }
.pmw-vsdot-upcoming { background:transparent; box-shadow:inset 0 0 0 1.5px #33383f; }
.pmw-vstage.pmw-vstage-done .pmw-vstage-name { color:var(--text-secondary,#9a9faa); }
.pmw-vstage.pmw-vstage-upcoming .pmw-vstage-name { color:var(--text-muted,#7f8590); }
.pmw-vstage-selected { margin:4px 10px; border-radius:10px; background:rgba(245,184,64,.05); box-shadow:inset 0 0 0 1px rgba(245,184,64,.22); }
.pmw-vstage-selected .pmw-vstage-row { color:var(--text-primary,#fff); font-weight:600; }
.pmw-vstage-selected .pmw-stage-files { flex:0 0 auto; max-height:230px; overflow:auto; padding:2px 12px 4px; }
.pmw-vstage-actions { display:flex; align-items:center; gap:8px; padding:6px 12px 12px; }
.pmw-vstage-actions .pmw-promote-stage { margin-left:auto; }
.pmw-nf-center { display:flex; flex-direction:column; min-width:0; min-height:0; background:var(--bg-primary,#17191f); }
.pmw-nf-center .pmw-stage-document { padding:22px 26px; }
.pmw-build{display:flex;flex-direction:column;flex:1 1 auto;min-height:0;padding:18px 20px;gap:14px}
.pmw-build-toolbar{display:flex;align-items:center;gap:10px}
.pmw-build-mlabel{color:var(--text-secondary,#9a9faa);font-size:11px;text-transform:uppercase;letter-spacing:.06em}
.pmw-build-model{padding:6px 8px;border:1px solid var(--border-color,#3a3d45);border-radius:5px;background:var(--bg-primary,#17191f);color:var(--text-primary,#e4e6eb);font-size:12px}
.pmw-build-state{margin-left:auto}
.pmw-build-runs{flex:1 1 auto;min-height:0;overflow:auto;display:flex;flex-direction:column;gap:6px}
.pmw-build-empty{padding:16px 18px;border:1px dashed var(--border-color,#3a3d45);border-radius:8px;color:var(--text-muted,#7f8590);font-size:12px;line-height:1.5}
.pmw-build-run{display:flex;align-items:center;gap:10px;padding:9px 12px;border:1px solid var(--border-color,#34363d);border-radius:7px;background:var(--bg-primary,#17191f);font-size:12px}
.pmw-build-run-id{color:var(--text-primary,#fff);font:11px/1 "SF Mono",Menlo,monospace}
.pmw-build-run-title{color:var(--text-secondary,#9a9faa);overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.pmw-build-pill{padding:2px 8px;border-radius:10px;font-size:10px;text-transform:uppercase;letter-spacing:.05em}
.pmw-build-queued{background:rgba(127,133,144,.16);color:#9a9faa}
.pmw-build-running{background:rgba(245,184,64,.16);color:#f5b840}
.pmw-build-done{background:rgba(87,185,138,.16);color:#57b98a}
.pmw-build-failed{background:rgba(230,90,90,.16);color:#e65a5a}
.pmw-build-cancelled{background:rgba(127,133,144,.16);color:#7f8590}
.pmw-build-pr{color:var(--accent,#4f8cff);text-decoration:none}
.pmw-build-plan-head{color:var(--text-secondary,#9a9faa);font-size:11px;text-transform:uppercase;letter-spacing:.05em;padding:2px 2px 6px}
.pmw-build-bar{display:flex;align-items:center;gap:10px;padding-bottom:2px}
.pmw-build-loop{color:var(--text-muted,#7f8590);font-size:10px;text-transform:uppercase;letter-spacing:.08em}
.pmw-build-runtime{display:inline-flex;border:1px solid var(--border-color,#3a3d45);border-radius:7px;overflow:hidden}
.pmw-build-rt{padding:5px 10px;border:0;background:transparent;color:var(--text-secondary,#9a9faa);font:inherit;font-size:11px;cursor:pointer}
.pmw-build-rt.active{background:rgba(245,184,64,.16);color:#f5b840}
.pmw-build-tabs{display:flex;align-items:center;gap:6px;flex-wrap:wrap;min-height:20px}
.pmw-build-tab{display:inline-flex;align-items:center;gap:6px;padding:5px 10px;border:1px solid var(--border-color,#34363d);border-radius:7px 7px 0 0;border-bottom:0;background:var(--bg-secondary,#202229);color:var(--text-secondary,#9a9faa);font:inherit;font-size:11px;cursor:pointer}
.pmw-build-tab.active{background:#0d0f13;color:var(--text-primary,#fff);box-shadow:inset 0 2px 0 #f5b840}
.pmw-build-tdot{width:7px;height:7px;flex:0 0 auto;border-radius:50%;background:#7f8590}
.pmw-build-tdot.pmw-build-running{background:#f5b840}.pmw-build-tdot.pmw-build-done{background:#57b98a}.pmw-build-tdot.pmw-build-failed{background:#e65a5a}
.pmw-build-term{position:relative;flex:1 1 auto;min-height:0;border:1px solid var(--border-color,#34363d);border-radius:8px;background:#0d0f13;overflow:hidden}
.pmw-build-log{position:absolute;inset:0;overflow:auto;padding:14px 16px;color:#c8d0d8;font:12px/1.55 "SF Mono",Menlo,monospace;white-space:pre-wrap;word-break:break-word}
.pmw-build-log .pmw-build-empty{border:0;padding:0;color:#7f8590;display:block}
.pmw-build-thost{position:absolute;inset:0;padding:6px 8px;background:#0d0f13}
.pmw-build-thost .xterm{height:100%;padding:0}
.pmw-build-wt{display:flex;flex-direction:column;gap:12px;padding:18px 20px}
.pmw-build-wt-h{display:flex;align-items:center;gap:10px}
.pmw-build-wt-h b{color:var(--text-primary,#fff);font-size:14px}
.pmw-build-wt-branch{font:10px/1 "SF Mono",Menlo,monospace;color:var(--text-muted,#7f8590)}
.pmw-build-wt-goal{color:var(--text-secondary,#c8d0d8);font-size:13px;line-height:1.5}
.pmw-build-wt-actions{display:flex;gap:8px;margin-top:2px}
.pmw-build-wt-note{color:var(--text-muted,#7f8590);font-size:11px;line-height:1.5}
.pmw-build-wt-note code{background:rgba(245,184,64,.12);color:#f5b840;padding:1px 5px;border-radius:4px;font-size:10px}
.pmw-build-wt-path{font-family:"SF Mono",Menlo,monospace;font-size:10px}
.pmw-nf-agent { display:flex; flex-direction:column; min-height:0; border-left:1px solid var(--border-color,#34363d); background:var(--editor-surface,#1b1d23); }
.pmw-nf-agent-head { display:flex; align-items:center; gap:11px; flex:0 0 auto; padding:13px 16px; border-bottom:1px solid var(--border-color,#34363d); }
.pmw-nf-agent-avatar { display:flex; align-items:center; justify-content:center; width:30px; height:30px; flex:0 0 auto; border-radius:8px; background:var(--accent,#4f8cff); color:#0a0b0e; font-size:11px; font-weight:700; text-transform:uppercase; }
.pmw-nf-agent-id strong { display:block; color:var(--text-primary,#fff); font-size:14px; }
.pmw-nf-agent-id span { display:block; margin-top:2px; color:var(--text-muted,#7f8590); font-size:10.5px; }
.pmw-nf-agent-body { flex:1 1 auto; min-height:0; overflow:auto; padding:18px 16px; }
.pmw-nf-agent-body .pmw-help { font-size:12.5px; line-height:1.6; color:var(--text-secondary,#9a9faa); }
.pmw-nf-agent-foot { flex:0 0 auto; padding:14px 16px; border-top:1px solid var(--border-color,#34363d); }
.pmw-nf-agent-foot .pmw-ask-agent { width:100%; justify-content:center; }
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
.pmw-detail-head { display:flex; align-items:center; gap:8px; min-height:46px; padding:7px 10px 7px 14px; border-bottom:1px solid var(--border-color,#34363d); }.pmw-detail-id { color:var(--text-muted,#858b96); font-size:11px; font-weight:650; }.pmw-id-chip { border:1px solid var(--border-color,#34363d); border-radius:6px; background:var(--bg-tertiary,#26272c); color:var(--text-primary,#e6e8ee); font-family:ui-monospace,Menlo,monospace; font-size:11px; font-weight:700; letter-spacing:.03em; padding:4px 10px; cursor:copy; }.pmw-id-chip:hover { border-color:var(--xnaut-yellow,#f5b840); color:var(--xnaut-yellow,#f5b840); }
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
        <aside class="pmw-rail"><div class="pmw-rail-head"><span class="pmw-rail-title">Projects</span><span class="pmw-spacer"></span><button class="pmw-focus" hidden title="Show only this project">Focus</button><button class="pmw-rail-toggle" title="Collapse projects" aria-label="Collapse projects">‹</button></div><div class="pmw-projects"></div></aside>
        <div class="pmw-work"><main class="pmw-content"></main><aside class="pmw-detail" hidden></aside></div>
      </div>
      <div class="pmw-overlay" hidden></div>`;
    parent.appendChild(pane);

    const $ = (selector) => pane.querySelector(selector);
    const state = { projects: [], tickets: [], changes: [], status: null, project: opts.project || '', section: opts.section || (opts.project ? 'overview' : 'work'), flowStage: opts.flowStage || '', view: 'board', focus: false, selected: null, selectedChange: '', events: [], request: 0, docsRequest: 0, docsEntry: null };

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
      const tabs = [['overview', 'Overview'], ['nautflow', 'NAUT-Flow'], ['docs', 'Docs'], ['changes', 'Change Management'], ['artifacts', 'Artifacts'], ['work', 'Work'], ['delivery', 'Delivery'], ['settings', 'Settings']];
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

    function disposeProjectDocs() {
      state.docsRequest += 1;
      if (!state.docsEntry) return;
      try { state.docsEntry.dispose?.(); } catch (_) { /* already disposed */ }
      state.docsEntry = null;
    }

    async function mountProjectDocs(project) {
      const host = $('.pmw-project-docs');
      if (!host || typeof window.xnautCreateVaultPane !== 'function') return;
      const request = ++state.docsRequest;
      const stages = stagesFor(project);
      // Scope the Docs tab to the whole PROJECT folder (e.g. "xnaut/") so it shows
      // every doc — Development/, features/, Architecture/ — not just one subtree.
      const prefix = stageDocumentRef(project, stages[0], 0).split('/')[0];
      try {
        const entry = await window.xnautCreateVaultPane(`${label}-docs`, host, { vault: 'work', scopePrefix: prefix, hideChat: true });
        if (request !== state.docsRequest || !host.isConnected || state.section !== 'docs') {
          entry.dispose?.();
          entry.pane?.remove();
          return;
        }
        state.docsEntry = entry;
      } catch (error) {
        if (request === state.docsRequest && host.isConnected) host.innerHTML = `<div class="pmw-empty">${esc(error)}</div>`;
      }
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
      if (state.projectsCollapsed === undefined) { try { state.projectsCollapsed = localStorage.getItem('xnaut-projects-collapsed') === '1'; } catch (_) { state.projectsCollapsed = false; } }
      const collapsed = !!state.projectsCollapsed;
      const counts = state.tickets.reduce((map, ticket) => map.set(ticket.project, (map.get(ticket.project) || 0) + 1), new Map());
      const options = ['<option value="">All projects</option>'].concat(state.projects.map((project) => `<option value="${esc(project.key)}">${esc(project.key)} - ${esc(project.name)}</option>`));
      $('.pmw-project-select').innerHTML = options.join('');
      $('.pmw-project-select').value = state.project;
      $('.pmw-project-details').hidden = !state.project;
      const railEl = $('.pmw-rail'); if (railEl) railEl.classList.toggle('pmw-rail-collapsed', collapsed);
      const toggleBtn = $('.pmw-rail-toggle');
      if (toggleBtn) { toggleBtn.textContent = collapsed ? '›' : '‹'; toggleBtn.title = collapsed ? 'Expand projects' : 'Collapse projects'; toggleBtn.onclick = () => { state.projectsCollapsed = !state.projectsCollapsed; try { localStorage.setItem('xnaut-projects-collapsed', state.projectsCollapsed ? '1' : '0'); } catch (_) {} renderProjectFilters(); }; }
      const mono = (k) => esc(String(k || '').replace(/[^A-Za-z0-9]/g, '').slice(0, 2) || '·');
      $('.pmw-projects').innerHTML = `<button class="pmw-project${state.project ? '' : ' active'}" data-project="" title="All tickets"><span class="pmw-project-mono">∗</span><span class="pmw-project-key">ALL</span><span class="pmw-project-name">All tickets</span><span class="pmw-count">${state.tickets.length}</span></button>` + state.projects.map((project) => `<button class="pmw-project${state.project === project.key ? ' active' : ''}" data-project="${esc(project.key)}" title="${esc(project.key)} · ${esc(project.name)}"><span class="pmw-project-mono">${mono(project.key)}</span><span class="pmw-project-key">${esc(project.key)}</span><span class="pmw-project-name">${esc(project.name)}</span><span class="pmw-count">${counts.get(project.key) || 0}</span></button>`).join('');
      $('.pmw-projects').querySelectorAll('[data-project]').forEach((button) => {
        button.onclick = () => selectProject(button.dataset.project || '');
      });
      const focusBtn = $('.pmw-focus');
      if (focusBtn) {
        if (!state.project) state.focus = false;
        focusBtn.hidden = !state.project || collapsed;
        focusBtn.classList.toggle('active', state.focus);
        focusBtn.onclick = () => { state.focus = !state.focus; renderProjectFilters(); };
      }
      $('.pmw-projects').classList.toggle('focused', state.focus && !!state.project);
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
      return `${folder}/Development/NAUT-Flow/${String(index + 1).padStart(2, '0')}-${file}.md`;
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
        if (note.rel === baseRel) documents.set(1, { rel: baseRel, title: note.title });
        const match = String(note.rel || '').match(new RegExp(`^${stem.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}_v(\\d+)\\.md$`, 'i'));
        if (match) documents.set(Number(match[1]), { rel: note.rel, title: note.title });
      }
      return Array.from(documents, ([version, meta]) => ({ version, rel: meta.rel, title: meta.title || '' })).filter((item) => Number.isFinite(item.version)).sort((a, b) => a.version - b.version);
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
      const rel = stageDocumentRef(project, selected, selectedIndex);
      const next = stages[selectedIndex + 1];
      const currentIndex = Math.max(0, stages.findIndex((stage) => stage[0] === currentKey));
      // Promote forward from the current edge; on any earlier stage, offer
      // "Re-promote" so a skipped / empty stage can be re-run to regenerate the
      // next stage from it (without dragging the project's stage backward).
      const rePromote = selectedIndex < currentIndex;
      const promote = next ? `<button class="pmw-btn ${rePromote ? '' : 'pmw-btn-primary'} pmw-promote-stage">${rePromote ? 'Re-promote' : 'Promote'} to ${esc(next[2])} →</button>` : '';
      // Vertical stage rail. Done stages (< current) collapse green; the selected
      // stage expands with its documents; upcoming stages stay muted. Every row
      // keeps the data-flow-stage hook so stage switching binds unchanged.
      const rail = stages.map((stage, i) => {
        const isSel = stage[0] === selected[0];
        const done = i < currentIndex;
        const st = done ? 'done' : (i === currentIndex ? 'current' : 'upcoming');
        const mark = done
          ? '<span class="pmw-vstage-dot pmw-vsdot-done">✓</span>'
          : `<span class="pmw-vstage-dot pmw-vsdot-${st}"></span>`;
        const body = isSel
          ? `<div class="pmw-stage-files"><div class="pmw-stage-file-empty">Loading documents…</div></div><div class="pmw-vstage-actions"><button class="pmw-icon pmw-stage-new-version" title="Add document" aria-label="Add document">${ICON.plus}</button>${promote}</div>`
          : '';
        return `<div class="pmw-vstage pmw-vstage-${st}${isSel ? ' pmw-vstage-selected' : ''}"><button class="pmw-vstage-row" data-flow-stage="${esc(stage[0])}">${mark}<span class="pmw-vstage-name">${esc(stage[2])}</span></button>${body}</div>`;
      }).join('');
      // Build is execution, not a document: the center becomes a launcher for the
      // multi-agent swarm (worktree-per-ticket → sandbox build/test loop → PR).
      const isBuild = selected[0] === 'build';
      const buildModels = (window.xnautLoom && window.xnautLoom.MODELS) || [['claude-opus-4-8', 'Opus 4.8']];
      const buildModelOpts = buildModels.map(([v, l]) => `<option value="${esc(v)}"${v === 'claude-opus-4-8' ? ' selected' : ''}>${esc(l)}</option>`).join('');
      const centerBody = isBuild
        ? `<div class="pmw-build"><div class="pmw-build-bar"><span class="pmw-build-loop" hidden>LOOP · <span class="pmw-build-iter"></span></span><span class="pmw-spacer"></span><div class="pmw-build-runtime"><button class="pmw-build-rt" data-rt="local" title="Run the agent in the worktree (no sandbox)">Local shell</button><button class="pmw-build-rt" data-rt="sandbox" title="Push to a GitVM sandbox">Sandbox</button></div><select class="pmw-build-model">${buildModelOpts}</select><button class="pmw-btn pmw-btn-primary pmw-build-start">Start build</button><button class="pmw-btn pmw-build-stop" hidden>Stop</button></div><div class="pmw-build-tabs"></div><div class="pmw-build-term"><div class="pmw-build-log"><span class="pmw-build-empty">Start build → the Build manager reads the spec, decides 1–3 worktrees, and opens a live shell in each. Local shell runs the agent (just -g cc) in the worktree; Sandbox pushes to GitVM. On green it merges, opens a PR, and promotes to Test.</span></div></div></div>`
        : `<div class="pmw-stage-document"><div class="pmw-stage-toolbar"><span class="pmw-stage-ref">work:${esc(rel)}</span><button class="pmw-icon pmw-stage-preview-toggle" title="Preview document" aria-label="Preview document">${ICON.eye}</button><button class="pmw-icon pmw-stage-load" title="Load from Vault" aria-label="Load a document from the Vault">${ICON.load}</button><button class="pmw-icon pmw-stage-open" title="Open in Vault" aria-label="Open in Vault">${ICON.open}</button><button class="pmw-icon pmw-stage-save" title="Save document" aria-label="Save document">${ICON.save}</button><button class="pmw-btn pmw-ask-agent">Work with ${esc(selected[3])}</button><button class="pmw-btn pmw-request-review">Request review</button></div><textarea class="pmw-stage-editor" spellcheck="true">${esc(stageTemplate(project, selected))}</textarea><div class="pmw-stage-preview xnaut-md" hidden></div></div>`;
      if (state.nfCollapsed === undefined) { try { state.nfCollapsed = localStorage.getItem('xnaut-nf-collapsed') === '1'; } catch (_) { state.nfCollapsed = false; } }
      const nfCollapsed = !!state.nfCollapsed;
      const spine = stages.map((stage, i) => {
        const done = i < currentIndex; const isSel = stage[0] === selected[0];
        const st = done ? 'done' : (i === currentIndex ? 'current' : 'upcoming');
        return `<button class="pmw-vspine-dot pmw-vsdot-${st}${isSel ? ' sel' : ''}" data-flow-stage="${esc(stage[0])}" title="${esc(stage[2])}">${done ? '✓' : ''}</button>`;
      }).join('');
      const railAside = nfCollapsed
        ? `<aside class="pmw-nf-rail pmw-nf-rail-collapsed"><header class="pmw-nf-rail-head"><button class="pmw-nf-toggle" title="Expand NautFlow">›</button></header><div class="pmw-nf-spine">${spine}</div></aside>`
        : `<aside class="pmw-nf-rail"><header class="pmw-nf-rail-head"><span>NAUTFLOW</span><span class="pmw-spacer"></span><span class="pmw-nf-rail-count">${currentIndex + 1} / ${stages.length}</span><button class="pmw-nf-toggle" title="Collapse NautFlow">‹</button></header><div class="pmw-nf-stages">${rail}</div></aside>`;
      return `<div class="pmw-project-page pmw-project-page-nautflow"><div class="pmw-nf3${nfCollapsed ? ' pmw-nf3-collapsed' : ''}">`
        + railAside
        + `<section class="pmw-nf-center"><header class="pmw-stage-head"><div><h2>${esc(selected[2])}</h2><p>${esc(stageDescription(selected[0]))}</p></div><span class="pmw-spacer"></span><span class="pmw-stage-badge">${isBuild ? 'Execution' : 'Draft'}</span></header>`
        + centerBody + `</section>`
        + `</div></div>`;
    }

    function renderSettings(project) {
      const context = projectContext(project);
      const purpose = project.purpose || project.client?.scope || '';
      return `<div class="pmw-project-page"><div class="pmw-project-hero"><div class="pmw-project-heading"><h2>Project settings</h2><p>Editable project baselines and connections. The project key remains stable because it identifies tickets.</p></div><span class="pmw-stage-badge">Revision ${esc(project.revision || 1)}</span></div><form class="pmw-settings-form"><section class="pmw-settings-section"><h3>Basics</h3><div class="pmw-create-grid"><div class="pmw-field"><label>Project key</label><input class="pmw-input" value="${esc(project.key)}" disabled><span class="pmw-help">Used for ticket IDs and cannot be changed.</span></div><div class="pmw-field"><label>Name</label><input class="pmw-input pmw-settings-name" value="${esc(project.name)}" required></div></div><div class="pmw-field"><label>Purpose</label><textarea class="pmw-textarea pmw-settings-purpose" placeholder="What problem does this project solve, for whom, and what outcome should it achieve?" required>${esc(purpose)}</textarea></div><div class="pmw-field"><label>NAUT-Flow</label><select class="pmw-select pmw-settings-flow"><option value="standard"${project.flow_type !== 'incident' ? ' selected' : ''}>Standard project</option><option value="incident"${project.flow_type === 'incident' ? ' selected' : ''}>Incident fast track</option></select></div></section><section class="pmw-settings-section"><h3>Ownership</h3><div class="pmw-create-grid pmw-create-grid-3"><div class="pmw-field"><label>Project owner</label><input class="pmw-input pmw-settings-owner" value="${esc(project.owner || '')}"></div><div class="pmw-field"><label>Client</label><input class="pmw-input pmw-settings-client" value="${esc(context.client)}"></div><div class="pmw-field"><label>Primary contact</label><input class="pmw-input pmw-settings-contact" value="${esc(project.contact_name || '')}"></div></div><div class="pmw-field"><label>Contact email</label><input class="pmw-input pmw-settings-email" type="email" value="${esc(project.contact_email || '')}"></div></section><section class="pmw-settings-section"><h3>Repository and commercial baseline</h3><div class="pmw-field"><label>Source repository or local folder</label><input class="pmw-input pmw-settings-source" value="${esc(project.source_repo || '')}"></div><div class="pmw-create-grid"><div class="pmw-field"><label>Budget (CHF)</label><input class="pmw-input pmw-settings-budget" type="number" min="0" step="1" value="${context.budget == null ? '' : esc(context.budget)}"></div><div class="pmw-field"><label>Hourly rate (CHF)</label><input class="pmw-input pmw-settings-rate" type="number" min="0" step="0.01" value="${context.rate == null ? '' : esc(context.rate)}"></div></div></section><section class="pmw-settings-section"><h3>Agent connection · MCP</h3><div class="pmw-create-grid"><div class="pmw-field"><label>Local endpoint</label><input class="pmw-input pmw-mcp-url" value="Starting local server..." readonly></div><div class="pmw-field"><label>Bearer token</label><input class="pmw-input pmw-mcp-token" type="password" readonly></div></div><div><button class="pmw-btn pmw-copy-mcp" type="button">Copy MCP connection</button></div></section><div class="pmw-settings-actions"><span class="pmw-settings-state pmw-help"></span><span class="pmw-spacer"></span><button type="submit" class="pmw-btn pmw-btn-primary pmw-settings-save">Save settings</button></div></form></div>`;
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
      if (state.section === 'docs') return '<div class="pmw-project-docs"></div>';
      if (state.section === 'changes') return renderChanges(project);
      if (state.section === 'settings') return renderSettings(project);
      if (state.section === 'artifacts') {
        return `<div class="pmw-project-page">${title}<section class="pmw-project-empty"><h3>NAUT-Flow artifacts</h3><p>Stage documents are stored in the work Vault under ${esc(project.name)}/Development/NAUT-Flow and remain available outside the project workspace.</p><button class="pmw-btn pmw-open-stage-artifacts" style="margin-top:14px">Open current document</button></section></div>`;
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

    // ---- BAMT: the agent methodology (per-persona definitions) ----------------
    // Each NAUT-Flow stage runs a specialised persona with a real working method,
    // an output structure, and elicitation behaviour — not a generic "you are the
    // X". Written for a capable model; personas auto-route to a cloud provider
    // (bamtCloud) when one is configured, otherwise the agent's default model.
    const BAMT_PERSONAS = {
      Analyst: `You are a senior product analyst and strategist (BMAD Analyst). Your job is rigorous discovery, not documentation theatre.
Method: (1) pin the real problem and exactly who has it — challenge vague or assumed needs; (2) explore the opportunity — context, existing alternatives, why now; (3) surface and pressure-test the riskiest assumptions.
Be curious and skeptical: when the input is thin, ask 2–4 sharp clarifying questions BEFORE writing. Never invent facts.
Document structure: Problem · Who it's for · Why now · Opportunity · Key assumptions & risks · Success signals.`,
      PM: `You are a senior product manager (BMAD PM). You turn discovery into a precise, buildable specification.
Method: state goals and non-goals; write user stories/epics with clear acceptance criteria; separate functional from non-functional requirements; mark scope boundaries explicitly. For an Executable-tickets document, shard the spec into small, independently buildable tickets, each with intent, acceptance criteria, and dependencies.
Elicit missing product decisions rather than inventing them.
Document structure: Goals · Non-goals · Users & stories · Functional requirements · Non-functional requirements · Acceptance criteria · Open decisions.`,
      Architect: `You are a principal software architect (BMAD Architect). You make the technical decisions that make the system buildable and maintainable.
Method: propose the architecture with explicit technology choices AND their rationale and tradeoffs; prefer boring, proven options and justify any novel one; define data models and API contracts where the stage calls for it; name the risks and the decisions you are deferring.
Elicit real constraints (scale, latency, compliance, existing stack) before committing.
Document structure: Context & constraints · Design (with a text diagram) · Key decisions & tradeoffs · Data/API detail as applicable · Risks · Open questions.`,
      Planner: `You are a delivery lead (BMAD Planner / Scrum Master). You turn the spec and architecture into an executable plan.
Method: sequence work into phases with explicit dependencies; write sprint stories that are small, testable, and independently shippable; give relative estimates and call out the critical path.
Document structure: Phases · Sprint stories (with acceptance) · Dependencies & critical path · Risks & mitigations.`,
      Security: `You are an application security engineer (BMAD Security). You threat-model the design before it is built.
Method: enumerate assets and trust boundaries; identify prioritised threats (authn/authz, data exposure, injection, supply chain); assess data protection and, for Swiss/EU clients, data-residency and compliance.
Be specific and prioritised — no generic checklists.
Document structure: Assets & trust boundaries · Threats (prioritised) · Controls & requirements · Compliance notes · Residual risks.`,
      Reviewer: `You are a staff QA / review engineer (BMAD Reviewer). You verify work against its acceptance criteria with evidence, not vibes.
Method: derive a test plan from the requirements; check each acceptance criterion; hunt edge cases and regressions; give a clear verdict with required corrections. For a Learning document, capture what worked, what didn't, and reusable anti-patterns for Engram.
Document structure: Test plan · Findings (with severity) · Verdict · Learnings where applicable.`,
      Builder: `You are a senior build engineer (BMAD Builder). You implement the executable tickets end to end — build, run, and test until acceptance passes — keeping changes surgical and verifying before declaring done.`,
    };
    function bamtPersona(role) { return BAMT_PERSONAS[role] || `You are the ${role} for this stage. Work rigorously and elicit missing decisions before writing.`; }
    function bamtSystemPrompt(role, project, stage, rel) {
      return `${bamtPersona(role)}

Project: ${project.name}${project.purpose ? ' — ' + project.purpose : ''}. Current NAUT-Flow stage: ${stage[2]}.
Read the upstream stage documents in the work Vault for context and build on them — never contradict an approved upstream decision without flagging it.
The authoritative artifact for this stage is at work:${rel}. Vault tool rel/from/to values must be relative paths such as "${rel}"; never include a "work:" prefix. When we agree on a revision, write it with vault_write on ${rel}.`;
    }
    // Auto-detect a cloud LLM provider so personas run on Claude, not local qwen.
    let bamtCloud = null; // { provider, model } once configured
    (async () => {
      try {
        const s = await invoke('settings_get');
        const provs = (s && s.llm_providers) || [];
        const cloud = provs.find((p) => p && p.enabled && /anthropic|claude|openrouter|nautgate|:8090/i.test((p.name || '') + ' ' + (p.endpoint || '')));
        if (cloud) bamtCloud = { provider: cloud.name, model: cloud.model || 'claude-opus-4-8' };
      } catch (_) {}
    })();
    const bamtCloudOpts = () => (bamtCloud ? { modelOverride: bamtCloud.model, providerOverride: bamtCloud.provider } : {});

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
        systemPromptAppend: bamtSystemPrompt(role, project, stage, rel),
        ...bamtCloudOpts(),
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
        systemPromptAppend: bamtSystemPrompt(role, project, targetStage, targetRel) + `\n\n${sourceStage[2]} was promoted into ${targetStage[2]}. The approved source is at ${sourceRel} — read it for context, do not modify it; make all new decisions in ${targetRel}.`,
        ...bamtCloudOpts(),
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
      const nfToggle = $('.pmw-nf-toggle');
      if (nfToggle) nfToggle.onclick = () => { state.nfCollapsed = !state.nfCollapsed; try { localStorage.setItem('xnaut-nf-collapsed', state.nfCollapsed ? '1' : '0'); } catch (_) {} renderContent(); };
      const selectedIndex = Math.max(0, stages.findIndex((stage) => stage[0] === state.flowStage));
      const stage = stages[selectedIndex];
      const baseRel = stageDocumentRef(project, stage, selectedIndex);
      if (stage[0] === 'build' && $('.pmw-build')) { bindBuildStage(project, stage, selectedIndex); return; }
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
      let editorDirty = false;
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
          editorDirty = false;
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
      const loadVersion = async (version, replaceDirty = false) => {
        const request = ++documentRequest;
        currentVersion = Number(version) || 1;
        currentRel = versionDocuments.get(currentVersion) || stageVersionRef(baseRel, currentVersion);
        ref.textContent = `work:${currentRel}`;
        let content;
        try { content = await readStageDocument(currentRel); }
        catch (_) { content = currentVersion === 1 ? stageTemplate(project, stage) : ''; }
        if (request !== documentRequest || state.section !== 'nautflow' || state.flowStage !== stage[0] || !editor?.isConnected) return;
        if (editorDirty && !replaceDirty) return;
        editor.value = content;
        editorDirty = false;
        publishAgentContext();
        fileList.querySelectorAll('[data-stage-version]').forEach((button) => button.classList.toggle('active', Number(button.dataset.stageVersion) === currentVersion));
        if (previewActive) paintPreview();
      };
      editor.addEventListener('input', () => { editorDirty = true; publishAgentContext(); });
      // Per-version 3-dot menu: rename (display name via the doc's H1 = vault title),
      // archive (move to an archive/ subfolder), delete (guard the last version).
      const openVersionMenu = (version, rel, documents) => {
        const overlay = $('.pmw-overlay');
        overlay.hidden = false;
        overlay.innerHTML = `<div class="pmw-dialog" style="max-width:420px"><div class="pmw-dialog-head"><span class="pmw-dialog-title">${esc(stage[2])} V${version}</span><span class="pmw-spacer"></span><button class="pmw-icon pmw-dialog-close">${ICON.close}</button></div><div class="pmw-field"><label>Rename (display name)</label><div style="display:flex;gap:6px"><input class="pmw-input pmw-ver-name" placeholder="e.g. Idea — Feature X" style="flex:1"><button class="pmw-btn pmw-ver-rename">Rename</button></div></div><div class="pmw-dialog-actions"><button class="pmw-btn pmw-ver-archive">Archive</button><span class="pmw-spacer"></span><button class="pmw-btn pmw-btn-danger pmw-ver-delete">Delete</button></div></div>`;
        const close = () => { overlay.hidden = true; overlay.innerHTML = ''; };
        overlay.querySelector('.pmw-dialog-close').onclick = close;
        overlay.onclick = (event) => { if (event.target === overlay) close(); };
        overlay.querySelector('.pmw-ver-rename').onclick = async () => {
          const name = overlay.querySelector('.pmw-ver-name').value.trim();
          if (!name) return;
          try {
            let content = await readStageDocument(rel);
            content = /^#\s.*$/m.test(content) ? content.replace(/^#\s.*$/m, `# ${name}`) : `# ${name}\n\n${content}`;
            await writeStageDocument(rel, content);
            if (rel === currentRel) { editor.value = content; editorDirty = false; }
            toast(`Renamed to “${name}”`); close(); await refreshVersions(currentVersion);
          } catch (error) { toast(error, true); }
        };
        overlay.querySelector('.pmw-ver-archive').onclick = async () => {
          const filename = rel.split('/').pop();
          const dir = rel.slice(0, rel.length - filename.length);
          try { await invoke('vault_note_move', { vault: 'work', fromRel: rel, toRel: `${dir}archive/${filename}` }); toast(`${stage[2]} V${version} archived`); close(); await refreshVersions(1); }
          catch (error) { toast(error, true); }
        };
        const delBtn = overlay.querySelector('.pmw-ver-delete');
        let armed = false;
        delBtn.onclick = async () => {
          if ((documents?.length || 1) <= 1) { toast('Cannot delete the only version', true); return; }
          if (!armed) { armed = true; delBtn.textContent = 'Confirm delete'; return; }
          try { await invoke('vault_note_delete', { vault: 'work', rel }); toast(`${stage[2]} V${version} deleted`); close(); await refreshVersions(1); }
          catch (error) { toast(error, true); }
        };
        setTimeout(() => overlay.querySelector('.pmw-ver-name')?.focus(), 0);
      };
      const refreshVersions = async (selectedVersion) => {
        const documents = await stageVersionDocuments(baseRel);
        const versions = documents.map((item) => item.version);
        versionDocuments = new Map(documents.map((item) => [item.version, item.rel]));
        currentVersion = versions.includes(Number(selectedVersion)) ? Number(selectedVersion) : (versions[0] || 1);
        if (!versionDocuments.size) versionDocuments.set(1, baseRel);
        fileList.innerHTML = documents.length ? documents.map((item) => {
          const filename = item.rel.split('/').pop() || item.rel;
          const named = item.title && item.title.trim() && item.title.trim() !== stage[2];
          const label = named ? item.title.trim() : `${stage[2]} V${item.version}`;
          const sub = named ? `V${item.version} · ${filename}` : filename;
          return `<div class="pmw-stage-file-row" style="display:flex;align-items:center;gap:2px">`
            + `<button class="pmw-stage-file${item.version === currentVersion ? ' active' : ''}" data-stage-version="${item.version}" title="${esc(item.rel)}" style="flex:1;min-width:0">${ICON.doc}<span class="pmw-stage-file-copy"><span class="pmw-stage-file-title">${esc(label)}</span><span class="pmw-stage-file-name">${esc(sub)}</span></span></button>`
            + `<button class="pmw-icon pmw-stage-kebab" data-rel="${esc(item.rel)}" data-version="${item.version}" title="Rename · archive · delete" aria-label="Version actions" style="flex-shrink:0">${ICON.kebab}</button>`
            + `</div>`;
        }).join('') : '<div class="pmw-stage-file-empty">No documents yet. Save the draft or create the first version.</div>';
        fileList.querySelectorAll('[data-stage-version]').forEach((button) => { button.onclick = () => loadVersion(button.dataset.stageVersion, true); });
        fileList.querySelectorAll('.pmw-stage-kebab').forEach((button) => { button.onclick = (event) => { event.stopPropagation(); openVersionMenu(Number(button.dataset.version), button.dataset.rel, documents); }; });
        await loadVersion(currentVersion);
      };
      versionCreate.onclick = async () => {
        versionCreate.disabled = true;
        try {
          const versions = (await stageVersionDocuments(baseRel)).map((item) => item.version);
          const next = versions.length ? Math.max(...versions) + 1 : 1;
          const nextRel = stageVersionRef(baseRel, next);
          // Start a NEW version from the blank stage template — not a copy of the
          // current editor. Copying was the "new case duplicates the existing one"
          // bug (XNAUT-17): every new version cloned the doc you were on.
          await writeStageDocument(nextRel, stageTemplate(project, stage));
          editorDirty = false;
          await refreshVersions(next);
          toast(`${stage[2]} V${next} created`);
        } catch (error) { toast(error, true); }
        finally { versionCreate.disabled = false; }
      };
      refreshVersions(1).catch((error) => toast(error, true));
      $('.pmw-stage-save').onclick = async (event) => {
        const button = event.currentTarget;
        button.disabled = true;
        try { await writeStageDocument(currentRel, editor.value); editorDirty = false; await refreshVersions(currentVersion); toast(`${stage[2]} V${currentVersion} saved to Vault`); }
        catch (error) { toast(error, true); }
        finally { button.disabled = false; }
      };
      // Load an existing document from the work Vault into this stage editor
      // (non-destructive: loads into the editor as an unsaved draft; Save to keep).
      $('.pmw-stage-load').onclick = async () => {
        let notes = [];
        try {
          let tree;
          try { tree = await invoke('vault_tree', { vault: 'work' }); }
          catch (e) { if (!String(e).includes('vault not open')) throw e; await invoke('vault_open', { vault: 'work' }); tree = await invoke('vault_tree', { vault: 'work' }); }
          // Scope to the current project's folder only (Development/<project>/…),
          // derived from this stage's baseRel — not the whole vault.
          const projectPrefix = baseRel.split('/').slice(0, 2).join('/') + '/';
          notes = (tree?.notes || []).filter((n) => { const r = String(n.rel || ''); return r.toLowerCase().endsWith('.md') && r.startsWith(projectPrefix); });
        } catch (error) { toast(error, true); return; }
        if (!notes.length) { toast('No documents for this project in the vault yet.'); return; }
        const overlay = $('.pmw-overlay');
        overlay.hidden = false;
        const options = notes.map((n) => `<option value="${esc(n.rel)}">${esc(n.title || n.rel)}</option>`).join('');
        overlay.innerHTML = `<div class="pmw-dialog" style="max-width:560px"><div class="pmw-dialog-head"><span class="pmw-dialog-title">Load from Vault → ${esc(stage[2])}</span><span class="pmw-spacer"></span><button class="pmw-icon pmw-dialog-close">${ICON.close}</button></div><div class="pmw-field"><label>Document</label><select class="pmw-select pmw-vault-select">${options}</select></div><div class="pmw-dialog-actions"><button class="pmw-btn pmw-dialog-cancel">Cancel</button><button class="pmw-btn pmw-btn-primary pmw-vault-load">Load</button></div></div>`;
        const close = () => { overlay.hidden = true; overlay.innerHTML = ''; };
        overlay.querySelector('.pmw-dialog-close').onclick = close;
        overlay.querySelector('.pmw-dialog-cancel').onclick = close;
        overlay.onclick = (event) => { if (event.target === overlay) close(); };
        overlay.querySelector('.pmw-vault-load').onclick = async () => {
          const rel = overlay.querySelector('.pmw-vault-select').value;
          try { editor.value = await readStageDocument(rel); editorDirty = true; if (previewActive) paintPreview(); publishAgentContext(); toast(`Loaded ${rel}`); close(); }
          catch (error) { toast(error, true); }
        };
        setTimeout(() => overlay.querySelector('.pmw-vault-select')?.focus(), 0);
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
          // Only advance the project's stage when promoting past the current
          // edge; re-promoting an earlier stage regenerates the next doc but must
          // not move the project backward.
          const curIdx = Math.max(0, stages.findIndex((s) => s[0] === (project.stage || stages[0][0])));
          const advanceKey = targetIndex > curIdx ? targetStage[0] : (project.stage || stages[0][0]);
          const updated = await invoke('pm_project_update', { request: projectUpdatePayload(project, advanceKey) });
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

    // Build stage: launch the multi-agent swarm for this project, watch its queue.
    // Live-shell helpers (real PTY via the same create_command_session the app's
    // terminals use). Sessions persist in buildRuns so navigating away from the
    // Build stage does not kill the running agent; only Stop ends the PTY.
    const buildRuns = {}; // project.key -> { wts:[{id,title,branch,wt,sid,status,host,ctl}] }
    async function startShell(cwd, command) {
      const res = await invoke('create_command_session', { config: { program: 'sh', args: ['-c', command], workingDir: cwd } });
      return res.session_id || res.sessionId || res.id;
    }
    function killShell(sid) { try { invoke('close_terminal', { sessionId: sid }).catch(() => {}); } catch (_) {} }
    async function embedShell(host, sid) {
      const listen = window.__TAURI__.event.listen;
      const term = new Terminal({ theme: { background: '#0d0f13', foreground: '#c8d0d8', cursor: '#f5b840' }, fontFamily: '"SF Mono", Menlo, "JetBrains Mono", monospace', fontSize: 12, lineHeight: 1.2, cursorBlink: true, scrollback: 10000, allowTransparency: true });
      term.open(host);
      let fit = null; try { fit = new FitAddon.FitAddon(); term.loadAddon(fit); fit.fit(); } catch (_) {}
      const unData = await listen(`terminal-output:${sid}`, (e) => { try { term.write(e.payload); } catch (_) {} });
      term.onData((d) => { invoke('write_to_terminal', { sessionId: sid, data: d }).catch(() => {}); });
      const ro = new ResizeObserver(() => { try { fit && fit.fit(); invoke('resize_pty', { sessionId: sid, cols: term.cols, rows: term.rows }).catch(() => {}); } catch (_) {} });
      try { ro.observe(host); } catch (_) {}
      return {
        show() { try { fit && fit.fit(); invoke('resize_pty', { sessionId: sid, cols: term.cols, rows: term.rows }).catch(() => {}); term.focus(); } catch (_) {} },
        detach() { try { unData && unData(); } catch (_) {} try { ro.disconnect(); } catch (_) {} try { term.dispose(); } catch (_) {} },
      };
    }
    function publishBuildToSwarm(key, wts) {
      try {
        if (!window.xnautSwarm) return;
        window.xnautSwarm.project = key;
        window.xnautSwarm.queue = wts.map((w) => ({ id: w.id, title: w.title, project: key, status: w.status, wt: w.wt, sid: w.sid, started: w.started, local: true, model: (window.xnautSwarm.model || '') }));
        window.xnautSwarm.active = wts.some((w) => w.status === 'running');
        window.dispatchEvent(new CustomEvent('xnaut-swarm-update'));
      } catch (_) {}
    }

    // Integrator: launch an agent in the MAIN repo to merge the parallel worktree
    // branches into one runnable product and write run instructions.
    async function consolidateBuild(projectKey) {
      const root = (await (window.xnautLoom && window.xnautLoom.resolveProjectRoot(projectKey))) || '';
      if (!root) throw new Error('No local folder for ' + projectKey + '.');
      let branches = [];
      try { branches = ((await invoke('git_branches', { repo: root })) || []).filter((b) => /(^|\/)nautloom\//.test(b) || /^nautloom\//.test(b)); } catch (_) {}
      const r = buildRuns[projectKey];
      const goals = {}; if (r) r.wts.forEach((w) => { goals[w.branch] = w.goal || w.title; });
      const src = branches.length ? branches : Object.keys(goals);
      const list = src.map((b) => `- ${b}${goals[b] ? ': ' + goals[b] : ''}`).join('\n') || '(no nautloom/* branches found — inspect the worktrees)';
      const goal = `Integrate the parallel build into a single runnable product.\n\n`
        + `You are in the main git repository. Parallel worktree branches each built part of this project:\n${list}\n\n`
        + `Do this, in order:\n`
        + `1. Merge the useful branches into the current branch (e.g. \`git merge <branch>\`), resolving conflicts sensibly. If several branches are competing/duplicate attempts at the same thing, keep the most complete one and drop the rest.\n`
        + `2. Make the project actually build and run — fix wiring, install dependencies, ensure a clear entry point exists.\n`
        + `3. Write a clear "## How to run" section in README.md: the exact install, build, and start commands, plus the URL/port if it is a web app.\n`
        + `4. Commit everything with a clear message.\n`
        + `End by printing exactly how to start the product.`;
      try { await invoke('write_file', { path: root + '/.integrate-goal.txt', content: goal }); } catch (_) {}
      const sid = await startShell(root, 'claude -p --allow-dangerously-skip-permissions "$(cat .integrate-goal.txt)" 2>&1');
      if (window.xnautAttachAgentTab) window.xnautAttachAgentTab(sid, 'Integrator · ' + projectKey);
      return sid;
    }
    window.xnautBuildConsolidate = (key) => consolidateBuild(key || (window.xnautSwarm && window.xnautSwarm.project) || '');

    function bindBuildStage(project, stage, selectedIndex) {
      const stages = stagesFor(project);
      const panel = $('.pmw-build'); if (!panel) return;
      const tabsEl = panel.querySelector('.pmw-build-tabs');
      const termEl = panel.querySelector('.pmw-build-term');
      const startBtn = panel.querySelector('.pmw-build-start');
      const stopBtn = panel.querySelector('.pmw-build-stop');
      const modelSel = panel.querySelector('.pmw-build-model');
      const loopEl = panel.querySelector('.pmw-build-loop');
      const iterEl = panel.querySelector('.pmw-build-iter');
      let activeTab = 0, lastLog = '';
      const logEl = () => panel.querySelector('.pmw-build-log');
      const run = () => buildRuns[project.key] || null; // ongoing local build for this project
      const units = () => { const r = run(); if (r) return r.wts; const sw = window.xnautSwarm; return sw && sw.queue && sw.project === project.key ? sw.queue : []; };
      const isActive = () => { const r = run(); if (r) return r.wts.some((w) => w.status === 'running'); return !!(window.xnautSwarm && window.xnautSwarm.active); };

      // Runtime toggle: local shell (real PTY in the worktree) | sandbox (GitVM).
      const runtime = () => (window.xnautSwarm && window.xnautSwarm.runtime) || localStorage.getItem('xnaut-build-runtime') || 'local';
      const paintRuntime = () => panel.querySelectorAll('.pmw-build-rt').forEach((b) => b.classList.toggle('active', b.dataset.rt === runtime()));
      panel.querySelectorAll('.pmw-build-rt').forEach((b) => b.onclick = () => {
        if (isActive()) { toast('Stop the current build to change runtime.'); return; }
        try { localStorage.setItem('xnaut-build-runtime', b.dataset.rt); } catch (_) {}
        if (window.xnautSwarm) window.xnautSwarm.runtime = b.dataset.rt;
        paintRuntime();
      });
      paintRuntime();

      const paintTerm = async () => {
        if (run()) return; // local shells render themselves
        const q = units(); const t = q[activeTab]; const log = logEl(); if (!t || !log) return;
        if (!t.log) { log.textContent = t.status === 'queued' ? 'Queued — waiting for a worktree slot…' : 'Starting worktree…'; return; }
        let txt = ''; try { txt = (await invoke('read_file', { path: t.log })) || ''; } catch (_) {}
        if (txt && txt !== lastLog) { lastLog = txt; log.textContent = txt.split('\n').slice(-500).join('\n'); log.scrollTop = log.scrollHeight; }
      };
      const showTerm = () => {
        const log = logEl(); if (!log) return;
        log.style.display = 'block';
        const r = run();
        if (!r) { paintTerm(); return; } // sandbox log tail / idle hint
        const u = r.wts; const t = u[activeTab];
        if (!t) { log.innerHTML = '<span class="pmw-build-empty">No worktree selected.</span>'; return; }
        const pill = `<span class="pmw-build-pill pmw-build-${esc(t.status)}">${esc(t.status)}</span>`;
        log.innerHTML = `<div class="pmw-build-wt">`
          + `<div class="pmw-build-wt-h">${pill}<b>${esc(t.title || t.id)}</b><span class="pmw-build-wt-branch">${esc(t.branch || '')}</span></div>`
          + `<div class="pmw-build-wt-goal">${esc(t.goal || ('Build ' + (t.title || t.id)))}</div>`
          + `<div class="pmw-build-wt-actions"><button class="pmw-btn pmw-btn-primary" data-openshell${t.sid ? '' : ' disabled'}>▸ Open shell — watch it build</button><button class="pmw-btn" data-openfolder>Open worktree folder</button></div>`
          + `<div class="pmw-build-wt-note">Runs <code>just -g cc</code> in <span class="pmw-build-wt-path">${esc(t.wt || '')}</span>. The agent builds here whether or not the shell is open — open it to watch or steer.</div>`
          + `</div>`;
        const os = log.querySelector('[data-openshell]'); if (os) os.onclick = () => { if (t.sid && window.xnautAttachAgentTab) window.xnautAttachAgentTab(t.sid, 'wt · ' + (t.title || t.id)); };
        const of = log.querySelector('[data-openfolder]'); if (of) of.onclick = () => { if (t.wt) openDocument(t.wt); };
      };
      const renderTabs = () => {
        const u = units(); const active = isActive();
        startBtn.hidden = active; stopBtn.hidden = !active;
        if (loopEl) loopEl.hidden = !u.length;
        if (!u.length) { tabsEl.innerHTML = ''; return; }
        if (activeTab >= u.length) activeTab = 0;
        tabsEl.innerHTML = u.map((t, i) => `<button class="pmw-build-tab${i === activeTab ? ' active' : ''}" data-tab="${i}"><span class="pmw-build-tdot pmw-build-${esc(t.status)}"></span>wt-${i + 1} · ${esc(String(t.title || t.id).slice(0, 22))}</button>`).join('');
        tabsEl.querySelectorAll('[data-tab]').forEach((b) => b.onclick = () => { activeTab = +b.dataset.tab; lastLog = ''; renderTabs(); showTerm(); });
        const done = u.filter((x) => x.status === 'done').length;
        if (iterEl) iterEl.textContent = active ? `building · ${done}/${u.length} green` : `${done}/${u.length} green`;
      };
      // Build manager: read the spec docs, decide 1–3 parallel worktrees.
      const specStages = ['tickets', 'architecture', 'prd', 'data_model', 'api_design'];
      async function planBuild() {
        const stgs = stagesFor(project);
        let spec = '';
        for (const key of specStages) {
          const i = stgs.findIndex((s) => s[0] === key); if (i < 0) continue;
          try { const txt = await readStageDocument(stageDocumentRef(project, stgs[i], i)); if (txt && txt.trim().length > 40) spec += `\n\n# ${stgs[i][2]}\n${txt}`; } catch (_) {}
        }
        const sys = `You are the Build manager for the software project "${project.name}". Read the specification and decide how to build it as 1 to 3 parallel git worktrees. Each worktree is a self-contained slice a single coding agent builds independently in its own branch. Prefer fewer worktrees; split only when parts are genuinely independent (e.g. frontend vs API vs data layer). Respond STRICT JSON only, no prose:\n{"worktrees":[{"branch":"feat/<slug>","title":"<short label>","goal":"<concrete description of what to build here>"}],"reasoning":"<one line>"}`;
        const user = spec.trim() || `Project purpose: ${project.purpose || project.name}. No detailed spec documents were found; plan a single worktree that scaffolds the project.`;
        const raw = await invoke('chat_send', { requestId: 'buildplan-' + Date.now(), messages: [{ role: 'system', content: sys }, { role: 'user', content: user }] });
        const jm = String(raw).match(/\{[\s\S]*\}/);
        const plan = jm ? JSON.parse(jm[0]) : null;
        if (!plan || !Array.isArray(plan.worktrees) || !plan.worktrees.length) return null;
        plan.worktrees = plan.worktrees.slice(0, 3);
        return plan;
      }
      // Create worktrees + a live PTY shell per worktree (no GitVM). Sessions live
      // in buildRuns so they survive panel re-renders; only Stop ends them.
      async function startLocalBuild(worktrees) {
        const root = (await (window.xnautLoom && window.xnautLoom.resolveProjectRoot(project.key))) || '';
        if (!root) throw new Error('No local folder for ' + project.key + '. Set the source path in Settings.');
        const model = modelSel.value;
        const rec = /^codex/.test(model) ? 'just -g codex' : /^pi/.test(model) ? 'justpi' : 'just -g cc';
        const wts = [];
        for (const w of worktrees) {
          const slug = (String(w.branch || w.title || ('wt' + (wts.length + 1))).toLowerCase().replace(/^nautloom\//, '').replace(/[^a-z0-9/_-]+/g, '-').replace(/(^-+|-+$)/g, '')) || ('wt' + (wts.length + 1));
          const branch = 'nautloom/' + slug;
          const wt = await invoke('worktree_suggest_path', { repoPath: root, branch });
          try { await invoke('worktree_add', { repoPath: root, worktreePath: wt, opts: { branch, base: null, checkout_existing: false } }); }
          catch (_) { await invoke('worktree_add', { repoPath: root, worktreePath: wt, opts: { branch, base: null, checkout_existing: true } }); }
          try { await invoke('write_file', { path: wt + '/.build-goal.txt', content: w.goal || w.title || '' }); } catch (_) {}
          let sid = null; try { sid = await startShell(wt, rec); } catch (_) {}
          wts.push({ id: slug, title: w.title || slug, goal: w.goal || w.title || '', branch, wt, sid, status: sid ? 'running' : 'failed', started: Date.now() });
        }
        buildRuns[project.key] = { wts };
        publishBuildToSwarm(project.key, wts);
        activeTab = 0; // shells are attached by the re-render below (avoids a double-attach race)
      }
      function stopLocalBuild() {
        const r = run(); if (!r) return;
        r.wts.forEach((w) => { if (w.sid) killShell(w.sid); w.status = 'cancelled'; });
        delete buildRuns[project.key];
        publishBuildToSwarm(project.key, []);
      }

      startBtn.onclick = async () => {
        if (isActive()) { toast('A build is already running.'); return; }
        startBtn.disabled = true; const log = logEl(); if (log) { log.style.display = 'block'; log.textContent = 'Build manager planning…'; }
        try {
          let plan = null; try { plan = await planBuild(); } catch (_) {}
          const worktrees = (plan && plan.worktrees) || [{ branch: project.key.toLowerCase() + '-build', title: 'Build ' + project.name, goal: 'Build ' + project.name + ' from its NautFlow specification in the work Vault.' }];
          const rt = runtime();
          if (rt === 'local') {
            await startLocalBuild(worktrees);
            toast(`Started ${worktrees.length} worktree build${worktrees.length === 1 ? '' : 's'} — open a shell to watch.`);
          } else {
            if (!window.xnautSwarm || !window.xnautSwarm.launchPlan) { toast('Swarm engine not loaded.', true); return; }
            const r = await window.xnautSwarm.launchPlan(project.key, worktrees, { model: modelSel.value, runtime: 'sandbox' });
            toast(`Sandbox build: ${r.count} worktree${r.count === 1 ? '' : 's'}.`);
          }
          state.nfCollapsed = true; // auto-collapse NautFlow when the build starts (per design)
          try { window.xnautShowRightPane && window.xnautShowRightPane(); window.xnautRightPaneShow && window.xnautRightPaneShow('buildrun'); } catch (_) {}
          renderContent(); // re-render applies the collapse + the fresh build status
        } catch (e) { const m = String((e && e.message) || e); const l = logEl(); if (l) { l.style.display = 'block'; l.textContent = m; } toast(m, true); }
        finally { startBtn.disabled = false; }
      };
      stopBtn.onclick = async () => {
        if (run()) { stopLocalBuild(); renderTabs(); const l = logEl(); if (l) { l.style.display = 'block'; l.textContent = 'Stopped.'; } }
        else if (window.xnautSwarm && window.xnautSwarm.stopAll) await window.xnautSwarm.stopAll();
      };

      // Re-render on swarm updates; tail the sandbox log on a timer. Local shells
      // (PTYs) live in buildRuns and keep running across renders regardless.
      const onUpdate = () => { if (!panel.isConnected) { window.removeEventListener('xnaut-swarm-update', onUpdate); return; } renderTabs(); showTerm(); };
      window.addEventListener('xnaut-swarm-update', onUpdate);
      const termTimer = setInterval(() => { if (!panel.isConnected) { clearInterval(termTimer); return; } if (!run() && window.xnautSwarm && window.xnautSwarm.active) paintTerm(); }, 2000);
      renderTabs(); showTerm();
      // Let the right-pane Build run "Promote to Test" button drive the rail promote.
      window.xnautBuildPromote = () => { const p = document.querySelector('.pmw-promote-stage'); if (p && !p.disabled) p.click(); };

      // Promote to Test — bound here because the editor path returned early.
      const promote = $('.pmw-promote-stage');
      const targetStage = stages[selectedIndex + 1];
      if (promote && targetStage) promote.onclick = async () => {
        const targetIndex = selectedIndex + 1;
        promote.disabled = true; promote.textContent = 'Promoting…';
        try {
          const curIdx = Math.max(0, stages.findIndex((s) => s[0] === (project.stage || stages[0][0])));
          const advanceKey = targetIndex > curIdx ? targetStage[0] : (project.stage || stages[0][0]);
          const updated = await invoke('pm_project_update', { request: projectUpdatePayload(project, advanceKey) });
          const idx = state.projects.findIndex((item) => item.key === updated.key);
          if (idx >= 0) state.projects[idx] = updated;
          state.flowStage = targetStage[0];
          renderProjectFilters();
          renderContent();
          toast(`${stage[2]} promoted to ${targetStage[2]}`);
        } catch (error) { toast(error, true); if (promote.isConnected) { promote.disabled = false; promote.textContent = `Promote to ${targetStage[2]}`; } }
      };
    }

    function bindSettings(project) {
      const form = $('.pmw-settings-form');
      if (!form) return;
      invoke('project_mcp_info').then((info) => {
        $('.pmw-mcp-url').value = info.url;
        $('.pmw-mcp-token').value = info.token;
        $('.pmw-copy-mcp').onclick = async () => {
          await navigator.clipboard.writeText(JSON.stringify({ url: info.url, headers: { Authorization: `Bearer ${info.token}` } }, null, 2));
          toast('MCP connection copied');
        };
      }).catch((error) => { $('.pmw-mcp-url').value = String(error); });
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
      if (state.section === 'docs') mountProjectDocs(project);
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
      if (state.section === 'docs' && state.docsEntry?.pane?.isConnected) return;
      disposeProjectDocs();
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
      detail.innerHTML = `<header class="pmw-detail-head"><span class="pmw-detail-id">revision ${ticket.revision}</span><span class="pmw-spacer"></span><button class="pmw-id-chip" title="Copy ticket ID">${esc(ticket.id)}</button><button class="pmw-icon pmw-detail-close" title="Close">${ICON.close}</button></header><div class="pmw-detail-body"><div class="pmw-field"><label>Title</label><input class="pmw-input pmw-edit-title" value="${esc(ticket.title)}"></div><div class="pmw-field-grid"><div class="pmw-field"><label>Type</label><select class="pmw-select pmw-edit-type">${TYPES.map((value) => `<option${ticket.ticket_type === value ? ' selected' : ''}>${value}</option>`).join('')}</select></div><div class="pmw-field"><label>Priority</label><select class="pmw-select pmw-edit-priority">${PRIORITIES.map((value) => `<option${ticket.priority === value ? ' selected' : ''}>${value}</option>`).join('')}</select></div><div class="pmw-field"><label>Status</label><select class="pmw-select pmw-edit-status">${STATUSES.map((value) => `<option value="${value}"${ticket.status === value ? ' selected' : ''}>${LABELS[value]}</option>`).join('')}</select></div></div><div class="pmw-field"><label>Owner</label><input class="pmw-input pmw-edit-owner" value="${esc(ticket.owner || '')}" placeholder="Unassigned"></div><div class="pmw-field"><label>Description</label><textarea class="pmw-textarea pmw-edit-body">${esc(ticket.body)}</textarea></div><div class="pmw-field"><label>Vault documents (one reference per line)</label><textarea class="pmw-textarea pmw-docs pmw-edit-docs" placeholder="work:project/Development/document.md">${esc((ticket.documentation || []).join('\n'))}</textarea><div class="pmw-doc-links"></div></div><section class="pmw-activity"><div class="pmw-section-title">Activity</div><div class="pmw-events"><span class="pmw-event-time">Loading...</span></div></section></div><footer class="pmw-detail-actions"><button class="pmw-btn pmw-btn-danger pmw-delete">Delete</button><button class="pmw-btn pmw-create-loom" title="Open a loom run pre-filled with this ticket">▸ Create Loom</button><span class="pmw-spacer"></span><button class="pmw-btn pmw-save">Save changes</button><button class="pmw-btn pmw-btn-primary pmw-save-close">Save and close</button></footer>`;
      detail.querySelector('.pmw-detail-close').onclick = () => { state.selected = null; renderDetail(); renderContent(); };
      const idChip = detail.querySelector('.pmw-id-chip');
      if (idChip) idChip.onclick = async () => {
        try { await navigator.clipboard.writeText(ticket.id); } catch (_) {}
        const was = idChip.textContent; idChip.textContent = 'copied ✓';
        setTimeout(() => { idChip.textContent = was; }, 900);
      };
      detail.querySelector('.pmw-save').onclick = () => saveDetail(false);
      detail.querySelector('.pmw-save-close').onclick = () => saveDetail(true);
      bindDelete(detail.querySelector('.pmw-delete'));
      const createLoom = detail.querySelector('.pmw-create-loom');
      if (createLoom) createLoom.onclick = () => {
        if (typeof window.xnautCreateLoomFromTicket === 'function') window.xnautCreateLoomFromTicket(state.selected);
        else toast('Looms view not available', true);
      };
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
      overlay.innerHTML = `<div class="pmw-dialog"><div class="pmw-dialog-head"><span class="pmw-dialog-title">New ticket</span><span class="pmw-spacer"></span><button class="pmw-icon pmw-dialog-close">${ICON.close}</button></div><div class="pmw-field"><label>Project</label><select class="pmw-select pmw-new-ticket-project">${projectOptions}</select></div><div class="pmw-field"><label>Title</label><input class="pmw-input pmw-new-ticket-title" placeholder="Describe the outcome"></div><div class="pmw-field-grid"><div class="pmw-field"><label>Type</label><select class="pmw-select pmw-new-type">${TYPES.map((value) => `<option${value === 'task' ? ' selected' : ''}>${value}</option>`).join('')}</select></div><div class="pmw-field"><label>Priority</label><select class="pmw-select pmw-new-priority">${PRIORITIES.map((value) => `<option${value === 'medium' ? ' selected' : ''}>${value}</option>`).join('')}</select></div><div class="pmw-field"><label>Status</label><select class="pmw-select pmw-new-status">${STATUSES.map((value) => `<option value="${value}">${LABELS[value]}</option>`).join('')}</select></div></div><div class="pmw-field"><label>Owner</label><input class="pmw-input pmw-new-owner" placeholder="Unassigned"></div><div class="pmw-field"><label>Description</label><textarea class="pmw-textarea pmw-new-body"></textarea></div><div class="pmw-field"><label>Vault documents</label><textarea class="pmw-textarea pmw-docs pmw-new-docs" placeholder="work:project/Development/document.md"></textarea></div><div class="pmw-dialog-actions"><button class="pmw-btn pmw-dialog-cancel">Cancel</button><button class="pmw-btn pmw-btn-primary pmw-dialog-submit">Create ticket</button></div></div>`;
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
        const keepNautFlowEditor = state.section === 'nautflow' && Boolean($('.pmw-stage-editor')?.isConnected) && Boolean(state.project);
        paintStatus(); renderProjectFilters();
        if (!keepNautFlowEditor) renderContent();
        renderDetail();
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
        disposeProjectDocs();
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
