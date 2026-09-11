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

  const stamp = (iso) => {
    const t = Date.parse(iso);
    return Number.isFinite(t) ? new Date(t).toLocaleString() : String(iso || '');
  };

  // ── Tests view model (XNAUT-329) ─────────────────────────────────────────
  //
  // The seven stages delivery.rs returns, in its order. Named here as well so
  // the panel can draw a ticket's whole life while the reader is unavailable:
  // an empty stage is information, a missing one is a lie by omission.
  const STAGES = [
    ['issue', 'Issue'],
    ['proposed', 'Proposed solution'],
    ['final', 'Final solution'],
    ['tested', 'Tested'],
    ['done', 'Done'],
    ['merged', 'Merged'],
    ['learnings', 'Learnings'],
  ];

  // passed | failed | running | unverified.
  //
  // A run that died in install, or that the app was killed under (status
  // "orphaned"), proved nothing about the code. Counting it as a failure reads
  // as "the tests found something" when nothing ran, so it is its own outcome,
  // "could not verify", and the pass rate is computed only over the runs that
  // actually reached the test step.
  function outcomeOf(r) {
    if (!r) return 'unverified';
    const reachedTest = (r.steps || []).some((s) => /test/i.test(s.name || '') && s.exit_code != null);
    if (r.status === 'running') return 'running';
    if (!reachedTest) return 'unverified';
    if (r.status === 'passed') return 'passed';
    if (r.status === 'failed') return 'failed';
    return 'unverified';
  }

  const OUTCOME_TEXT = {
    passed: 'passed', failed: 'failed', running: 'running', unverified: 'could not verify',
  };

  // One donut, inline SVG, no library. pathLength="100" turns the dash array
  // into percentages, so nothing here has to know a circumference.
  function donut(fraction, value, label, tone) {
    const f = Number.isFinite(fraction) ? Math.max(0, Math.min(1, fraction)) : 0;
    const pct = Math.round(f * 100);
    return `<div class="dlv-stat dlv-donut dlv-t-${esc(tone)}">
      <svg width="52" height="52" viewBox="0 0 36 36" aria-hidden="true">
        <circle class="dlv-ring" cx="18" cy="18" r="15.5" pathLength="100"></circle>
        <circle class="dlv-arc" cx="18" cy="18" r="15.5" pathLength="100"
          stroke-dasharray="${pct} ${Math.max(0.01, 100 - pct)}" transform="rotate(-90 18 18)"></circle>
      </svg>
      <div class="dlv-donut-t"><b>${esc(value)}</b><span>${esc(label)}</span></div>
    </div>`;
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
.dlv-pane { position:relative; }
.dlv-pane.dlv-tests-tab .dlv-side { width:272px; flex:0 0 272px; }
.dlv-side-pad { padding:0 12px 8px; }
.dlv-side-pad .dlv-select { width:100%; }
.dlv-run { border-left:2px solid transparent; cursor:pointer; }
.dlv-run:hover { background:rgba(255,255,255,.05); }
.dlv-run.active { background:rgba(79,140,255,.13); border-left-color:var(--accent,#4f8cff); }
.dlv-run-h { padding-bottom:2px; }
.dlv-run-h .dlv-pill { white-space:nowrap; }
.dlv-run-h b { font-size:12px; }
.dlv-run-sub { padding:0 11px 8px; }
.dlv-thumb { width:40px; height:27px; padding:0; border:1px solid var(--border-color,#3a3d45); border-radius:4px;
  background:rgba(0,0,0,.35); color:var(--text-secondary,#9a9faa); font:inherit; font-size:11px; cursor:pointer; }
.dlv-thumb:hover { border-color:var(--accent,#4f8cff); color:var(--text-primary,#fff); }
.dlv-thumb-lg { width:82px; height:52px; font-size:16px; }
.dlv-donut { display:flex; align-items:center; gap:10px; flex:0 1 auto; min-width:162px; }
.dlv-donut svg { flex:0 0 auto; }
.dlv-ring { fill:none; stroke:var(--border-color,#34363d); stroke-width:3.2; }
.dlv-arc { fill:none; stroke:currentColor; stroke-width:3.2; stroke-linecap:round; }
.dlv-donut-t b { display:block; font-size:19px; font-weight:650; line-height:1.2; color:var(--text-primary,#e7e9ee); }
.dlv-donut-t span { color:var(--text-secondary,#8f949e); font-size:11px; }
.dlv-t-passed { color:#4ade80; } .dlv-t-failed { color:#f87171; }
.dlv-t-unverified { color:#a78bfa; }
/* Counts, not states. The theme's --accent is #404040 (colour is reserved for
   state here), so an accent arc on a dark ground is an invisible donut. */
.dlv-t-total,.dlv-t-accent { color:var(--text-secondary,#8f949e); }
.dlv-unverified { background:rgba(167,139,250,.16); color:#a78bfa; }
.dlv-split { display:flex; gap:14px; align-items:flex-start; }
.dlv-half { flex:1 1 0; min-width:0; }
.dlv-half+.dlv-half { padding-left:14px; border-left:1px solid var(--border-color,#2a2d34); }
.dlv-h2 { font-size:14px; font-weight:650; margin-bottom:7px; }
.dlv-chips { display:flex; flex-wrap:wrap; gap:6px; }
.dlv-chip { display:inline-flex; align-items:center; gap:5px; padding:2px 8px; border-radius:999px;
  border:1px solid var(--border-color,#2f323a); color:var(--text-secondary,#9a9faa); font-size:11px; }
.dlv-chip b { color:var(--text-primary,#e7e9ee); font-weight:600; }
.dlv-text { margin-top:8px; padding:8px 10px; border-radius:6px; background:rgba(255,255,255,.025);
  white-space:pre-wrap; line-height:1.55; color:#c9ccd4; max-height:300px; overflow:auto; }
.dlv-suite { display:flex; align-items:center; gap:7px; padding:3px 0; }
.dlv-fail-list { margin:6px 0 0; padding-left:18px; color:#f8a5a5; font-size:11.5px; line-height:1.7; }
.dlv-fail-top { margin-bottom:14px; }
.dlv-side-sub { padding:9px 12px 3px; color:var(--text-secondary,#8f949e); font-size:11px;
  text-transform:uppercase; letter-spacing:.06em; }
.dlv-relgrp { display:flex; align-items:center; gap:7px; width:100%; text-align:left; padding:8px 12px;
  border:0; border-top:1px solid var(--border-color,#2a2d34); background:rgba(255,255,255,.02);
  color:var(--text-secondary,#9a9faa); font:inherit; font-size:12px; cursor:pointer; }
.dlv-relgrp:hover { background:rgba(255,255,255,.05); }
.dlv-relgrp.open { color:var(--text-primary,#e7e9ee); }
.dlv-relgrp b { color:var(--text-primary,#e7e9ee); font-weight:600; }
.dlv-relgrp .dlv-dim { margin-left:auto; font-size:11px; }
.dlv-ltabs { display:flex; gap:4px; margin-bottom:11px; }
.dlv-ltab { padding:5px 12px; border:1px solid var(--border-color,#2f323a); border-radius:7px;
  background:transparent; color:var(--text-secondary,#9a9faa); font:inherit; font-size:12px; cursor:pointer; }
.dlv-ltab:hover { color:var(--text-primary,#e7e9ee); }
.dlv-ltab.active { background:rgba(79,140,255,.14); border-color:rgba(79,140,255,.4); color:var(--text-primary,#fff); }
.dlv-card { border:1px solid var(--border-color,#2f323a); border-radius:8px; margin-bottom:8px; overflow:hidden; }
.dlv-card.empty { opacity:.55; }
.dlv-card-h { display:flex; align-items:center; gap:8px; width:100%; text-align:left; padding:9px 11px;
  border:0; background:transparent; color:var(--text-primary,#e7e9ee); font:inherit; font-size:13px; cursor:pointer; }
.dlv-card-h:hover { background:rgba(255,255,255,.03); }
.dlv-card.open .dlv-card-h { border-bottom:1px solid var(--border-color,#2a2d34); }
.dlv-card-b { padding:9px 11px 11px; }
.dlv-code { display:flex; gap:10px; align-items:flex-start; min-height:0; }
.dlv-code-files { flex:0 0 168px; max-height:520px; overflow:auto; }
.dlv-codefile { display:block; width:100%; text-align:left; padding:5px 8px; border:0; border-radius:6px;
  background:transparent; color:var(--text-secondary,#9a9faa); font:inherit; font-size:12px; cursor:pointer; }
.dlv-codefile:hover { background:rgba(255,255,255,.04); color:var(--text-primary,#e7e9ee); }
.dlv-codefile.active { background:rgba(79,140,255,.14); color:var(--text-primary,#fff); }
.dlv-codefile small { display:block; color:var(--text-secondary,#7e838d); font-size:10.5px;
  overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.dlv-code-diff { flex:1 1 0; min-width:0; max-height:520px; overflow:auto;
  border:1px solid var(--border-color,#2a2d34); border-radius:7px; }
.dlv-diff-h { padding:5px 9px; background:rgba(255,255,255,.03); color:var(--text-secondary,#8f949e); font-size:11px; }
.dlv-diff { margin:0; padding:0; font-size:11.5px; line-height:1.55; overflow-x:auto; }
.dlv-dl { display:block; padding:0 9px; white-space:pre; }
.dlv-dl-add { background:rgba(74,222,128,.10); color:#8ff0b0; }
.dlv-dl-del { background:rgba(248,113,113,.10); color:#f8a5a5; }
.dlv-dl-hunk { color:#8ab4ff; background:rgba(138,180,255,.07); }
.dlv-dl-head,.dlv-dl-meta { color:var(--text-secondary,#8f949e); }
.dlv-filelist { margin:8px 0 0; padding-left:18px; line-height:1.8; font-size:12px; }
.dlv-runstats { margin-bottom:10px; }
.dlv-runcard { flex:0 1 auto; min-width:118px; }
.dlv-runcard b { color:currentColor; }
.dlv-runcard span { display:block; }
.dlv-mini { color:var(--text-secondary,#8f949e); font-size:11px; margin-top:3px; }
.dlv-foot { margin-top:16px; padding-top:11px; border-top:1px solid var(--border-color,#2a2d34); }
.dlv-raw { margin-top:9px; padding-top:4px; border-top:1px solid var(--border-color,#2a2d34); }
.dlv-life-err { color:#f8a5a5; }
.dlv-overlay { position:absolute; inset:0; z-index:40; display:flex; align-items:center; justify-content:center;
  background:rgba(0,0,0,.62); }
.dlv-overlay-box { display:flex; flex-direction:column; width:min(880px,92%); max-height:86%; overflow:hidden;
  border:1px solid var(--border-color,#3a3d45); border-radius:10px; background:var(--bg-primary,#14161b); }
.dlv-overlay-h { display:flex; align-items:center; gap:9px; padding:9px 12px;
  border-bottom:1px solid var(--border-color,#2a2d34); }
.dlv-overlay-b { padding:12px; overflow:auto; }
.dlv-evidence { display:block; width:100%; border-radius:6px; background:#000; }
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
          <button class="dlv-memory">Memory</button>
        </div>
        <div class="dlv-spacer"></div>
        <select class="dlv-select dlv-since" title="Report window">
          ${SINCE.map(([v, l]) => `<option value="${esc(v)}">${esc(l)}</option>`).join('')}
        </select>
        <button class="dlv-btn dlv-download">Download report</button>
        <button class="dlv-btn dlv-refresh">Refresh</button>
      </div>
      <div class="dlv-main">
        <div class="dlv-side"></div>
        <div class="dlv-body"><div class="dlv-empty">Loading…</div></div>
      </div>`;
    parentContainer.appendChild(pane);

    const bodyEl = pane.querySelector('.dlv-body');
    const sideEl = pane.querySelector('.dlv-side');
    const sinceEl = pane.querySelector('.dlv-since');
    const state = {
      projects: [], keys: [], project: null, tab: opts.tab || 'tests',
      focusTicket: opts.ticket || '', since: '7 days ago',
      records: [], releases: [], commits: [], tickets: [], error: '',
      // Tests: the selected run, the lifecycle read for its ticket, and
      // whether the raw logs are open. `life` is keyed by ticket so a stale
      // read can never be painted under a different run.
      run: '', life: null, raw: false,
      sessions: [], kek: '',
      // Evidence: the verify report as evidence.rs returned it, the records
      // read per expanded session, and which sessions are expanded.
      // NOT `records`: that key is the Tests tab's run list, and a second one
      // here would silently shadow it and empty the Tests tab.
      verify: null, sessionRecords: {},
      releaseTag: '', session: '', reportSection: '', openReleases: null,
      leftTab: 'issue', codeFile: '', diffs: {}, openStages: null,
    };
    sinceEl.value = state.since;

    const repoOf = (p) => (p && p.source_path) || '';
    const setTab = (tab) => {
      state.tab = tab;
      pane.classList.toggle('dlv-tests-tab', tab === 'tests');
      pane.querySelectorAll('.dlv-tabs button').forEach((b) => b.classList.toggle('active', b.dataset.tab === tab));
      pane.querySelector('.dlv-download').hidden = tab !== 'report';
      load();
    };

    // Every tab has the same left column: the project is chosen from one
    // dropdown at the top, and under it is that tab's list of things to pick.
    // A tab that showed a different project selector than its neighbour made
    // the reader check which one they were looking at every time they moved.
    function projectSelect() {
      const options = state.projects.map((p) => `<option value="${esc(p.key)}"${
        state.project && p.key === state.project.key ? ' selected' : ''}>${esc(p.name || p.key)}</option>`).join('');
      return `<div class="dlv-side-h">Project</div>
        <div class="dlv-side-pad">
          <select class="dlv-select dlv-proj-select" aria-label="Project">${
            options || '<option value="">No projects</option>'}</select>
        </div>`;
    }

    function bindProjectSelect() {
      const select = sideEl.querySelector('.dlv-proj-select');
      if (!select) return;
      select.onchange = () => {
        state.project = state.projects.find((p) => p.key === select.value) || null;
        state.run = ''; state.life = null; state.raw = false; state.focusTicket = '';
        state.releaseTag = ''; state.session = ''; state.openReleases = null;
        load();
      };
    }

    // The side list for whichever tab is showing. One selection each: a run,
    // a release, a session, a section of the report.
    function renderSide(items, heading, cls, activeKey) {
      sideEl.innerHTML = projectSelect()
        + `<div class="dlv-side-h">${esc(heading)}</div>`
        + (items.length ? items.join('') : `<div class="dlv-empty" style="padding:12px">Nothing here</div>`);
      bindProjectSelect();
      sideEl.querySelectorAll(`.${cls}`).forEach((el) => {
        el.onclick = (e) => {
          // A control inside a row is not the row: shred, and the evidence
          // thumbnail, both live in one and neither selects it.
          if (e.target.closest('button') || e.target.closest('.dlv-thumb')) return;
          activeKey(el.dataset.key);
        };
      });
    }

    // ── Tests ────────────────────────────────────────────────────────────────
    //
    // The tab was a list of runs that expanded into the raw log. A log is what
    // the machine wrote, not what the run settled, so the log now sits one
    // control away and the page answers three questions instead: what was
    // asked (the ticket), what the verify proved (totals, failing names, the
    // steps), and where the ticket stands in its life.

    const SINCE_DAYS = { '2 days ago': 2, '7 days ago': 7, '30 days ago': 30, '90 days ago': 90 };
    const windowStart = () => new Date(Date.now() - (SINCE_DAYS[state.since] || 7) * 86400000).toISOString();

    const runsFor = () => (state.records || [])
      .filter((r) => !state.project || r.project === state.project.key)
      .sort((a, b) => String(b.created_at).localeCompare(String(a.created_at)));

    // One read per ticket, keyed by ticket so a render loop cannot re-ask and
    // a resolved read cannot be painted under a different run. The command is
    // allowed to fail: the page then shows the run's own record and says so.
    async function loadLifecycle(ticket) {
      if (!ticket) { state.life = null; return; }
      if (state.life && state.life.ticket === ticket) return;
      state.life = { ticket, data: null, error: '' };
      try {
        const data = await invoke('delivery_lifecycle', { ticket });
        if (!data) throw new Error('delivery_lifecycle returned nothing');
        state.life = { ticket, data, error: '' };
      } catch (e) {
        state.life = { ticket, data: null, error: String(e && e.message ? e.message : e) };
      }
      if (state.tab === 'tests') renderTests();
    }

    function renderTests() {
      const runs = runsFor();
      if (!runs.some((r) => r.id === state.run)) {
        // The selection was made for the reader, not by them: from another
        // panel focusing a ticket, or by falling back to the newest run. Open
        // the group it landed in, or the row they are being shown is behind a
        // fold. A group the reader closed themselves stays closed.
        const focus = state.focusTicket && runs.find((r) => r.ticket_id === state.focusTicket);
        const picked = focus || runs[0] || null;
        state.run = (picked || {}).id || '';
        if (picked && state.openReleases) state.openReleases.add(releaseOf(picked));
      }
      const run = runs.find((r) => r.id === state.run) || null;
      if (run) loadLifecycle(run.ticket_id);
      const life = run && state.life && state.life.ticket === run.ticket_id ? state.life : null;
      const lc = life && life.data ? life.data : null;

      renderRunsSide(runs);

      const counts = { passed: 0, failed: 0, running: 0, unverified: 0 };
      runs.forEach((r) => { counts[outcomeOf(r)] += 1; });
      // Only the runs that reached the test step can pass or fail, so only
      // they are the rate's denominator. An install that never compiled says
      // nothing about the tests, in either direction.
      const settled = counts.passed + counts.failed;
      const rate = settled ? Math.round((counts.passed / settled) * 100) : 0;
      const sinceIso = windowStart();
      const inWindow = (state.tickets || []).filter((t) => String(t.updated_at || '') >= sinceIso);
      const green = new Set(runs.filter((r) => outcomeOf(r) === 'passed').map((r) => r.ticket_id).filter(Boolean));
      const verified = inWindow.filter((t) => green.has(t.id)).length;

      const winLabel = (SINCE.find(([v]) => v === state.since) || [])[1] || state.since;
      const donuts = `<div class="dlv-sec">Every run in this project, last ${esc(winLabel)}</div>
      <div class="dlv-stats dlv-donuts">
        ${donut(1, runs.length, 'runs', 'total')}
        ${donut(settled ? counts.passed / settled : 0, `${rate}%`, 'pass rate', 'passed')}
        ${donut(runs.length ? counts.passed / runs.length : 0, counts.passed, 'passed', 'passed')}
        ${donut(runs.length ? counts.failed / runs.length : 0, counts.failed, 'failed', 'failed')}
        ${donut(runs.length ? counts.unverified / runs.length : 0, counts.unverified, 'could not verify', 'unverified')}
        ${donut(inWindow.length ? verified / inWindow.length : 0,
          `${verified}/${inWindow.length}`, 'tickets verified', 'accent')}
      </div>`;

      if (!run) {
        bodyEl.innerHTML = `${donuts}<div class="dlv-empty">No verification runs for this project yet. A run is started
          from a ticket, or with <span class="dlv-mono">sandbox_verify_start</span>.</div>`;
        return;
      }

      // Honest degradation. delivery_lifecycle is one reader over five files;
      // when it cannot answer, the run's own record is still here and the page
      // says which half is missing rather than inventing a stage.
      const note = !run.ticket_id
        ? '<div class="dlv-note">This run is not linked to a ticket, so there is no lifecycle to read.</div>'
        : life && life.error
          ? `<div class="dlv-note dlv-life-err">The lifecycle could not be read: ${esc(life.error)}.
             Showing this run's own record.</div>`
          : lc ? '' : '<div class="dlv-note">Reading the lifecycle…</div>';

      bodyEl.innerHTML = donuts + runCards(run, lc) + note
        + `<div class="dlv-split">${leftPane(run, lc)}
             <div class="dlv-half dlv-life">${resolutionSection(lc)}</div></div>`
        + runFoot(run, lc);

      const raw = bodyEl.querySelector('.dlv-raw-toggle');
      if (raw) raw.onclick = () => { state.raw = !state.raw; renderTests(); };
      bodyEl.querySelectorAll('.dlv-ltab').forEach((b) => {
        b.onclick = () => { state.leftTab = b.dataset.ltab; renderTests(); };
      });
      bodyEl.querySelectorAll('.dlv-codefile').forEach((b) => {
        b.onclick = () => { state.codeFile = b.dataset.file; renderTests(); };
      });
      bodyEl.querySelectorAll('.dlv-stage-h').forEach((b) => {
        b.onclick = () => {
          const key = b.dataset.toggle;
          if (state.openStages.has(key)) state.openStages.delete(key);
          else state.openStages.add(key);
          renderTests();
        };
      });
      if (state.leftTab === 'code' && lc) loadDiffs(lc);
      bindEvidence(bodyEl);
    }

    // Runs grouped by the release their ticket is tagged for, newest release
    // first, each group collapsible. A flat list of two hundred runs is a
    // scroll, not a list; grouped, the question "what went into 1.27.0" is
    // one glance. A ticket with no release is General, which is where most of
    // them live and is not an error.
    const GENERAL = 'General';

    function releaseOf(run) {
      const t = (state.tickets || []).find((x) => x.id === run.ticket_id);
      const rel = t && String(t.release || '').trim();
      return rel || GENERAL;
    }

    function groupByRelease(runs) {
      const groups = new Map();
      runs.forEach((r) => {
        const key = releaseOf(r);
        if (!groups.has(key)) groups.set(key, []);
        groups.get(key).push(r);
      });
      // Releases newest first by version order, General last: it is the
      // holding pen, not the newest release.
      const cmp = (a, b) => {
        if (a === GENERAL) return 1;
        if (b === GENERAL) return -1;
        const pa = String(a).split('.').map(Number);
        const pb = String(b).split('.').map(Number);
        for (let i = 0; i < Math.max(pa.length, pb.length); i += 1) {
          const d = (pb[i] || 0) - (pa[i] || 0);
          if (d) return d;
        }
        return String(a).localeCompare(String(b));
      };
      return [...groups.keys()].sort(cmp).map((key) => [key, groups.get(key)]);
    }

    function renderRunsSide(runs) {
      const groups = groupByRelease(runs);
      // Open to begin with: the grouping is there to let the reader fold away
      // what they are not looking at, not to hide the list until they ask for
      // it. A closed group stays closed while the tab is open.
      if (!state.openReleases) state.openReleases = new Set(groups.map(([key]) => key));
      const items = groups.flatMap(([key, rows]) => {
        const open = state.openReleases.has(key);
        const tickets = new Set(rows.map((r) => r.ticket_id).filter(Boolean));
        const head = `
          <button class="dlv-relgrp${open ? ' open' : ''}" data-rel="${esc(key)}"
            aria-expanded="${open ? 'true' : 'false'}">
            <span class="dlv-caret">${open ? '▾' : '▸'}</span>
            <b>${esc(key)}</b>
            <span class="dlv-dim">${rows.length} run${rows.length === 1 ? '' : 's'} ·
              ${tickets.size} ticket${tickets.size === 1 ? '' : 's'}</span>
          </button>`;
        return open ? [head, ...rows.map(runRow)] : [head];
      });

      renderSide(items, `Runs (${runs.length})`, 'dlv-run', (id) => {
        state.run = id;
        state.raw = false;
        state.focusTicket = '';
        renderTests();
      });
      sideEl.querySelectorAll('.dlv-relgrp').forEach((b) => {
        b.onclick = () => {
          const key = b.dataset.rel;
          if (state.openReleases.has(key)) state.openReleases.delete(key);
          else state.openReleases.add(key);
          renderTests();
        };
      });
      bindEvidence(sideEl);
    }

    function runRow(r) {
      const out = outcomeOf(r);
      return `<div class="dlv-run${r.id === state.run ? ' active' : ''}" data-run="${esc(r.id)}" data-key="${esc(r.id)}">
        <div class="dlv-row-h dlv-run-h">
          <span class="dlv-pill dlv-${esc(out)}">${esc(OUTCOME_TEXT[out])}</span>
          <b>${esc(r.ticket_id || 'unlinked')}</b>
          <div class="dlv-spacer"></div>
          ${thumbButton(r, false)}
        </div>
        <div class="dlv-run-sub dlv-dim">${esc(duration(r.created_at, r.updated_at) || 'no duration')}
          · ${esc(relativeTime(r.created_at))}</div>
      </div>`;
    }

    // The left half is two readings of the same work: what was asked, and
    // what changed on disk because of it. Tabs rather than more columns; the
    // pane is already half a window wide.
    function leftPane(run, lc) {
      const tabs = [['issue', 'Issue'], ['code', 'Code']].map(([key, label]) =>
        `<button class="dlv-ltab${state.leftTab === key ? ' active' : ''}" data-ltab="${key}">${label}</button>`).join('');
      return `<div class="dlv-half dlv-left">
        <div class="dlv-ltabs">${tabs}</div>
        ${state.leftTab === 'code' ? codePane(run, lc) : issueBody(run, lc)}
      </div>`;
    }

    // The files the work touched on the left, the change itself on the right.
    // The diff is read per commit with git_commit_diff and split by file, the
    // same raw unified text the vault's Changes pane paints; one renderer for
    // a diff in this app is enough.
    function codePane(run, lc) {
      const commits = (lc && lc.commits) || [];
      const repo = repoOf(state.project);
      if (!commits.length) {
        const files = (lc && lc.files) || [];
        return files.length
          ? `<div class="dlv-note">No commit is recorded on this ticket, so there is no diff to read.
               The handback named these files:</div>
             <ul class="dlv-filelist">${files.map((f) => `<li class="dlv-mono">${esc(f)}</li>`).join('')}</ul>`
          : '<div class="dlv-note">No commit and no file is recorded on this ticket.</div>';
      }
      if (!repo) return '<div class="dlv-note">This project has no local repo path set, so the commits cannot be read.</div>';

      const loaded = commits.map((sha) => state.diffs[sha]).filter(Boolean);
      if (loaded.length < commits.length) return '<div class="dlv-note">Reading the commits…</div>';
      const failed = loaded.filter((d) => d.error);
      const byFile = new Map();
      loaded.forEach((d) => (d.files || []).forEach((f) => {
        if (!byFile.has(f.path)) byFile.set(f.path, []);
        byFile.get(f.path).push({ sha: d.sha, text: f.text });
      }));
      const paths = [...byFile.keys()].sort();
      if (!paths.length) {
        return `<div class="dlv-note">The ${commits.length} commit${commits.length === 1 ? '' : 's'} on this ticket
          changed no file this checkout can read.${failed.length ? ` ${esc(failed[0].error)}` : ''}</div>`;
      }
      if (!byFile.has(state.codeFile)) state.codeFile = paths[0];
      const sections = byFile.get(state.codeFile) || [];
      return `<div class="dlv-code">
        <div class="dlv-code-files">
          ${paths.map((p) => `<button class="dlv-codefile${p === state.codeFile ? ' active' : ''}" data-file="${esc(p)}"
            title="${esc(p)}"><span class="dlv-mono">${esc(p.split('/').pop())}</span>
            <small>${esc(p.split('/').slice(0, -1).join('/') || '.')}</small></button>`).join('')}
        </div>
        <div class="dlv-code-diff">
          ${sections.map((sec) => `<div class="dlv-diff-h"><span class="dlv-mono">${esc(sec.sha.slice(0, 8))}</span></div>
            <pre class="dlv-diff">${diffHtml(sec.text)}</pre>`).join('')}
        </div>
      </div>`;
    }

    // Painted line by line, the same rule the vault pane uses. No library.
    function diffHtml(raw) {
      return String(raw || '').split('\n').map((line) => {
        const t = esc(line) || ' ';
        if (/^\+\+\+|^---/.test(line)) return `<span class="dlv-dl dlv-dl-head">${t}</span>`;
        if (line.startsWith('@@')) return `<span class="dlv-dl dlv-dl-hunk">${t}</span>`;
        if (line.startsWith('+')) return `<span class="dlv-dl dlv-dl-add">${t}</span>`;
        if (line.startsWith('-')) return `<span class="dlv-dl dlv-dl-del">${t}</span>`;
        if (/^diff |^index |^new file|^deleted file|^rename /.test(line)) return `<span class="dlv-dl dlv-dl-meta">${t}</span>`;
        return `<span class="dlv-dl">${t}</span>`;
      }).join('\n');
    }

    // One read per commit, cached by sha. A commit this checkout does not have
    // is recorded as an error against that sha rather than retried forever.
    async function loadDiffs(lc) {
      const repo = repoOf(state.project);
      const missing = ((lc && lc.commits) || []).filter((sha) => !state.diffs[sha]);
      if (!repo || !missing.length) return;
      for (const sha of missing) {
        try {
          const raw = await invoke('git_commit_diff', { repo, sha });
          state.diffs[sha] = { sha, files: splitDiffByFile(raw), error: '' };
        } catch (e) {
          state.diffs[sha] = { sha, files: [], error: String(e && e.message ? e.message : e) };
        }
      }
      if (state.tab === 'tests' && state.leftTab === 'code') renderTests();
    }

    // `git show` writes one `diff --git a/x b/x` header per file; that header
    // is the only reliable boundary, and the b-side path is the one that
    // exists after the change.
    function splitDiffByFile(raw) {
      const out = [];
      String(raw || '').split(/^diff --git /m).forEach((chunk) => {
        if (!chunk.trim()) return;
        const m = /^a\/(\S+)\s+b\/(\S+)/.exec(chunk);
        out.push({ path: (m && (m[2] || m[1])) || 'unknown', text: `diff --git ${chunk}`.replace(/\s+$/, '') });
      });
      return out;
    }

    function issueBody(run, lc) {
      const ticket = (state.tickets || []).find((t) => t.id === run.ticket_id) || null;
      const pick = (a, b) => a || b || '';
      const meta = [
        ['type', pick(lc && lc.kind, ticket && ticket.type)],
        ['priority', pick(lc && lc.priority, ticket && ticket.priority)],
        ['owner', pick(lc && lc.owner, ticket && ticket.owner)],
        ['status', pick(lc && lc.status, ticket && ticket.status)],
        ['branch', (lc && lc.branch) || ''],
      ];
      const body = pick(lc && lc.body, ticket && ticket.body);
      const title = pick(lc && lc.title, ticket && ticket.title);
      return `<div class="dlv-issue">
        <div class="dlv-h2">${run.ticket_id ? `<span class="dlv-mono">${esc(run.ticket_id)}</span> ` : ''}${
          esc(title || 'no title recorded')}</div>
        <div class="dlv-chips">${meta.map(([k, v]) =>
          `<span class="dlv-chip">${esc(k)} <b>${esc(v || 'unknown')}</b></span>`).join('')}</div>
        ${body
          ? `<div class="dlv-text">${esc(body)}</div>`
          : '<div class="dlv-note">The ticket text is not on this record.</div>'}
      </div>`;
    }

    // The numbers this run produced, as cards beside the window's donuts.
    // They are the ones that change when the reader picks another run, which
    // is what makes the two rows legible as two different questions.
    function runCards(run, lc) {
      const v = (lc && lc.verify) || null;
      const suites = (v && v.suites) || [];
      const steps = (v && v.steps && v.steps.length ? v.steps : run.steps) || [];
      const cards = suites.map((x) => {
        const bad = Number(x.failed) > 0;
        return `<div class="dlv-stat dlv-runcard dlv-t-${bad ? 'failed' : 'passed'}" data-suite="${esc(x.name)}">
          <b>${esc(x.passed)}</b><span>${esc(x.name)} passed</span>
          <div class="dlv-mini">${esc(x.failed)} failed · ${esc(x.skipped)} skipped</div>
        </div>`;
      }).concat(steps.map((x) => {
        const code = x.exit_code;
        const ms = Number(x.duration_ms || 0);
        const tone = code == null ? 'total' : code === 0 ? 'passed' : 'failed';
        return `<div class="dlv-stat dlv-runcard dlv-t-${tone}" data-step="${esc(x.name)}">
          <b>${code == null ? 'running' : `exit ${esc(code)}`}</b><span>${esc(x.name)}</span>
          <div class="dlv-mini">${ms > 0 ? esc(stepDuration(ms)) : 'not timed'}</div>
        </div>`;
      }));
      const failing = (v && v.failing) || [];
      if (!cards.length && !failing.length) return '';
      return `<div class="dlv-sec">${esc(run.ticket_id || 'This run')} · what the verify proved</div>
        <div class="dlv-stats dlv-runstats">${cards.join('')}</div>
        ${failing.length
          ? `<ul class="dlv-fail-list dlv-fail-top">${failing.map((f) => `<li>${esc(f)}</li>`).join('')}</ul>`
          : ''}`;
    }

    function stepDuration(ms) {
      if (ms < 1000) return `${ms}ms`;
      if (ms < 60000) return `${Math.round(ms / 100) / 10}s`;
      return `${Math.floor(ms / 60000)}m ${Math.round((ms % 60000) / 1000)}s`;
    }

    // Where the run happened and how to look at it for yourself. Below the
    // reading, because it is the apparatus, not the finding.
    function runFoot(run, lc) {
      const v = (lc && lc.verify) || null;
      const sandbox = (v && v.sandbox) || run.public_url || run.sandbox_id || '';
      const commit = (v && v.commit) || run.commit_sha || '';
      return `<div class="dlv-foot">
        <div class="dlv-row-h" style="padding:0; cursor:default">
          ${thumbButton(run, true)}
          <div class="dlv-note" style="margin:0">
            sandbox <span class="dlv-mono">${esc(sandbox || 'none recorded')}</span>
            · commit <span class="dlv-mono">${esc(commit ? String(commit).slice(0, 12) : 'unknown')}</span>
            ${run.error ? `<br><span class="dlv-life-err">${esc(run.error)}</span>` : ''}
          </div>
          <div class="dlv-spacer"></div>
          <button class="dlv-btn dlv-raw-toggle">${state.raw ? 'Hide raw output' : 'Raw output'}</button>
        </div>
        ${state.raw ? `<div class="dlv-raw">${rawBlock(run)}</div>` : ''}
      </div>`;
    }

    // The old Tests tab, unchanged, behind one control. Nothing about it was
    // wrong; it answered a different question than the page asks.
    function rawBlock(r) {
      return `${(r.steps || []).map((s) => `
        <div class="dlv-step">
          <div class="dlv-step-h">
            <span class="dlv-pill ${s.exit_code == null ? 'dlv-running' : (s.exit_code === 0 ? 'dlv-passed' : 'dlv-failed')}">${
              s.exit_code == null ? 'running' : `exit ${esc(s.exit_code)}`}</span>
            <b>${esc(s.name)}</b>
            <span class="dlv-dim dlv-mono">${esc(s.command)}</span>
          </div>
          ${s.log_tail ? `<pre>${esc(s.log_tail)}</pre>` : ''}
        </div>`).join('')}
        ${r.error ? `<div class="dlv-err" style="margin-top:8px">${esc(r.error)}</div>` : ''}
        <div class="dlv-note">
          ${r.public_url ? `sandbox <span class="dlv-mono">${esc(r.public_url)}</span> · ` : ''}
          logs <span class="dlv-mono">${esc(r.log_dir || 'none recorded')}</span>
          ${r.video_path ? ` · video <span class="dlv-mono">${esc(r.video_path)}</span>` : ''}
        </div>`;
    }

    // Every stage, always, in delivery.rs's order. A stage the ticket has not
    // reached renders empty: "not reached yet" is what the reader needs to
    // know, and omitting it would read as a life that skipped a step.
    // The resolution: what happened to the ticket after it was written, as
    // cards. The Issue stage is not among them; it is the left pane, and
    // printing the same paragraph twice on one screen taught the reader
    // nothing the first copy had not.
    //
    // No connecting line. A line down the side implies these are moments on
    // one thread, and they are not: they are six different records, some of
    // which never happen.
    const RESOLUTION = STAGES.filter(([key]) => key !== 'issue');

    function resolutionSection(lc) {
      const byKey = new Map(((lc && lc.stages) || []).map((x) => [x.key, x]));
      // Open what has something to say, shut what has not. A reader opening
      // this pane wants the story, not six headers to click. Recomputed when
      // the ticket changes: which cards are worth opening is a fact about the
      // ticket, so carrying one ticket's answer to the next is just wrong.
      const key0 = (lc && lc.ticket) || '';
      if (!state.openStages || state.openStages.ticket !== key0) {
        state.openStages = new Set(RESOLUTION
          .filter(([key]) => { const x = byKey.get(key); return x && (x.text || (x.items || []).length); })
          .map(([key]) => key));
        state.openStages.ticket = key0;
      }
      return '<div class="dlv-sec">Resolution</div>' + RESOLUTION.map(([key, title]) => {
        const stage = byKey.get(key) || null;
        const at = (stage && stage.at) || '';
        const text = (stage && stage.text) || '';
        const items = (stage && stage.items) || [];
        const empty = !at && !text && !items.length;
        const open = !empty && state.openStages.has(key);
        if (empty) {
          return `<div class="dlv-card empty" data-stage="${esc(key)}">
            <div class="dlv-card-h" style="cursor:default">
              <b>${esc((stage && stage.title) || title)}</b>
              <span class="dlv-dim">${key === 'learnings' ? 'nothing recorded yet' : 'not reached yet'}</span>
            </div></div>`;
        }
        return `<div class="dlv-card${open ? ' open' : ''}" data-stage="${esc(key)}">
          <button class="dlv-card-h dlv-stage-h" data-toggle="${esc(key)}" aria-expanded="${open ? 'true' : 'false'}">
            <span class="dlv-caret">${open ? '▾' : '▸'}</span>
            <b>${esc((stage && stage.title) || title)}</b>
            ${at ? `<span class="dlv-dim">${esc(stamp(at))}</span>` : ''}
            <span class="dlv-spacer"></span>
            ${items.length ? `<span class="dlv-dim">${items.length} item${items.length === 1 ? '' : 's'}</span>` : ''}
          </button>
          ${open ? `<div class="dlv-card-b">
            ${text ? `<div class="dlv-text" style="margin-top:0">${esc(text)}</div>` : ''}
            ${items.length
              ? `<div class="dlv-tickets">${items.map((i) => `<span class="dlv-tag">${esc(i)}</span>`).join('')}</div>`
              : ''}
          </div>` : ''}
        </div>`;
      }).join('');
    }

    // ── Evidence of the run itself ───────────────────────────────────────────
    const thumbButton = (r, big) => `<button class="dlv-thumb${big ? ' dlv-thumb-lg' : ''}"
      data-evidence="${esc(r.id)}" title="Open the evidence for this run"
      aria-label="Evidence for ${esc(r.ticket_id || r.id)}">${r.video_path ? '▶' : '⛶'}</button>`;

    const bindEvidence = (root) => root.querySelectorAll('[data-evidence]')
      .forEach((b) => { b.onclick = () => openEvidence(b.dataset.evidence); });

    function openEvidence(id) {
      const run = (state.records || []).find((r) => r.id === id);
      if (!run) return;
      const life = state.life && state.life.ticket === run.ticket_id ? state.life : null;
      const ev = (life && life.data && life.data.evidence) || null;
      const video = run.video_path || (ev && ev.video_path) || '';
      const shot = (ev && ev.screenshot_path) || '';
      // Tauri will not load a file:// path from the webview; the asset URL is
      // how right-pane-workspace.js:1210 does it. Outside Tauri the path is
      // used as-is rather than throwing.
      const assetUrl = (p) => {
        try { return window.__TAURI__.core.convertFileSrc(p); } catch (_) { return p; }
      };
      const media = video
        ? `<video class="dlv-evidence" src="${esc(assetUrl(video))}" controls autoplay muted></video>`
        : shot
          ? `<img class="dlv-evidence" src="${esc(assetUrl(shot))}" alt="Screenshot of this run">`
          : `<div class="dlv-empty dlv-no-recording">No recording for this run.${
            ev && ev.note ? ` ${esc(ev.note)}` : ''}</div>`;
      const el = document.createElement('div');
      el.className = 'dlv-overlay';
      el.innerHTML = `<div class="dlv-overlay-box">
        <div class="dlv-overlay-h">
          <b>Evidence · ${esc(run.ticket_id || run.id)}</b>
          <span class="dlv-dim">${esc(relativeTime(run.created_at))}</span>
          <div class="dlv-spacer"></div>
          <button class="dlv-btn dlv-overlay-x" aria-label="Close evidence">Close</button>
        </div>
        <div class="dlv-overlay-b">
          ${media}
          <div class="dlv-note">logs <span class="dlv-mono">${esc(run.log_dir || 'none recorded')}</span></div>
        </div>
      </div>`;
      pane.appendChild(el);
      el.querySelector('.dlv-overlay-x').onclick = () => el.remove();
      el.onclick = (e) => { if (e.target === el) el.remove(); };
    }

    // ── Releases ─────────────────────────────────────────────────────────────
    function renderReleases() {
      const rel = state.releases;
      if (!rel.some((r) => r.tag === state.releaseTag)) state.releaseTag = (rel[0] || {}).tag || '';
      renderSide(rel.map((r) => `
        <div class="dlv-run dlv-rel${r.tag === state.releaseTag ? ' active' : ''}" data-key="${esc(r.tag)}">
          <div class="dlv-row-h dlv-run-h" style="cursor:pointer">
            <b class="dlv-mono">${esc(r.tag)}</b>
            <div class="dlv-spacer"></div>
            <span class="dlv-dim">${r.tickets.length} ticket${r.tickets.length === 1 ? '' : 's'}</span>
          </div>
          <div class="dlv-run-m"><span class="dlv-dim">${esc(r.date)}</span>
            <span class="dlv-dim">${r.commits} commit${r.commits === 1 ? '' : 's'}</span></div>
        </div>`), `Releases (${rel.length})`, 'dlv-rel', (tag) => { state.releaseTag = tag; renderReleases(); });

      const shipped = rel.reduce((n, r) => n + r.tickets.length, 0);
      const stats = `
        <div class="dlv-stats">
          <div class="dlv-stat"><b>${rel.length}</b><span>releases</span></div>
          <div class="dlv-stat"><b>${rel[0] ? esc(rel[0].tag) : '—'}</b><span>latest</span></div>
          <div class="dlv-stat"><b>${rel[0] ? esc(rel[0].date) : '—'}</b><span>shipped</span></div>
          <div class="dlv-stat"><b>${shipped}</b><span>tickets shipped</span></div>
        </div>`;
      const one = rel.find((r) => r.tag === state.releaseTag);
      if (!one) {
        bodyEl.innerHTML = `${stats}<div class="dlv-empty">No <span class="dlv-mono">v*</span> tags in this repository.</div>`;
        return;
      }
      bodyEl.innerHTML = stats
        + `<div class="dlv-sec">${esc(one.tag)} · ${esc(one.date)}</div>
        <div class="dlv-split">
          <div class="dlv-half">
            <div class="dlv-h2">What shipped</div>
            ${one.subject ? `<div class="dlv-text">${esc(one.subject)}</div>` : '<div class="dlv-note">This tag carries no subject.</div>'}
            <div class="dlv-note">${one.commits} commit${one.commits === 1 ? '' : 's'} since the tag before it.</div>
          </div>
          <div class="dlv-half">
            <div class="dlv-h2">Tickets in this release (${one.tickets.length})</div>
            ${one.tickets.length
              ? `<div class="dlv-tickets">${one.tickets.map((t) => `<span class="dlv-tag">${esc(t)}</span>`).join('')}</div>`
              : '<div class="dlv-note">No commit in this range named a ticket.</div>'}
          </div>
        </div>`;
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
      const sinceIso = windowStart();
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
      const sections = [
        ['unproven', `Claimed finished, no commit (${m.unproven.length})`],
        ['tickets', `Tickets moved (${m.moved.length})`],
        ['commits', `Commits by type (${m.commits.length})`],
      ];
      renderSide(sections.map(([key, label]) => `
        <div class="dlv-run dlv-rep${key === state.reportSection ? ' active' : ''}" data-key="${esc(key)}">
          <div class="dlv-row-h dlv-run-h" style="cursor:pointer"><b>${esc(label)}</b></div>
        </div>`), 'Report', 'dlv-rep', (key) => {
        state.reportSection = key;
        const el = bodyEl.querySelector(`#dlv-rep-${key}`);
        if (el) el.scrollIntoView({ block: 'start', behavior: 'smooth' });
        renderReport();
      });
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

      const ticketSection = '<div id="dlv-rep-tickets"></div>' + STATUSES.map((status) => {
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

      const commitSection = '<div id="dlv-rep-commits"></div>' + [...TYPES, 'other'].map((type) => {
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

      const unprovenSection = m.unproven.length ? `<div id="dlv-rep-unproven" class="dlv-sec">Claimed finished, no commit found (${m.unproven.length})</div>`
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
      if (!rows.some((r) => r.session_id === state.session)) state.session = (rows[0] || {}).session_id || '';
      renderSide(
        groupByDay(rows).flatMap(([day, group]) => [
          `<div class="dlv-side-sub">${esc(dayLabel(day))} · ${group.length}</div>`,
          ...group.map(sessionRow),
        ]),
        `Sessions (${rows.length})`, 'dlv-sess',
        (id) => { state.session = id; openSession(id); });

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
      const one = rows.find((r) => r.session_id === state.session);
      const body = one
        ? `<div class="dlv-sec">${esc((one.agents || []).join(', ') || 'unattributed')} ·
             ${one.records} record${one.records === 1 ? '' : 's'} · <span class="dlv-mono">${esc(one.session_id)}</span></div>
           ${renderRecords(one.session_id)}`
        : `<div class="dlv-empty">
             ${esc(verdict.headline)}.
             <div style="margin-top:8px">Looked in <span class="dlv-mono">${esc(verdict.path || 'no path reported')}</span></div>
             <div style="margin-top:8px">Records are written by the agent hook with no action from the agent;
               once one runs a tool, its session appears here.</div>
           </div>`;

      bodyEl.innerHTML = banner + stats + body;
      pane.querySelectorAll('.dlv-shred').forEach((b) => { b.onclick = () => shred(b); });
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
      const who = (r.agents && r.agents.length) ? r.agents.join(', ') : 'unattributed';
      return `<div class="dlv-run dlv-sess${r.session_id === state.session ? ' active' : ''}" data-key="${esc(r.session_id)}">
        <div class="dlv-row-h dlv-run-h" style="cursor:pointer">
          ${statePill}
          <b>${esc(who)}</b>
          <div class="dlv-spacer"></div>
          ${r.shredded ? '' : `<button class="dlv-btn dlv-shred" data-session="${esc(r.session_id)}">Shred</button>`}
        </div>
        <div class="dlv-run-m">
          <span class="dlv-dim">${r.records} record${r.records === 1 ? '' : 's'}</span>
          ${r.refused ? `<span class="dlv-dim">${r.refused} refused</span>` : ''}
          <span class="dlv-dim dlv-mono">${esc(String(r.session_id).slice(0, 8))}</span>
        </div>
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

    async function openSession(session) {
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
      if (!await window.xnautConfirmDialog(`Destroy the sealing key for ${session}?`, 'Destroy key',
        'The arguments behind its records become unreadable by everyone, including us. '
        + 'The records stay and still verify. This cannot be undone.')) return;
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
      const next = await window.xnautPromptDialog(
        'Re-wrap every session key under which KEK?\n\nThe key must already exist in the HSM with encrypt and decrypt.',
        state.kek || '', 'Rotate');
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
      const err = state.error ? `<div class="dlv-err">${esc(state.error)}</div>` : '';
      // Tests renders both columns: its left column is the run list, not the
      // project list, and the project is chosen from the dropdown above it.
      if (state.tab === 'tests') renderTests();
      else if (state.tab === 'releases') renderReleases();
      else if (state.tab === 'evidence') renderEvidence();
      else renderReport();
      if (err) bodyEl.insertAdjacentHTML('afterbegin', err);
    }

    async function load() {
      state.error = '';
      const repo = repoOf(state.project);
      try {
        if (state.tab === 'tests') {
          state.records = (await invoke('sandbox_verify_records')) || [];
          // The denominator of "tickets verified": the tickets that moved
          // inside the selector's window.
          state.tickets = (state.project
            ? await invoke('pm_ticket_list', { project: state.project.key })
            : await invoke('pm_ticket_list', {})) || [];
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

    pane.querySelectorAll('.dlv-tabs button[data-tab]').forEach((b) => { b.onclick = () => setTab(b.dataset.tab); });
    // The Memory panel is built elsewhere (XNAUT-333) and exports this global.
    // Nothing assigns it yet, so the optional call is the whole contract: the
    // button does nothing until that panel lands, rather than throwing.
    pane.querySelector('.dlv-memory').onclick = () => { window.xnautOpenMemoryPanel?.(); };
    sinceEl.onchange = () => {
      state.since = sinceEl.value;
      if (state.tab === 'report') load();
      else if (state.tab === 'tests') renderTests();
    };
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
        if (next.ticket) { state.focusTicket = next.ticket; state.run = ''; state.life = null; state.raw = false; }
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
    typeOf, isTestFile, duration, outcomeOf,
    // Evidence view model, for scripts/evidence-view-check.cjs.
    dayOf, dayLabel, groupByDay, integrityVerdict, recordLine, clockOf, clockRange,
  };

  window.xnautCreateDeliveryPanel = createDeliveryPanel;
  window.xnautDestroyDeliveryPanel = destroyDeliveryPanel;
})();
