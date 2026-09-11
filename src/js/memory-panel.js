// Memory view (XNAUT-333). What xNAUT remembers, as memory rather than as a
// folder of files.
//
// XNAUT-331 gave the machine a memory: every handback, jury fix and incident
// becomes a note under `work/<project>/Memory/`, indexed in
// `work/_memory/index.jsonl`, read by agents through `memory_search` and
// `memory_read`, and recalled at the top of every dispatch prompt. The owner
// asked where he could see it and the honest answer was "in Obsidian, or on
// disk". This is the view.
//
// The search box does not filter the list it was handed. It calls
// `memory_find_cmd`, which is the same `find()` an agent's `memory_search`
// runs, with the same ranking and the same limit semantics. So what the owner
// types returns what an agent would have been given for the same words, and
// there is no second search in the app to drift from the first one.
//
// Read only in this first cut. No editing, no deleting.
//
// Shape follows skills-panel.js: IIFE, 'use strict', createMemoryPanel(tabId,
// parent, opts) -> entry, and a window.xnautOpenMemoryPanel() that opens it as
// a singleton tab. The Delivery panel's Memory tab calls that global, so it
// takes no required argument.
(function () {
  'use strict';

  const invoke = (...args) => window.__TAURI__.core.invoke(...args);

  const esc = (value) => String(value == null ? '' : value).replace(/[&<>"']/g, (c) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  }[c]));

  // The five kinds memory.rs writes (memory.rs:29, KINDS).
  const KIND_COLOR = {
    learning: '#4f9cf5',
    fix: '#3fb950',
    incident: '#f85149',
    decision: '#c678dd',
    ask: '#f5b840',
  };

  function ago(ms) {
    const t = Number(ms);
    if (!Number.isFinite(t) || t <= 0) return '';
    const sec = Math.max(0, Math.floor((Date.now() - t) / 1000));
    for (const [name, div] of [['year', 31536000], ['month', 2592000], ['day', 86400], ['hour', 3600], ['minute', 60]]) {
      if (sec >= div) { const n = Math.floor(sec / div); return `${n} ${name}${n === 1 ? '' : 's'} ago`; }
    }
    return 'just now';
  }

  function ensureStyles() {
    if (document.getElementById('memory-panel-styles')) return;
    const style = document.createElement('style');
    style.id = 'memory-panel-styles';
    style.textContent = `
      .mem { display:flex; flex:1 1 auto; flex-direction:column; min-width:0; min-height:0; overflow:hidden;
        color:var(--text-primary,#e0e0e0); background:var(--bg-primary,#0a0a0f);
        font-family:var(--font-ui,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif); font-size:12px; }
      .mem * { box-sizing:border-box; }
      .mem-top { display:flex; align-items:center; flex-wrap:wrap; gap:8px 16px; padding:12px 16px;
        border-bottom:1px solid var(--border,#2a2a2f); background:var(--bg-secondary,#141419); }
      .mem-count { font-size:18px; font-weight:650; }
      .mem-count small { margin-left:6px; font-size:10px; font-weight:600; letter-spacing:.12em;
        text-transform:uppercase; color:var(--text-secondary,#7a7a84); }
      .mem-tally { display:flex; align-items:center; flex-wrap:wrap; gap:6px; }
      .mem-chip { display:inline-flex; align-items:center; gap:5px; padding:2px 9px; border-radius:999px;
        border:1px solid var(--border,#2a2a2f); font-size:10px; color:var(--text-secondary,#a0a0a0); }
      .mem-chip b { color:var(--text-primary,#e0e0e0); font-weight:650; }
      .mem-dot { width:7px; height:7px; border-radius:50%; background:#5a5a62; flex:0 0 auto; }
      .mem-sync { margin-left:auto; text-align:right; font-size:10px; color:var(--text-secondary,#7a7a84); }
      .mem-sync code { font-family:var(--font-mono,monospace); font-size:9px; color:#5a5a62; }
      .mem-body { display:flex; flex:1 1 auto; min-height:0; }
      .mem-left { display:flex; flex:0 0 330px; flex-direction:column; min-height:0;
        border-right:1px solid var(--border,#2a2a2f); background:var(--bg-secondary,#141419); }
      .mem-search { display:flex; gap:6px; padding:10px 12px; border-bottom:1px solid #1c1c22; }
      .mem-q, .mem-proj { padding:6px 9px; border:1px solid var(--border,#2a2a2f); border-radius:7px;
        background:var(--bg-primary,#0a0a0f); color:var(--text-primary,#e0e0e0); font:inherit; font-size:11px; }
      .mem-q { flex:1 1 auto; min-width:0; }
      .mem-proj { flex:0 0 auto; max-width:120px; }
      .mem-hint { padding:6px 12px; border-bottom:1px solid #1c1c22; font-size:10px; color:#6a6a74; }
      .mem-list { flex:1 1 auto; min-height:0; overflow-y:auto; }
      .mem-row { display:flex; flex-direction:column; gap:4px; padding:10px 12px; border-top:1px solid #1c1c22; cursor:pointer; }
      .mem-row:hover { background:rgba(255,255,255,.02); }
      .mem-row.on { background:#16161c; border-left:2px solid #4f9cf5; }
      .mem-row-top { display:flex; align-items:center; gap:7px; min-width:0; }
      .mem-kind { flex:0 0 auto; padding:1px 7px; border-radius:999px; font-size:9px; font-weight:700;
        letter-spacing:.06em; text-transform:uppercase; color:#0a0a0f; background:#5a5a62; }
      .mem-title { flex:1 1 auto; min-width:0; font-size:12px; font-weight:600; overflow:hidden;
        text-overflow:ellipsis; white-space:nowrap; }
      .mem-meta { display:flex; align-items:center; flex-wrap:wrap; gap:8px; font-size:10px; color:#7a7a84; }
      .mem-meta b { color:var(--text-secondary,#a0a0a0); font-weight:600; }
      .mem-files { font-family:var(--font-mono,monospace); font-size:9.5px; color:#5a5a62;
        overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
      .mem-right { display:flex; flex:1 1 auto; flex-direction:column; min-width:0; min-height:0; }
      .mem-tabs { display:flex; gap:2px; padding:8px 18px 0; border-bottom:1px solid var(--border,#2a2a2f); }
      .mem-tabs button { padding:6px 13px; border:0; border-bottom:2px solid transparent; background:transparent;
        color:var(--text-secondary,#7a7a84); font:inherit; font-size:11px; cursor:pointer; }
      .mem-tabs button.on { color:var(--text-primary,#e0e0e0); border-bottom-color:#4f9cf5; }
      .mem-detail { flex:1 1 auto; min-height:0; overflow-y:auto; padding:18px 22px; }
      .mem-h { font-size:17px; font-weight:620; line-height:1.35; }
      .mem-sub { display:flex; align-items:center; flex-wrap:wrap; gap:8px; margin-top:8px; }
      .mem-sec { margin-top:18px; }
      .mem-sec > h4 { margin:0 0 5px; font-size:9.5px; font-weight:700; letter-spacing:.12em;
        text-transform:uppercase; color:#6a6a74; }
      .mem-sec p { margin:0; font-size:12.5px; line-height:1.6; white-space:pre-wrap; }
      .mem-sec ul { margin:0; padding-left:16px; }
      .mem-sec li { font-family:var(--font-mono,monospace); font-size:11px; line-height:1.7; color:var(--text-secondary,#a0a0a0); }
      .mem-actions { display:flex; gap:8px; margin-top:20px; }
      .mem-btn { padding:5px 12px; border:1px solid var(--border,#2a2a2f); border-radius:7px; background:transparent;
        color:var(--text-secondary,#a0a0a0); font:inherit; font-size:11px; cursor:pointer; }
      .mem-btn:hover { color:var(--text-primary,#e0e0e0); }
      .mem-recall-bar { display:flex; align-items:center; gap:8px; margin-bottom:14px; }
      .mem-recall-bar input { flex:0 0 190px; padding:6px 9px; border:1px solid var(--border,#2a2a2f); border-radius:7px;
        background:var(--bg-secondary,#141419); color:var(--text-primary,#e0e0e0); font:inherit; font-size:11px; }
      .mem-recall { margin:0; padding:14px 16px; border:1px solid var(--border,#2a2a2f); border-radius:9px;
        background:var(--bg-secondary,#141419); font-family:var(--font-mono,monospace); font-size:11px;
        line-height:1.65; white-space:pre-wrap; color:var(--text-primary,#e0e0e0); }
      .mem-empty { padding:40px 30px; text-align:center; font-size:12px; line-height:1.7; color:var(--text-secondary,#a0a0a0); }
      .mem-err { margin:14px 0 0; padding:10px 12px; border:1px solid #5a2020; border-radius:8px;
        background:#1c1113; font-size:11px; color:#f8a8a2; }
    `;
    document.head.appendChild(style);
  }

  const kindChip = (kind) =>
    `<span class="mem-kind" style="background:${esc(KIND_COLOR[kind] || '#5a5a62')}">${esc(kind || 'note')}</span>`;

  async function createMemoryPanel(tabId, parent, opts) {
    ensureStyles();
    opts = opts || {};

    const pane = document.createElement('section');
    pane.className = 'mem';
    pane.innerHTML = `
      <header class="mem-top"></header>
      <div class="mem-body">
        <div class="mem-left">
          <div class="mem-search">
            <input class="mem-q" type="search" placeholder="Search the memory" aria-label="Search the memory">
            <select class="mem-proj" aria-label="Filter by project"><option value="">All projects</option></select>
          </div>
          <div class="mem-hint">This is the search the agents run, not a second one.</div>
          <div class="mem-list"></div>
        </div>
        <div class="mem-right">
          <div class="mem-tabs">
            <button data-view="note" class="on">Note</button>
            <button data-view="recall">Recall</button>
          </div>
          <div class="mem-detail"></div>
        </div>
      </div>`;
    parent.appendChild(pane);

    const $ = (sel) => pane.querySelector(sel);
    const listEl = $('.mem-list');
    const detailEl = $('.mem-detail');
    const queryEl = $('.mem-q');
    const projEl = $('.mem-proj');

    const state = {
      index: [],
      rows: [],
      stats: null,
      query: '',
      project: opts.project || '',
      selected: '',
      note: null,
      view: 'note',
      recallTicket: opts.ticket || '',
      recall: null,
      recallFor: '',
      error: '',
      noteError: '',
      recallError: '',
    };

    // ── Top: what is in the memory, and whether the other machine has seen it ──
    function renderTop() {
      const stats = state.stats;
      if (!stats) { $('.mem-top').innerHTML = ''; return; }
      const tally = (pairs) => (pairs || []).map(([name, n]) =>
        `<span class="mem-chip"><i class="mem-dot" style="background:${esc(KIND_COLOR[name] || '#5a5a62')}"></i>${esc(name)} <b>${esc(n)}</b></span>`).join('');
      const projects = (stats.by_project || []).map(([name, n]) =>
        `<span class="mem-chip">${esc(name)} <b>${esc(n)}</b></span>`).join('');
      const sync = stats.last_sync
        ? `Last sync ${esc(ago(Date.parse(stats.last_sync)) || stats.last_sync)}`
        : (stats.remote ? 'Never synced with the other machine.' : 'This vault has no remote, so nothing syncs.');
      $('.mem-top').innerHTML = `
        <span class="mem-count">${esc(stats.notes)}<small>memories</small></span>
        <span class="mem-tally">${tally(stats.by_kind)}</span>
        <span class="mem-tally">${projects}</span>
        <span class="mem-sync">${sync}<br><code>${esc(stats.root || '')}</code></span>`;
    }

    function renderProjects() {
      const names = (state.stats && state.stats.by_project ? state.stats.by_project.map(([n]) => n) : [])
        .filter((n) => n);
      projEl.innerHTML = `<option value="">All projects</option>`
        + names.map((n) => `<option value="${esc(n)}">${esc(n)}</option>`).join('');
      projEl.value = state.project;
    }

    // ── Left: the index, newest first, or whatever the agents' search returned ──
    function renderList() {
      if (state.error) {
        listEl.innerHTML = `<div class="mem-err">${esc(state.error)}</div>`;
        return;
      }
      if (!state.rows.length) {
        listEl.innerHTML = `<div class="mem-empty">${state.index.length
          ? 'Nothing in the memory matches that. An agent searching the same words would also come back empty.'
          : 'xNAUT has not remembered anything yet. Notes appear here as handbacks, jury fixes and incidents happen.'}</div>`;
        return;
      }
      listEl.innerHTML = state.rows.map((e) => {
        const files = (e.files || []).join('  ');
        return `<div class="mem-row${e.note === state.selected ? ' on' : ''}" data-note="${esc(e.note)}">
          <span class="mem-row-top">${kindChip(e.kind)}<span class="mem-title">${esc(e.title || e.note)}</span></span>
          <span class="mem-meta">${e.project ? `<b>${esc(e.project)}</b>` : ''}${e.ticket ? `<span>${esc(e.ticket)}</span>` : ''}<span>${esc(ago(e.at))}</span></span>
          ${files ? `<span class="mem-files">${esc(files)}</span>` : ''}
        </div>`;
      }).join('');
      listEl.querySelectorAll('.mem-row').forEach((row) => {
        row.onclick = () => select(row.dataset.note);
      });
    }

    // ── Right, Note: the selected memory as it was written ──
    function noteMarkup() {
      if (state.noteError) return `<div class="mem-err">${esc(state.noteError)}</div>`;
      const m = state.note;
      if (!m) {
        return `<div class="mem-empty">${state.index.length
          ? 'Pick a memory on the left to read what it says.'
          : 'There is nothing to read yet.'}</div>`;
      }
      const first = String(m.text || '').split('\n')[0] || m.path;
      const sec = (label, body) => body
        ? `<div class="mem-sec"><h4>${esc(label)}</h4><p>${esc(body)}</p></div>` : '';
      const files = (m.files || []).length
        ? `<div class="mem-sec"><h4>Files</h4><ul>${m.files.map((f) => `<li>${esc(f)}</li>`).join('')}</ul></div>` : '';
      const buttons = [];
      if (m.ticket) buttons.push(`<button class="mem-btn" data-open-ticket>Open ${esc(m.ticket)}</button>`);
      if (m.run_id) buttons.push(`<button class="mem-btn" data-open-run>Open run in Observatory</button>`);
      return `
        <div class="mem-h">${esc(first)}</div>
        <div class="mem-sub">${kindChip(m.kind)}
          ${m.project ? `<span class="mem-chip">${esc(m.project)}</span>` : ''}
          ${m.ticket ? `<span class="mem-chip">${esc(m.ticket)}</span>` : ''}
          <span class="mem-chip">${esc(ago(m.at))}</span></div>
        ${sec('Cause', m.cause)}
        ${sec('Fix', m.fix)}
        ${sec('What happened', m.text)}
        ${files}
        ${sec('Source', m.source)}
        ${sec('Run', m.run_id)}
        <div class="mem-sec"><h4>Note</h4><ul><li>${esc(m.path)}</li></ul></div>
        ${buttons.length ? `<div class="mem-actions">${buttons.join('')}</div>` : ''}`;
    }

    // ── Right, Recall: what the agent was told before it started ──
    //
    // This is the whole point of the view. `memory_recall_for_ticket` returns
    // the block memory.rs builds for a dispatch, so the owner reads the same
    // text the agent read, rather than a summary of it.
    function recallMarkup() {
      const bar = `<div class="mem-recall-bar">
        <input class="mem-recall-t" type="text" placeholder="XNAUT-333" value="${esc(state.recallTicket)}" aria-label="Ticket">
        <button class="mem-btn" data-recall>Show recall block</button>
      </div>`;
      let body;
      if (state.recallError) {
        body = `<div class="mem-err">${esc(state.recallError)}</div>`;
      } else if (state.recall == null) {
        body = `<div class="mem-empty">Name a ticket to see the memories its dispatch prompt carried.</div>`;
      } else if (!String(state.recall).trim()) {
        body = `<div class="mem-empty">xNAUT remembers nothing about ${esc(state.recallFor)} yet, so its dispatch prompt carried no recall block at all.</div>`;
      } else {
        body = `<pre class="mem-recall" data-recall-block>${esc(state.recall)}</pre>`;
      }
      return bar + body;
    }

    function renderRight() {
      pane.querySelectorAll('.mem-tabs button').forEach((b) => b.classList.toggle('on', b.dataset.view === state.view));
      detailEl.innerHTML = state.view === 'recall' ? recallMarkup() : noteMarkup();

      const ticketBtn = detailEl.querySelector('[data-open-ticket]');
      if (ticketBtn) ticketBtn.onclick = () => {
        // The path every other panel already uses to open a ticket
        // (right-pane-workspace.js:1000). Assigned in tasks-mode-glue.js.
        if (typeof window.xnautOpenDelivery !== 'function') return;
        window.xnautOpenDelivery({ project: state.note.project || '', ticket: state.note.ticket });
      };
      const runBtn = detailEl.querySelector('[data-open-run]');
      if (runBtn) runBtn.onclick = () => {
        // Runs live in the Observatory; nothing in the app focuses a single
        // run yet, so this opens the surface that lists them.
        if (typeof window.xnautAttachObservatoryTab !== 'function') return;
        window.xnautAttachObservatoryTab();
      };
      const recallBtn = detailEl.querySelector('[data-recall]');
      if (recallBtn) recallBtn.onclick = () => {
        const input = detailEl.querySelector('.mem-recall-t');
        state.recallTicket = input ? input.value.trim() : state.recallTicket;
        loadRecall();
      };
    }

    function render() { renderTop(); renderList(); renderRight(); }

    // ── Data ────────────────────────────────────────────────────────────────

    async function select(note) {
      state.selected = note;
      state.note = null;
      state.noteError = '';
      state.view = 'note';
      renderList();
      renderRight();
      try {
        state.note = await invoke('memory_note_read', { note });
        if (state.note && state.note.ticket && !state.recallTicket) state.recallTicket = state.note.ticket;
      } catch (e) {
        state.noteError = String(e && e.message ? e.message : e);
      }
      renderRight();
    }

    async function loadRecall() {
      const ticket = String(state.recallTicket || '').trim();
      state.recallError = '';
      if (!ticket) { state.recall = null; renderRight(); return; }
      try {
        state.recall = await invoke('memory_recall_for_ticket', { ticket, files: null });
        state.recallFor = ticket;
      } catch (e) {
        state.recall = null;
        state.recallError = String(e && e.message ? e.message : e);
      }
      renderRight();
    }

    // Empty query is the index itself, filtered by project. A non-empty query
    // goes to the backend, because `find()` there is what an agent gets.
    async function search() {
      const q = state.query.trim();
      try {
        if (!q) {
          state.rows = state.project
            ? state.index.filter((e) => String(e.project).toLowerCase() === state.project.toLowerCase())
            : state.index.slice();
        } else {
          state.rows = (await invoke('memory_find_cmd', {
            query: q, project: state.project || null, limit: 200,
          })) || [];
        }
        state.error = '';
      } catch (e) {
        state.rows = [];
        state.error = String(e && e.message ? e.message : e);
      }
      renderList();
    }

    async function load() {
      try {
        const [stats, index] = await Promise.all([invoke('memory_stats'), invoke('memory_index_list')]);
        state.stats = stats || null;
        state.index = index || [];
        state.error = '';
      } catch (e) {
        state.stats = null;
        state.index = [];
        state.error = String(e && e.message ? e.message : e);
      }
      renderProjects();
      await search();
      renderTop();
      renderRight();
      if (state.recallTicket) { state.view = 'recall'; await loadRecall(); }
    }

    let debounce = 0;
    queryEl.oninput = () => {
      state.query = queryEl.value;
      window.clearTimeout(debounce);
      debounce = window.setTimeout(() => search(), 150);
    };
    projEl.onchange = () => { state.project = projEl.value; search(); };
    pane.querySelectorAll('.mem-tabs button').forEach((b) => {
      b.onclick = () => { state.view = b.dataset.view; renderRight(); };
    });

    render();
    await load();

    return {
      kind: 'memory',
      label: `memory-${tabId}`,
      pane,
      refresh: load,
      updateOptions(next) {
        next = next || {};
        if (next.project) { state.project = next.project; projEl.value = next.project; search(); }
        if (next.ticket) { state.recallTicket = next.ticket; state.view = 'recall'; loadRecall(); }
        else renderRight();
      },
      destroy() { if (pane.parentNode) pane.parentNode.removeChild(pane); },
    };
  }

  window.xnautCreateMemoryPanel = createMemoryPanel;

  // The Delivery panel's Memory tab and the sidebar both call this. It takes
  // no required argument on purpose: opts is a courtesy, never a requirement.
  window.xnautOpenMemoryPanel = (opts) => {
    if (window.xnautHomeContext) window.xnautHomeContext();
    return window.xnautAttachSingletonPanelTab('Memory', 'xnautCreateMemoryPanel', opts || {});
  };
})();
