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

  const STATUSES = ['complete', 'done', 'review', 'blocked', 'in_progress', 'ready', 'inbox'];
  const SINCE = [['2 days ago', '2 days'], ['7 days ago', '7 days'], ['30 days ago', '30 days'], ['90 days ago', '90 days']];

  // ── Evidence view model ──────────────────────────────────────────────────
  //
  // Pure functions, exported for scripts/evidence-view-check.cjs. The Evidence
  // tab's failure mode is not a thrown error, it is a confident sentence over
  // data that does not support it, and only a test can catch that.

  const DAY_MS = 86400000;
  const dayOf = (iso) => {
    const t = Date.parse(iso);
    if (!Number.isFinite(t)) return '';
    const d = new Date(t);
    // Local day, not the UTC prefix of the string: "what did my agents do
    // today" is asked in the reader's timezone, and slicing the ISO string
    // would file a 01:00 CEST record under yesterday.
    const pad = (n) => String(n).padStart(2, '0');
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
  };

  function dayLabel(dayKey, now) {
    if (!dayKey) return 'Undated';
    const today = dayOf(new Date(now == null ? Date.now() : now).toISOString());
    if (dayKey === today) return 'Today';
    const [y, m, d] = dayKey.split('-').map(Number);
    const then = new Date(y, m - 1, d).getTime();
    const [ty, tm, td] = today.split('-').map(Number);
    if (Math.round((new Date(ty, tm - 1, td).getTime() - then) / DAY_MS) === 1) return 'Yesterday';
    return new Date(y, m - 1, d).toLocaleDateString(undefined,
      { weekday: 'long', day: 'numeric', month: 'long' });
  }

  // Sessions bucketed by the day they last did anything, newest day first.
  function groupByDay(sessions) {
    const buckets = new Map();
    for (const s of sessions || []) {
      const key = dayOf(s.last_at);
      if (!buckets.has(key)) buckets.set(key, []);
      buckets.get(key).push(s);
    }
    return [...buckets.entries()].sort((a, b) => String(b[0]).localeCompare(String(a[0])));
  }

  // The verdict the integrity banner renders.
  //
  // This function is the whole reason the Evidence tab may be trusted, so it
  // is deliberately unable to say "verified" on its own initiative: `ok` is
  // computed in evidence.rs by actually re-hashing every record and following
  // every prev_hash, and nothing here can synthesise it. The three non-ok
  // tones exist so that "no file", "empty file" and "broken chain" can never
  // be rendered as the same reassuring tick.
  function integrityVerdict(report) {
    if (!report) {
      return { tone: 'unknown', headline: 'Integrity not checked', detail: [], path: '' };
    }
    const path = report.path || '';
    if (report.ok) {
      return {
        tone: 'passed',
        headline: `Chain intact: ${report.records} records across ${report.sessions} sessions`,
        detail: report.checked || [],
        path,
      };
    }
    if (report.broken) {
      return {
        tone: 'failed',
        headline: 'Chain broken',
        detail: [report.broken, `Records that verified before the break: ${report.records}`],
        path,
      };
    }
    // Present but empty, or absent. Never a tick: there is nothing here that
    // was checked, and saying otherwise is the bug this panel was built to
    // stop repeating.
    return {
      tone: 'unknown',
      headline: report.exists ? 'Nothing recorded yet' : 'No evidence log on this machine',
      detail: [report.exists
        ? 'The log exists and holds no records, so there is nothing to verify.'
        : 'No agent has recorded an action yet, so the file has never been written.'],
      path,
    };
  }

  const clockOf = (iso) => {
    const t = Date.parse(iso);
    if (!Number.isFinite(t)) return '--:--';
    const d = new Date(t);
    return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
  };

  const clockRange = (from, to) => {
    const a = clockOf(from);
    const b = clockOf(to);
    if (a === '--:--') return '';
    // A session that ran past midnight must not render as "23:47-13:46",
    // which reads as ten hours backwards. The real log has one: nautbot's
    // session opened on 27 August and was still recording on the 31st.
    if (dayOf(from) !== dayOf(to)) {
      const d = new Date(Date.parse(from));
      return `${d.toLocaleDateString(undefined, { day: 'numeric', month: 'short' })} ${a} to ${b}`;
    }
    return a === b ? a : `${a}-${b}`;
  };

  // The one line a record shows. Real text, or an explicit reason there is
  // none; never the args hash standing in for the command.
  function recordLine(r) {
    if (r.summary) return r.summary;
    if (r.args_error) return `arguments unavailable: ${r.args_error}`;
    if (r.model) return r.model;
    return r.tool || r.kind || '';
  }

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
.dlv-integrity { margin-bottom:12px; }
.dlv-i-passed { border-color:rgba(74,222,128,.42); background:rgba(74,222,128,.06); }
.dlv-i-failed { border-color:rgba(248,113,113,.5); background:rgba(248,113,113,.07); }
.dlv-i-unknown { border-color:rgba(255,255,255,.16); background:rgba(255,255,255,.02); }
.dlv-checked { margin:0; padding-left:17px; color:var(--text-secondary,#8f949e); font-size:11.5px; line-height:1.7; }
.dlv-caret { width:10px; color:var(--text-secondary,#8f949e); }
.dlv-sess-h { cursor:pointer; }
.dlv-cmd { font-weight:500; color:#dfe3ea; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; min-width:0; }
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
          <button data-tab="evidence">Evidence</button>
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
      sessions: [], kek: '',
      // Evidence: the verify report as evidence.rs returned it, the records
      // read per expanded session, and which sessions are expanded.
      // NOT `records`: that key is the Tests tab's run list, and a second one
      // here would silently shadow it and empty the Tests tab.
      verify: null, sessionRecords: {}, openSessions: new Set(),
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

    // ── Evidence ─────────────────────────────────────────────────────────────
    //
    // Two jobs, and until now the tab only did the second.
    //
    // Read: every tool and model call an agent made is already recorded, with
    // the arguments stored in full beside the log. Nothing surfaced it. The tab
    // listed session UUIDs and record counts, which answers "which run" and
    // never "what happened"; `evidence_arguments` shipped permitted and had
    // zero callers, so the full command text was on disk and unreachable.
    //
    // Control: delete a session's key and its arguments are gone for everyone,
    // us included, while the records stay and still verify. That part worked
    // and is kept unchanged below.
    function renderEvidence() {
      const rows = state.sessions;
      const sealed = rows.filter((r) => r.sealed).length;
      const shredded = rows.filter((r) => r.shredded).length;
      const keks = [...new Set(rows.filter((r) => r.kek).map((r) => r.kek))];
      const refused = rows.reduce((n, r) => n + (r.refused || 0), 0);
      const agents = [...new Set(rows.flatMap((r) => r.agents || []))];
      const verdict = integrityVerdict(state.verify);

      // The banner is rendered from the report and nothing else. There is no
      // branch here that can print a tick without evidence.rs having walked
      // the chain first.
      const banner = `
        <div class="dlv-row dlv-integrity dlv-i-${verdict.tone}">
          <div class="dlv-row-h" style="cursor:default">
            <span class="dlv-pill dlv-${verdict.tone}">${verdict.tone === 'passed' ? 'verified' : verdict.tone === 'failed' ? 'broken' : 'unverified'}</span>
            <b>${esc(verdict.headline)}</b>
            <div class="dlv-spacer"></div>
            <button class="dlv-btn dlv-reverify">Re-verify</button>
          </div>
          <div class="dlv-steps" style="padding-top:9px">
            ${verdict.detail.length
              ? `<ul class="dlv-checked">${verdict.detail.map((d) => `<li>${esc(d)}</li>`).join('')}</ul>`
              : ''}
            <div class="dlv-note" style="margin-top:6px">Read from <span class="dlv-mono">${esc(verdict.path || 'no path reported')}</span></div>
          </div>
        </div>`;

      const stats = `
        <div class="dlv-stats">
          <div class="dlv-stat"><b>${rows.length}</b><span>sessions</span></div>
          <div class="dlv-stat"><b>${rows.reduce((n, r) => n + r.records, 0)}</b><span>records</span></div>
          <div class="dlv-stat"><b>${esc(agents.join(', ') || '—')}</b><span>agents</span></div>
          <div class="dlv-stat"><b>${refused}</b><span>refused</span></div>
          <div class="dlv-stat"><b>${sealed}</b><span>sealed</span></div>
          <div class="dlv-stat"><b>${shredded}</b><span>shredded</span></div>
        </div>
        <div class="dlv-row"><div class="dlv-row-h" style="cursor:default">
          <span class="dlv-dim">Key encryption key</span>
          <b class="dlv-mono">${esc(keks.join(', ') || state.kek || 'none in use')}</b>
          <div class="dlv-spacer"></div>
          ${keks.length > 1 ? '<span class="dlv-pill dlv-running">rotation unfinished</span>' : ''}
          <button class="dlv-btn dlv-rotate"${sealed ? '' : ' disabled'}>Rotate…</button>
        </div></div>`;

      // The empty state names the file it looked at, and says which of the two
      // empty cases this is. "No execution records yet." sent a person looking
      // for a bug in the recorder when the answer was a path they could stat.
      const body = rows.length
        ? groupByDay(rows).map(([day, group]) => `
            <div class="dlv-sec">${esc(dayLabel(day))} · ${group.length} session${group.length === 1 ? '' : 's'}</div>
            ${group.map(sessionRow).join('')}`).join('')
        : `<div class="dlv-empty">
             ${esc(verdict.headline)}.
             <div style="margin-top:8px">Looked in <span class="dlv-mono">${esc(verdict.path || 'no path reported')}</span></div>
             <div style="margin-top:8px">Records are written by the agent hook with no action from the agent;
               once one runs a tool, its session appears here.</div>
           </div>`;

      bodyEl.innerHTML = banner + stats + body;
      bodyEl.querySelectorAll('.dlv-shred').forEach((b) => { b.onclick = () => shred(b); });
      bodyEl.querySelectorAll('.dlv-sess-h').forEach((h) => {
        h.onclick = (e) => {
          if (e.target.closest('button')) return;   // Shred is not an expander.
          toggleSession(h.dataset.session);
        };
      });
      const rotateBtn = bodyEl.querySelector('.dlv-rotate');
      if (rotateBtn) rotateBtn.onclick = () => rotate(rotateBtn);
      const reverify = bodyEl.querySelector('.dlv-reverify');
      if (reverify) reverify.onclick = () => load();
    }

    function sessionRow(r) {
      const statePill = r.shredded
        ? '<span class="dlv-pill dlv-failed">shredded</span>'
        : r.sealed
          ? '<span class="dlv-pill dlv-passed">sealed</span>'
          : '<span class="dlv-pill dlv-unknown">not sealed</span>';
      const kek = r.kek ? `<span class="dlv-dim dlv-mono">${esc(r.kek)}</span>` : '';
      const action = r.sealed
        ? `<button class="dlv-btn dlv-shred" data-session="${esc(r.session_id)}">Shred key</button>`
        : '';
      const open = state.openSessions.has(r.session_id);
      const who = (r.agents && r.agents.length) ? r.agents.join(', ') : 'unattributed';
      return `<div class="dlv-row">
        <div class="dlv-row-h dlv-sess-h" data-session="${esc(r.session_id)}">
          <span class="dlv-caret">${open ? '▾' : '▸'}</span>
          <b>${esc(who)}</b>
          <span class="dlv-dim">${r.records} record${r.records === 1 ? '' : 's'}</span>
          ${r.refused ? `<span class="dlv-pill dlv-failed">${r.refused} refused</span>` : ''}
          <span class="dlv-dim dlv-mono">${esc(r.session_id)}</span>
          <div class="dlv-spacer"></div>
          <span class="dlv-dim">${esc(clockRange(r.first_at, r.last_at))}</span>
          <span class="dlv-dim">${esc(relativeTime(r.last_at))}</span>
          ${kek}${statePill}${action}
        </div>
        ${open ? `<div class="dlv-steps">${renderRecords(r.session_id)}</div>` : ''}
      </div>`;
    }

    function renderRecords(session) {
      const recs = state.sessionRecords[session];
      if (!recs) return '<div class="dlv-dim" style="padding:9px 0">Reading the chain…</div>';
      if (recs.error) return `<div class="dlv-err" style="margin:9px 0">${esc(recs.error)}</div>`;
      if (!recs.rows.length) return '<div class="dlv-dim" style="padding:9px 0">No records in this session.</div>';
      return recs.rows.map((r) => {
        const denied = r.decision === 'deny' || r.kind === 'tool_refused';
        const pill = denied ? 'dlv-failed' : r.kind === 'model_call' ? 'dlv-unknown' : 'dlv-passed';
        const what = denied ? 'refused' : (r.tool || r.kind || 'call');
        return `<div class="dlv-step">
          <div class="dlv-step-h">
            <span class="dlv-dim dlv-mono">${esc(clockOf(r.at))}</span>
            <span class="dlv-pill ${pill}">${esc(what)}</span>
            <b class="dlv-mono dlv-cmd">${esc(recordLine(r))}</b>
          </div>
          ${r.rule ? `<div class="dlv-note" style="margin-top:4px">refused because: ${esc(r.rule)}</div>` : ''}
          ${r.args && r.args !== recordLine(r) ? `<pre>${esc(r.args)}</pre>` : ''}
          <div class="dlv-note">
            #${r.seq} · ${esc(r.agent || 'unattributed')}${r.model ? ` · ${esc(r.model)}` : ''}
            ${r.cwd ? ` · <span class="dlv-mono">${esc(r.cwd)}</span>` : ''}
            ${r.args_hash ? ` · <span class="dlv-mono">${esc(r.args_hash.slice(0, 19))}…</span>` : ''}
          </div>
        </div>`;
      }).join('');
    }

    async function toggleSession(session) {
      if (state.openSessions.has(session)) {
        state.openSessions.delete(session);
        renderEvidence();
        return;
      }
      state.openSessions.add(session);
      renderEvidence();                              // Shows "Reading the chain…".
      if (!state.sessionRecords[session]) {
        try {
          state.sessionRecords[session] = { rows: await invoke('evidence_records', { session }), error: '' };
        } catch (e) {
          state.sessionRecords[session] = { rows: [], error: String(e && e.message ? e.message : e) };
        }
        if (state.tab === 'evidence') renderEvidence();
      }
    }

    async function shred(btn) {
      const session = btn.dataset.session;
      if (!confirm(`Destroy the sealing key for ${session}?\n\nThe arguments behind its records become unreadable by everyone, including us. The records stay and still verify. This cannot be undone.`)) return;
      btn.disabled = true;
      try {
        await invoke('evidence_shred', { session });
      } catch (e) {
        state.error = String(e && e.message ? e.message : e);
      }
      load();
    }

    // Rotation moves the wrapped keys, never the data keys, so no blob is
    // touched and nothing has to be re-sealed. The new KEK must already exist
    // in the HSM; creating one is not something this app can do.
    async function rotate(btn) {
      const next = prompt('Re-wrap every session key under which KEK?\n\nThe key must already exist in the HSM with encrypt and decrypt.', state.kek || '');
      if (!next || !next.trim()) return;
      btn.disabled = true;
      try {
        const moved = await invoke('evidence_rotate_kek', { newLabel: next.trim() });
        state.error = moved
          ? ''
          : `Nothing moved: every sealed session already names ${next.trim()}.`;
      } catch (e) {
        state.error = String(e && e.message ? e.message : e);
      }
      load();
    }

    function render() {
      renderProjects();
      const err = state.error ? `<div class="dlv-err">${esc(state.error)}</div>` : '';
      if (state.tab === 'tests') renderTests();
      else if (state.tab === 'releases') renderReleases();
      else if (state.tab === 'evidence') renderEvidence();
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
        } else if (state.tab === 'evidence') {
          state.sessions = await invoke('evidence_sessions');
          state.kek = await invoke('evidence_kek_label');
          // Walked on every load, not cached: a chain verified once and shown
          // as green forever is a claim about a file that has changed since.
          state.verify = await invoke('evidence_verify');
          state.sessionRecords = {};
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
  window.xnautDeliveryInternals = {
    typeOf, isTestFile, duration,
    // Evidence view model, for scripts/evidence-view-check.cjs.
    dayOf, dayLabel, groupByDay, integrityVerdict, recordLine, clockOf, clockRange,
  };

  window.xnautCreateDeliveryPanel = createDeliveryPanel;
  window.xnautDestroyDeliveryPanel = destroyDeliveryPanel;
})();
