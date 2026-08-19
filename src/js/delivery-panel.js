// Delivery panel — the middle of the loop (XNAUT-208).
//
// xNAUT could start work on a ticket and it could ship a version, and
// everything between the two was invisible: no test surface, no release
// surface, and the proof-of-work report existed only as a terminal script.
//
// Nothing here is new data. Test runs have been persisted per step by
// sandbox_verify.rs since the sandbox landed, and the only consumer was one
// line of text in the PM panel. Releases and commits come from git via the two
// thin commands in gitops.rs. This panel is the reader those three sources
// never had.
//
// Shape follows tasks-panel.js: IIFE, 'use strict', window.xnaut* exports,
// createDeliveryPanel(tabId, parentContainer, opts) -> entry with
// updateOptions(), so xnautAttachSingletonPanelTab can re-point a live tab.
(function () {
  'use strict';

  const invoke = (...a) => window.__TAURI__.core.invoke(...a);
  const listen = (...a) => window.__TAURI__.event.listen(...a);

  const panes = new Map();
  let labelCounter = 0;
  const nextLabel = () => `delivery-${Date.now().toString(36)}-${(labelCounter += 1)}`;

  const esc = (s) => String(s == null ? '' : s)
    .replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));

  function relativeTime(iso) {
    const t = Date.parse(iso);
    if (!Number.isFinite(t)) return '';
    const sec = Math.max(0, Math.floor((Date.now() - t) / 1000));
    for (const [name, div] of [['year', 31536000], ['month', 2592000], ['day', 86400], ['hour', 3600], ['minute', 60]]) {
      if (sec >= div) { const n = Math.floor(sec / div); return `${n} ${name}${n === 1 ? '' : 's'} ago`; }
    }
    return 'just now';
  }

  function duration(from, to) {
    const a = Date.parse(from);
    const b = Date.parse(to);
    if (!Number.isFinite(a) || !Number.isFinite(b) || b < a) return '';
    const sec = Math.round((b - a) / 1000);
    return sec < 60 ? `${sec}s` : `${Math.floor(sec / 60)}m ${sec % 60}s`;
  }

  // Conventional-commit type, or 'other'. Same grouping the proof-of-work
  // script uses, so the two reports can be compared line for line.
  const TYPES = ['feat', 'fix', 'test', 'perf', 'refactor', 'docs', 'chore'];
  const typeOf = (subject) => {
    const m = /^([a-z]+)(\([^)]*\))?!?:/.exec(String(subject));
    return m && TYPES.includes(m[1]) ? m[1] : 'other';
  };
  const isTestFile = (f) => /(^|\/)tests?\//.test(f) || /\.(test|spec)\./.test(f) || /_test\.\w+$/.test(f);

  const STATUSES = ['done', 'review', 'blocked', 'in_progress', 'ready', 'inbox'];
  const SINCE = [['2 days ago', '2 days'], ['7 days ago', '7 days'], ['30 days ago', '30 days'], ['90 days ago', '90 days']];

  function injectStyles() {
    if (document.getElementById('delivery-panel-styles')) return;
    const st = document.createElement('style');
    st.id = 'delivery-panel-styles';
    st.textContent = `
.dlv-pane { display:flex; flex-direction:column; width:100%; height:100%; min-height:0; overflow:hidden;
  background:var(--bg-primary,#14161b); color:var(--text-primary,#e7e9ee); font-size:13px; }
.dlv-head { display:flex; align-items:center; gap:8px; min-height:48px; padding:7px 12px;
  border-bottom:1px solid var(--border-color,#34363d); }
.dlv-title { font-size:14px; font-weight:650; }
.dlv-spacer { flex:1 1 auto; }
.dlv-tabs { display:flex; border:1px solid var(--border-color,#3a3d45); border-radius:6px; overflow:hidden; }
.dlv-tabs button { min-height:28px; padding:3px 12px; border:0; background:transparent;
  color:var(--text-secondary,#9a9faa); font:inherit; cursor:pointer; }
.dlv-tabs button+button { border-left:1px solid var(--border-color,#3a3d45); }
.dlv-tabs button.active { color:var(--text-primary,#fff); background:rgba(79,140,255,.17); }
.dlv-btn { min-height:30px; padding:4px 10px; border:1px solid var(--border-color,#3a3d45); border-radius:6px;
  background:transparent; color:inherit; font:inherit; cursor:pointer; }
.dlv-btn:hover:not(:disabled) { border-color:var(--accent,#4f8cff); }
.dlv-btn:disabled { opacity:.45; cursor:default; }
.dlv-btn-primary { background:var(--accent,#4f8cff); border-color:var(--accent,#4f8cff); color:#fff; }
.dlv-select { min-height:30px; padding:4px 8px; border:1px solid var(--border-color,#3a3d45); border-radius:6px;
  background:rgba(255,255,255,.05); color:inherit; font:inherit; }
.dlv-main { display:flex; flex:1 1 auto; min-height:0; overflow:hidden; }
.dlv-side { width:210px; flex:0 0 210px; border-right:1px solid var(--border-color,#34363d);
  overflow:auto; padding:8px 0; }
.dlv-side-h { padding:4px 12px 8px; font-size:10.5px; letter-spacing:.09em; text-transform:uppercase;
  color:var(--text-secondary,#8f949e); }
.dlv-proj { display:block; width:100%; text-align:left; padding:7px 12px; border:0; border-left:2px solid transparent;
  background:transparent; color:var(--text-secondary,#b6bac3); font:inherit; cursor:pointer; }
.dlv-proj:hover { background:rgba(255,255,255,.05); color:var(--text-primary,#fff); }
.dlv-proj.active { background:rgba(79,140,255,.13); border-left-color:var(--accent,#4f8cff); color:var(--text-primary,#fff); }
.dlv-proj small { display:block; color:var(--text-secondary,#7e838d); font-size:11px; }
.dlv-body { flex:1 1 auto; min-width:0; overflow:auto; padding:14px 16px 28px; }
.dlv-stats { display:flex; flex-wrap:wrap; gap:10px; margin-bottom:14px; }
.dlv-stat { flex:1 1 120px; min-width:110px; padding:9px 11px; border:1px solid var(--border-color,#2f323a);
  border-radius:8px; background:rgba(255,255,255,.025); }
.dlv-stat b { display:block; font-size:20px; font-weight:650; line-height:1.25; }
.dlv-stat span { color:var(--text-secondary,#8f949e); font-size:11px; }
.dlv-sec { margin:18px 0 8px; font-size:11px; letter-spacing:.08em; text-transform:uppercase;
  color:var(--text-secondary,#8f949e); }
.dlv-row { border:1px solid var(--border-color,#2f323a); border-radius:8px; margin-bottom:7px;
  background:rgba(255,255,255,.02); }
.dlv-row-h { display:flex; align-items:center; gap:9px; padding:9px 11px; cursor:pointer; }
.dlv-row-h b { font-weight:600; }
.dlv-row-h .dlv-spacer { min-width:8px; }
.dlv-dim { color:var(--text-secondary,#8f949e); font-size:11.5px; }
.dlv-mono { font-family:ui-monospace,SFMono-Regular,Menlo,monospace; font-size:11.5px; }
.dlv-pill { padding:1px 7px; border-radius:999px; font-size:10.5px; font-weight:600; letter-spacing:.02em; }
.dlv-passed { background:rgba(74,222,128,.16); color:#4ade80; }
.dlv-failed { background:rgba(248,113,113,.16); color:#f87171; }
.dlv-running { background:rgba(250,204,21,.16); color:#facc15; }
.dlv-cancelled,.dlv-unknown { background:rgba(255,255,255,.08); color:#9a9faa; }
.dlv-steps { padding:0 11px 11px; border-top:1px solid var(--border-color,#2a2d34); }
.dlv-step { padding:8px 0; border-bottom:1px dashed var(--border-color,#2a2d34); }
.dlv-step:last-child { border-bottom:0; }
.dlv-step-h { display:flex; align-items:center; gap:8px; }
.dlv-step pre { margin:6px 0 0; padding:8px 9px; max-height:220px; overflow:auto; white-space:pre-wrap;
  border-radius:6px; background:rgba(0,0,0,.3); color:#b6bac3; font-size:11px; line-height:1.45; }
.dlv-empty { padding:26px 4px; color:var(--text-secondary,#8f949e); }
.dlv-err { padding:10px 11px; border:1px solid rgba(248,113,113,.35); border-radius:8px;
  background:rgba(248,113,113,.08); color:#f8a5a5; margin-bottom:10px; }
.dlv-note { margin:10px 0 0; color:var(--text-secondary,#8f949e); font-size:11.5px; }
.dlv-tickets { display:flex; flex-wrap:wrap; gap:5px; margin-top:5px; }
.dlv-tag { padding:1px 6px; border-radius:4px; background:rgba(79,140,255,.14); color:#8fb4ff;
  font-size:10.5px; font-family:ui-monospace,SFMono-Regular,Menlo,monospace; }
`;
    document.head.appendChild(st);
  }

  const statusClass = (s) => `dlv-${['passed', 'failed', 'running', 'cancelled'].includes(s) ? s : 'unknown'}`;

  async function createDeliveryPanel(tabId, parentContainer, opts) {
    opts = opts || {};
    injectStyles();
    const label = nextLabel();

    const pane = document.createElement('div');
    pane.className = 'dlv-pane';
    pane.innerHTML = `
      <div class="dlv-head">
        <div class="dlv-title">Delivery</div>
        <div class="dlv-tabs">
          <button data-tab="tests">Tests</button>
          <button data-tab="releases">Releases</button>
          <button data-tab="report">Report</button>
        </div>
        <div class="dlv-spacer"></div>
        <select class="dlv-select dlv-since" title="Report window">
          ${SINCE.map(([v, l]) => `<option value="${esc(v)}">${esc(l)}</option>`).join('')}
        </select>
        <button class="dlv-btn dlv-download">Download report</button>
        <button class="dlv-btn dlv-refresh">Refresh</button>
      </div>
      <div class="dlv-main">
        <div class="dlv-side"><div class="dlv-side-h">Projects</div><div class="dlv-projects"></div></div>
        <div class="dlv-body"><div class="dlv-empty">Loading…</div></div>
      </div>`;
    parentContainer.appendChild(pane);

    const bodyEl = pane.querySelector('.dlv-body');
    const projectsEl = pane.querySelector('.dlv-projects');
    const sinceEl = pane.querySelector('.dlv-since');
    const state = {
      projects: [], keys: [], project: null, tab: opts.tab || 'tests',
      focusTicket: opts.ticket || '', since: '7 days ago',
      records: [], releases: [], commits: [], tickets: [], open: new Set(), error: '',
    };
    sinceEl.value = state.since;

    const repoOf = (p) => (p && p.source_path) || '';
    const setTab = (tab) => {
      state.tab = tab;
      pane.querySelectorAll('.dlv-tabs button').forEach((b) => b.classList.toggle('active', b.dataset.tab === tab));
      pane.querySelector('.dlv-download').hidden = tab !== 'report';
      load();
    };

    function renderProjects() {
      projectsEl.innerHTML = state.projects.length
        ? state.projects.map((p) => `
            <button class="dlv-proj${state.project && p.key === state.project.key ? ' active' : ''}" data-key="${esc(p.key)}">
              ${esc(p.name || p.key)}<small>${esc(p.key)}${p.source_path ? '' : ' · no repo path'}</small>
            </button>`).join('')
        : '<div class="dlv-empty" style="padding:12px">No projects</div>';
    }

    // ── Tests ────────────────────────────────────────────────────────────────
    function renderTests() {
      const runs = state.records
        .filter((r) => !state.project || r.project === state.project.key)
        .sort((a, b) => String(b.created_at).localeCompare(String(a.created_at)));
      const passed = runs.filter((r) => r.status === 'passed').length;
      const failed = runs.filter((r) => r.status === 'failed').length;
      const running = runs.filter((r) => r.status === 'running').length;
      const finished = passed + failed;
      const tickets = new Set(runs.map((r) => r.ticket_id).filter(Boolean));

      const stats = `
        <div class="dlv-stats">
          <div class="dlv-stat"><b>${runs.length}</b><span>runs</span></div>
          <div class="dlv-stat"><b>${finished ? Math.round((passed / finished) * 100) : 0}%</b><span>pass rate</span></div>
          <div class="dlv-stat"><b>${failed}</b><span>failed</span></div>
          <div class="dlv-stat"><b>${running}</b><span>running</span></div>
          <div class="dlv-stat"><b>${tickets.size}</b><span>tickets verified</span></div>
          <div class="dlv-stat"><b>${runs[0] ? esc(relativeTime(runs[0].created_at)) : '—'}</b><span>last run</span></div>
        </div>`;

      if (!runs.length) {
        bodyEl.innerHTML = `${stats}<div class="dlv-empty">No verification runs for this project yet. A run is started
          from a ticket, or with <span class="dlv-mono">sandbox_verify_start</span>.</div>`;
        return;
      }

      bodyEl.innerHTML = stats + '<div class="dlv-sec">Runs</div>' + runs.map((r) => {
        const open = state.open.has(r.id) || (state.focusTicket && r.ticket_id === state.focusTicket);
        const steps = r.steps || [];
        const failedStep = steps.find((s) => s.exit_code != null && s.exit_code !== 0);
        return `
          <div class="dlv-row" data-run="${esc(r.id)}">
            <div class="dlv-row-h">
              <span class="dlv-pill ${statusClass(r.status)}">${esc(r.status)}</span>
              <b>${esc(r.ticket_id || 'unlinked')}</b>
              <span class="dlv-dim">${steps.length} step${steps.length === 1 ? '' : 's'}</span>
              ${failedStep ? `<span class="dlv-dim">failed at ${esc(failedStep.name)}</span>` : ''}
              <div class="dlv-spacer"></div>
              <span class="dlv-dim">${esc(duration(r.created_at, r.updated_at))}</span>
              <span class="dlv-dim">${esc(relativeTime(r.created_at))}</span>
            </div>
            ${open ? `<div class="dlv-steps">${steps.map((s) => `
              <div class="dlv-step">
                <div class="dlv-step-h">
                  <span class="dlv-pill ${s.exit_code == null ? 'dlv-running' : (s.exit_code === 0 ? 'dlv-passed' : 'dlv-failed')}">${
                    s.exit_code == null ? 'running' : `exit ${s.exit_code}`}</span>
                  <b>${esc(s.name)}</b>
                  <span class="dlv-dim dlv-mono">${esc(s.command)}</span>
                </div>
                ${s.log_tail ? `<pre>${esc(s.log_tail)}</pre>` : ''}
              </div>`).join('')}
              ${r.error ? `<div class="dlv-err" style="margin-top:8px">${esc(r.error)}</div>` : ''}
              <div class="dlv-note">
                ${r.public_url ? `sandbox <span class="dlv-mono">${esc(r.public_url)}</span> · ` : ''}
                logs <span class="dlv-mono">${esc(r.log_dir || '—')}</span>
                ${r.video_path ? ` · video <span class="dlv-mono">${esc(r.video_path)}</span>` : ''}
              </div>
            </div>` : ''}
          </div>`;
      }).join('');

      bodyEl.querySelectorAll('.dlv-row-h').forEach((h) => {
        h.onclick = () => {
          const id = h.parentElement.dataset.run;
          if (state.open.has(id)) state.open.delete(id); else state.open.add(id);
          state.focusTicket = '';
          renderTests();
        };
      });
    }

    // ── Releases ─────────────────────────────────────────────────────────────
    function renderReleases() {
      const rel = state.releases;
      const shipped = rel.reduce((n, r) => n + r.tickets.length, 0);
      const stats = `
        <div class="dlv-stats">
          <div class="dlv-stat"><b>${rel.length}</b><span>releases</span></div>
          <div class="dlv-stat"><b>${rel[0] ? esc(rel[0].tag) : '—'}</b><span>latest</span></div>
          <div class="dlv-stat"><b>${rel[0] ? esc(rel[0].date) : '—'}</b><span>shipped</span></div>
          <div class="dlv-stat"><b>${shipped}</b><span>tickets shipped</span></div>
        </div>`;
      if (!rel.length) {
        bodyEl.innerHTML = `${stats}<div class="dlv-empty">No <span class="dlv-mono">v*</span> tags in this repository.</div>`;
        return;
      }
      bodyEl.innerHTML = stats + '<div class="dlv-sec">Releases, newest first</div>' + rel.map((r) => `
        <div class="dlv-row"><div class="dlv-row-h" style="cursor:default">
          <b class="dlv-mono">${esc(r.tag)}</b>
          <span class="dlv-dim">${esc(r.date)}</span>
          <span class="dlv-dim">${r.commits} commit${r.commits === 1 ? '' : 's'}</span>
          <div class="dlv-spacer"></div>
        </div>
        ${r.subject || r.tickets.length ? `<div class="dlv-steps" style="padding-top:9px">
          ${r.subject ? `<div class="dlv-dim">${esc(r.subject)}</div>` : ''}
          ${r.tickets.length ? `<div class="dlv-tickets">${r.tickets.map((t) => `<span class="dlv-tag">${esc(t)}</span>`).join('')}</div>` : ''}
        </div>` : ''}
        </div>`).join('');
    }

    // ── Report ───────────────────────────────────────────────────────────────
    //
    // Same joins as scripts/proof-of-work.mjs: ticket id in the commit subject
    // links the ask to the change, and the first v* tag containing that commit
    // links the change to what shipped.
    function reportModel() {
      const commits = state.commits;
      const byType = new Map();
      for (const c of commits) {
        const t = typeOf(c.subject);
        if (!byType.has(t)) byType.set(t, []);
        byType.get(t).push(c);
      }
      const evidence = new Map();
      for (const c of commits) {
        if (!c.ticket) continue;
        if (!evidence.has(c.ticket)) evidence.set(c.ticket, []);
        evidence.get(c.ticket).push(c);
      }
      const sinceIso = new Date(Date.now() - ({
        '2 days ago': 2, '7 days ago': 7, '30 days ago': 30, '90 days ago': 90,
      }[state.since] || 7) * 86400000).toISOString();
      const moved = state.tickets.filter((t) => String(t.updated_at || '') >= sinceIso);
      const files = new Set();
      let added = 0; let deleted = 0; let testCommits = 0;
      for (const c of commits) {
        added += c.added; deleted += c.deleted;
        c.files.forEach((f) => files.add(f));
        if (c.files.some(isTestFile) || typeOf(c.subject) === 'test') testCommits += 1;
      }
      const tags = new Set(commits.map((c) => c.tag).filter(Boolean));
      const unproven = moved.filter((t) => ['review', 'done'].includes(t.status) && !evidence.has(t.id));
      return { commits, byType, evidence, moved, files, added, deleted, testCommits, tags, unproven };
    }

    function renderReport() {
      const m = reportModel();
      const linked = m.commits.filter((c) => c.ticket).length;
      const stats = `
        <div class="dlv-stats">
          <div class="dlv-stat"><b>${m.commits.length}</b><span>commits</span></div>
          <div class="dlv-stat"><b>+${m.added.toLocaleString()}</b><span>lines added</span></div>
          <div class="dlv-stat"><b>-${m.deleted.toLocaleString()}</b><span>lines removed</span></div>
          <div class="dlv-stat"><b>${m.files.size}</b><span>files touched</span></div>
          <div class="dlv-stat"><b>${m.testCommits}</b><span>test commits</span></div>
          <div class="dlv-stat"><b>${m.tags.size}</b><span>releases</span></div>
          <div class="dlv-stat"><b>${linked}/${m.commits.length}</b><span>ticket-linked</span></div>
          <div class="dlv-stat"><b>${m.moved.length}</b><span>tickets moved</span></div>
        </div>`;

      const ticketSection = STATUSES.map((status) => {
        const rows = m.moved.filter((t) => t.status === status).sort((a, b) => a.id.localeCompare(b.id));
        if (!rows.length) return '';
        return `<div class="dlv-sec">${esc(status)} (${rows.length})</div>` + rows.map((t) => {
          const ev = m.evidence.get(t.id) || [];
          const tag = [...new Set(ev.map((c) => c.tag).filter(Boolean))].sort().pop();
          return `<div class="dlv-row"><div class="dlv-row-h" style="cursor:default">
            <b class="dlv-mono">${esc(t.id)}</b><span>${esc(t.title)}</span><div class="dlv-spacer"></div>
            <span class="dlv-dim">${ev.length ? `${ev.length} commit${ev.length === 1 ? '' : 's'}` : 'no commit in window'}</span>
            ${tag ? `<span class="dlv-tag">${esc(tag)}</span>` : ''}
          </div></div>`;
        }).join('');
      }).join('');

      const commitSection = [...TYPES, 'other'].map((type) => {
        const rows = m.byType.get(type) || [];
        if (!rows.length) return '';
        return `<div class="dlv-sec">${esc(type)} (${rows.length})</div>` + rows.map((c) => `
          <div class="dlv-row"><div class="dlv-row-h" style="cursor:default">
            <span class="dlv-dim dlv-mono">${esc(c.date)}</span>
            <span class="dlv-mono">${esc(c.short_sha)}</span>
            <span>${esc(c.subject)}</span>
            <div class="dlv-spacer"></div>
            <span class="dlv-dim">+${c.added}/-${c.deleted} in ${c.files.length}</span>
            ${c.ticket ? `<span class="dlv-tag">${esc(c.ticket)}</span>` : ''}
            ${c.tag ? `<span class="dlv-tag">${esc(c.tag)}</span>` : ''}
          </div></div>`).join('');
      }).join('');

      const unprovenSection = m.unproven.length ? `<div class="dlv-sec">Claimed finished, no commit found (${m.unproven.length})</div>`
        + m.unproven.map((t) => `<div class="dlv-err"><b class="dlv-mono">${esc(t.id)}</b> ${esc(t.title)}</div>`).join('') : '';

      bodyEl.innerHTML = stats + unprovenSection + ticketSection + commitSection
        + `<div class="dlv-note">Window: ${esc(state.since)}. Commits from every ref in
           <span class="dlv-mono">${esc(repoOf(state.project))}</span>. A commit is linked to a ticket by the id in its
           subject, and to a release by the first <span class="dlv-mono">v*</span> tag containing it.</div>`;
    }

    // One self-contained file: inline CSS, no assets, prints to PDF from any
    // browser. Reuses the rendered body rather than a second template, so the
    // download can never disagree with what is on screen.
    async function downloadReport() {
      const btn = pane.querySelector('.dlv-download');
      btn.disabled = true;
      try {
        const home = await invoke('get_home_directory');
        const key = state.project ? state.project.key : 'all';
        const stamp = new Date().toISOString().slice(0, 10);
        const path = `${home}/Downloads/${key}-work-report-${stamp}.html`;
        const html = `<!doctype html><html><head><meta charset="utf-8">
<title>${esc(key)} work report ${esc(stamp)}</title><style>
body { margin:0; padding:32px 40px 64px; background:#fff; color:#16181d;
  font:14px/1.55 -apple-system,BlinkMacSystemFont,"Segoe UI",Helvetica,sans-serif; }
h1 { font-size:22px; margin:0 0 4px; } .sub { color:#6b7280; margin-bottom:22px; }
.dlv-stats { display:flex; flex-wrap:wrap; gap:10px; margin-bottom:18px; }
.dlv-stat { flex:1 1 130px; padding:10px 12px; border:1px solid #e3e5ea; border-radius:8px; }
.dlv-stat b { display:block; font-size:21px; } .dlv-stat span { color:#6b7280; font-size:11px; }
.dlv-sec { margin:20px 0 8px; font-size:11px; letter-spacing:.08em; text-transform:uppercase; color:#6b7280; }
.dlv-row { border:1px solid #e3e5ea; border-radius:8px; margin-bottom:6px; }
.dlv-row-h { display:flex; align-items:center; gap:9px; padding:8px 11px; }
.dlv-spacer { flex:1 1 auto; } .dlv-dim { color:#6b7280; font-size:11.5px; }
.dlv-mono { font-family:ui-monospace,SFMono-Regular,Menlo,monospace; font-size:11.5px; }
.dlv-tag { padding:1px 6px; border-radius:4px; background:#eef2ff; color:#3f5bd9; font-size:10.5px;
  font-family:ui-monospace,SFMono-Regular,Menlo,monospace; }
.dlv-err { padding:9px 11px; border:1px solid #f3c2c2; background:#fdf2f2; border-radius:8px; margin-bottom:8px; }
.dlv-note { margin-top:18px; color:#6b7280; font-size:11.5px; }
@media print { body { padding:0 } .dlv-row { break-inside:avoid } }
</style></head><body>
<h1>${esc(state.project ? (state.project.name || key) : 'All projects')} — work report</h1>
<div class="sub">${esc(state.since)} · generated ${esc(new Date().toString())}</div>
${bodyEl.innerHTML}
</body></html>`;
        await invoke('write_file', { path, content: html });
        btn.textContent = 'Saved to Downloads';
        setTimeout(() => { btn.textContent = 'Download report'; }, 4000);
      } catch (e) {
        state.error = `Download failed: ${e}`;
        render();
      } finally {
        btn.disabled = false;
      }
    }

    function render() {
      renderProjects();
      const err = state.error ? `<div class="dlv-err">${esc(state.error)}</div>` : '';
      if (state.tab === 'tests') renderTests();
      else if (state.tab === 'releases') renderReleases();
      else renderReport();
      if (err) bodyEl.insertAdjacentHTML('afterbegin', err);
      pane.querySelectorAll('.dlv-proj').forEach((b) => {
        b.onclick = () => {
          state.project = state.projects.find((p) => p.key === b.dataset.key) || null;
          state.open.clear();
          load();
        };
      });
    }

    async function load() {
      state.error = '';
      const repo = repoOf(state.project);
      try {
        if (state.tab === 'tests') {
          state.records = await invoke('sandbox_verify_records');
        } else if (state.tab === 'releases') {
          state.releases = repo ? await invoke('git_release_history', { repo, keys: state.keys }) : [];
          if (!repo) state.error = 'This project has no local repo path set, so there is nothing to read tags from.';
        } else {
          if (!repo) {
            state.commits = [];
            state.error = 'This project has no local repo path set, so there is nothing to read commits from.';
          } else {
            state.commits = await invoke('git_commit_log', { repo, since: state.since, limit: 2000, keys: state.keys });
          }
          state.tickets = state.project
            ? await invoke('pm_ticket_list', { project: state.project.key })
            : await invoke('pm_ticket_list', {});
        }
      } catch (e) {
        state.error = String(e && e.message ? e.message : e);
      }
      render();
    }

    // Live test progress: sandbox_verify emits the whole record per step.
    const unlisten = await listen('sandbox-verify-changed', (evt) => {
      const rec = evt && evt.payload;
      if (!rec || !rec.id) return;
      const at = state.records.findIndex((r) => r.id === rec.id);
      if (at >= 0) state.records[at] = rec; else state.records.unshift(rec);
      if (state.tab === 'tests') renderTests();
    }).catch(() => null);

    pane.querySelectorAll('.dlv-tabs button').forEach((b) => { b.onclick = () => setTab(b.dataset.tab); });
    sinceEl.onchange = () => { state.since = sinceEl.value; if (state.tab === 'report') load(); };
    pane.querySelector('.dlv-refresh').onclick = load;
    pane.querySelector('.dlv-download').onclick = downloadReport;

    try {
      state.projects = await invoke('pm_project_list');
      state.keys = state.projects.map((p) => p.key);
      state.project = state.projects.find((p) => p.key === opts.project)
        || state.projects.find((p) => repoOf(p)) || state.projects[0] || null;
    } catch (e) {
      state.error = String(e);
    }
    setTab(state.tab);

    const entry = {
      kind: 'delivery',
      label,
      pane,
      updateOptions(next) {
        next = next || {};
        if (next.project) {
          const found = state.projects.find((p) => p.key === next.project);
          if (found) state.project = found;
        }
        if (next.ticket) { state.focusTicket = next.ticket; state.open.clear(); }
        setTab(next.tab || state.tab);
      },
      destroy() {
        if (typeof unlisten === 'function') unlisten();
        if (pane.parentNode) pane.parentNode.removeChild(pane);
        panes.delete(label);
      },
    };
    panes.set(label, entry);
    return entry;
  }

  function destroyDeliveryPanel(label) {
    const entry = panes.get(label);
    if (entry) entry.destroy();
  }

  // Exported for scripts/delivery-report-check.cjs: the report's joins are the
  // only logic here that can be quietly wrong rather than visibly broken.
  window.xnautDeliveryInternals = { typeOf, isTestFile, duration };

  window.xnautCreateDeliveryPanel = createDeliveryPanel;
  window.xnautDestroyDeliveryPanel = destroyDeliveryPanel;
})();
