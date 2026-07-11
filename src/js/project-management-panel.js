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
  const ICON = {
    refresh: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M13 8a5 5 0 1 1-1.5-3.5"/><path d="M13 2v3h-3"/></svg>',
    sync: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M3 5h8l-2-2M13 11H5l2 2"/></svg>',
    close: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M4 4l8 8M12 4l-8 8"/></svg>',
    doc: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3"><path d="M4 1.5h5l3 3v10H4z"/><path d="M9 1.5v3h3"/></svg>',
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
.pmw-btn-primary { background:var(--accent,#4f8cff); border-color:var(--accent,#4f8cff); color:#fff; }
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
@media(max-width:900px){.pmw-rail{display:none}.pmw-detail{position:absolute;inset:0;z-index:8;min-width:0;flex-basis:auto}.pmw-work{position:relative}.pmw-sync-state{display:none}}
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
        <div class="pmw-segment"><button data-view="board" class="active">Board</button><button data-view="list">List</button></div>
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
    const state = { projects: [], tickets: [], status: null, project: opts.project || '', view: 'board', selected: null, events: [], request: 0 };

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

    function renderContent() {
      const tickets = visibleTickets();
      if (!state.projects.length) {
        $('.pmw-content').innerHTML = '<div class="pmw-empty"><strong>No projects yet.</strong><br>Create the first project to start tracking work.</div>';
        return;
      }
      if (state.view === 'list') {
        $('.pmw-content').innerHTML = `<table class="pmw-list"><thead><tr><th>ID</th><th>Title</th><th>Project</th><th>Type</th><th>Priority</th><th>Status</th><th>Owner</th><th>Updated</th></tr></thead><tbody>${tickets.map((ticket) => `<tr data-id="${esc(ticket.id)}"><td>${esc(ticket.id)}</td><td>${esc(ticket.title)}</td><td>${esc(ticket.project)}</td><td>${esc(ticket.ticket_type)}</td><td><span class="pmw-chip pmw-priority-${esc(ticket.priority)}">${esc(ticket.priority)}</span></td><td>${esc(LABELS[ticket.status] || ticket.status)}</td><td>${esc(ticket.owner || '')}</td><td>${esc(relativeTime(ticket.updated_at))}</td></tr>`).join('')}</tbody></table>`;
      } else {
        $('.pmw-content').innerHTML = `<div class="pmw-board">${STATUSES.map((status) => { const items = tickets.filter((ticket) => ticket.status === status); return `<section class="pmw-column"><header class="pmw-column-head"><span class="pmw-status-dot" data-status="${status}"></span><span>${esc(LABELS[status])}</span><span class="pmw-count">${items.length}</span></header><div class="pmw-column-body" data-drop-status="${status}">${items.map(ticketCard).join('')}</div></section>`; }).join('')}</div>`;
      }
      bindTickets();
    }

    function selectProject(key) {
      state.project = key;
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

    function showDialog(kind) {
      const overlay = $('.pmw-overlay');
      const projectOptions = state.projects.map((project) => `<option value="${esc(project.key)}"${state.project === project.key ? ' selected' : ''}>${esc(project.key)} - ${esc(project.name)}</option>`).join('');
      overlay.hidden = false;
      overlay.innerHTML = kind === 'project' ? `<div class="pmw-dialog"><div class="pmw-dialog-head"><span class="pmw-dialog-title">New project</span><span class="pmw-spacer"></span><button class="pmw-icon pmw-dialog-close">${ICON.close}</button></div><div class="pmw-field"><label>Project key</label><input class="pmw-input pmw-new-key" maxlength="12" placeholder="XNAUT"></div><div class="pmw-field"><label>Name</label><input class="pmw-input pmw-new-project-name" placeholder="xNaut"></div><div class="pmw-field"><label>Source repository or folder</label><input class="pmw-input pmw-new-source" placeholder="/path/to/project or SSH URL"></div><div class="pmw-dialog-actions"><button class="pmw-btn pmw-dialog-cancel">Cancel</button><button class="pmw-btn pmw-btn-primary pmw-dialog-submit">Create project</button></div></div>` : `<div class="pmw-dialog"><div class="pmw-dialog-head"><span class="pmw-dialog-title">New ticket</span><span class="pmw-spacer"></span><button class="pmw-icon pmw-dialog-close">${ICON.close}</button></div><div class="pmw-field"><label>Project</label><select class="pmw-select pmw-new-ticket-project">${projectOptions}</select></div><div class="pmw-field"><label>Title</label><input class="pmw-input pmw-new-ticket-title" placeholder="Describe the outcome"></div><div class="pmw-field-grid"><div class="pmw-field"><label>Type</label><select class="pmw-select pmw-new-type">${TYPES.map((value) => `<option${value === 'task' ? ' selected' : ''}>${value}</option>`).join('')}</select></div><div class="pmw-field"><label>Priority</label><select class="pmw-select pmw-new-priority">${PRIORITIES.map((value) => `<option${value === 'medium' ? ' selected' : ''}>${value}</option>`).join('')}</select></div><div class="pmw-field"><label>Status</label><select class="pmw-select pmw-new-status">${STATUSES.map((value) => `<option value="${value}">${LABELS[value]}</option>`).join('')}</select></div></div><div class="pmw-field"><label>Owner</label><input class="pmw-input pmw-new-owner" placeholder="Unassigned"></div><div class="pmw-field"><label>Description</label><textarea class="pmw-textarea pmw-new-body"></textarea></div><div class="pmw-field"><label>Vault documents</label><textarea class="pmw-textarea pmw-docs pmw-new-docs" placeholder="work:Development/project/document.md"></textarea></div><div class="pmw-dialog-actions"><button class="pmw-btn pmw-dialog-cancel">Cancel</button><button class="pmw-btn pmw-btn-primary pmw-dialog-submit">Create ticket</button></div></div>`;
      const close = () => { overlay.hidden = true; overlay.innerHTML = ''; };
      overlay.querySelector('.pmw-dialog-close').onclick = close;
      overlay.querySelector('.pmw-dialog-cancel').onclick = close;
      overlay.onclick = (event) => { if (event.target === overlay) close(); };
      overlay.querySelector('.pmw-dialog-submit').onclick = async () => {
        try {
          if (kind === 'project') {
            const project = await invoke('pm_project_create', { request: { key: overlay.querySelector('.pmw-new-key').value, name: overlay.querySelector('.pmw-new-project-name').value, source_repo: overlay.querySelector('.pmw-new-source').value } });
            state.project = project.key;
          } else {
            const ticket = await invoke('pm_ticket_create', { request: { project: overlay.querySelector('.pmw-new-ticket-project').value, title: overlay.querySelector('.pmw-new-ticket-title').value, ticket_type: overlay.querySelector('.pmw-new-type').value, status: overlay.querySelector('.pmw-new-status').value, priority: overlay.querySelector('.pmw-new-priority').value, owner: overlay.querySelector('.pmw-new-owner').value.trim() || null, body: overlay.querySelector('.pmw-new-body').value, documentation: overlay.querySelector('.pmw-new-docs').value.split('\n').map((value) => value.trim()).filter(Boolean) } });
            state.selected = ticket;
          }
          close(); await load(); if (state.selected) openTicket(state.selected.id);
        } catch (error) { toast(error, true); }
      };
      setTimeout(() => overlay.querySelector('input,select,textarea')?.focus(), 0);
    }

    function showProjectDetails() {
      const project = state.projects.find((item) => item.key === state.project);
      if (!project) return;
      const client = project.client || null;
      const contacts = client && Array.isArray(client.contacts) ? client.contacts : [];
      const overlay = $('.pmw-overlay');
      overlay.hidden = false;
      overlay.innerHTML = `<div class="pmw-dialog"><div class="pmw-dialog-head"><span class="pmw-dialog-title">${esc(project.key)} - ${esc(project.name)}</span><span class="pmw-spacer"></span><button class="pmw-icon pmw-dialog-close">${ICON.close}</button></div><div class="pmw-field"><label>Local folder</label><div>${esc(project.source_path || 'Not linked')}</div></div><div class="pmw-field"><label>Forge remote</label><div>${esc(project.forge_remote || 'Not linked')}</div></div>${client ? `<div class="pmw-field"><label>Client</label><div>${esc(client.client_company)}</div></div><div class="pmw-field"><label>Scope</label><div>${esc(client.scope || 'Not specified')}</div></div><div class="pmw-field-grid"><div class="pmw-field"><label>Rate</label><div>CHF ${esc(client.rate_chf_per_hour || 0)} / hour</div></div><div class="pmw-field"><label>Offer</label><div>CHF ${esc(client.offer_amount_chf == null ? 0 : client.offer_amount_chf)}</div></div><div class="pmw-field"><label>Expected close</label><div>${esc(client.expected_close || 'Not specified')}</div></div></div>${contacts.length ? `<div class="pmw-field"><label>Contacts</label>${contacts.map((contact) => `<div>${esc(contact.name)} · ${esc(contact.role)} · ${esc(contact.email)}</div>`).join('')}</div>` : ''}` : ''}<div class="pmw-dialog-actions"><button class="pmw-btn pmw-dialog-close-action">Close</button></div></div>`;
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

    async function load() {
      const request = ++state.request;
      try {
        const projects = await invoke('pm_project_import_existing');
        const tickets = await invoke('pm_ticket_list', { project: null });
        const status = await invoke('pm_module_status');
        if (request !== state.request) return;
        state.status = status; state.projects = projects || []; state.tickets = tickets || [];
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
    $('.pmw-refresh').onclick = load;
    $('.pmw-sync').onclick = async (event) => { const button = event.currentTarget; button.disabled = true; $('.pmw-sync-state').textContent = 'Synchronizing...'; try { state.status = await invoke('pm_module_sync'); await load(); toast('Control repository synchronized'); } catch (error) { toast(error, true); paintStatus(); } finally { button.disabled = false; } };
    $('.pmw-new-project').onclick = () => showDialog('project');
    $('.pmw-new-ticket').onclick = () => state.projects.length ? showDialog('ticket') : showDialog('project');
    $('.pmw-project-details').onclick = showProjectDetails;

    load();
    const entry = { kind: 'project-management', label, pane, refresh: load };
    panes.set(label, entry);
    return entry;
  }

  function destroyPanel(label) {
    const entry = panes.get(label);
    if (!entry) return;
    entry.pane.remove();
    panes.delete(label);
  }

  window.xnautCreateProjectManagementPanel = createPanel;
  window.xnautDestroyProjectManagementPanel = destroyPanel;
})();
