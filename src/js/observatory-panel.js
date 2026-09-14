// Observatory — the command deck (main panel tab, left menu above Tasks).
// Shows the MAX-plan budget up top, the selected project's zellij sessions with
// Connect / Kill / Open another session (XNAUT-340: they used to live on the
// Projects Overview tab, where they were the stale copy), and every running
// agent with elapsed/model/status and a per-row kill switch: terminal sessions
// (agent_sessions_list), persona/sandbox loom runs (loom_runs_list +
// loom_run_alive), and build worktree shells (loom_runs_list provider "build" +
// zellij_sessions, which are durable, so they survive a webview reload and
// can be re-attached).
//
// The agent rows are GROUPED BY PROJECT (XNAUT-340). Twenty-five flat rows say
// nothing; eight rows under one project name, open for three days, is a
// question. See `attribute()` for the three sources a row's project comes from.
(function () {
  'use strict';

  // Throws when the bridge is missing rather than returning undefined. The
  // `&&` guard this replaces made a missing bridge look SUCCESSFUL to
  // Promise.allSettled ({status:'fulfilled', value:undefined}), so the usage
  // cards blanked with nothing logged and no reason to show (XNAUT-257).
  const invoke = (...a) => {
    const core = window.__TAURI__ && window.__TAURI__.core;
    if (!core || !core.invoke) return Promise.reject(new Error('Tauri bridge unavailable'));
    return core.invoke(...a);
  };
  const esc = (s) => String(s == null ? '' : s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));

  // How much run history to pull before filtering to the live ones. The store
  // collapses by id, sorts newest-first and truncates to this limit BEFORE we
  // filter on status === 'started', so the window has to cover live runs AND
  // whatever finished records sit above them, or a running agent falls off the
  // deck entirely. It was 30, i.e. exactly zero headroom at 30 agents.
  const RUN_WINDOW = 200;

  // How many runs to ask the registry for, newest first. It only has to cover
  // the live ones plus whatever finished records sit above them.
  const REGISTRY_ROWS = 80;
  // How many ledger lines the band shows. Enough that a refusal written an hour
  // ago is still on screen next to the dispatches that followed it, few enough
  // that the deck does not become a log viewer — the Agent timeline is where
  // the whole history lives.
  const LEDGER_ROWS = 25;
  // How long one project-board read stays good. The board changes on human
  // timescales, and this panel repaints every 5s.
  const PROJECTS_TTL_MS = 30000;
  // The group a row lands in when all three attribution sources miss. It is a
  // real group, sorted last and always visible: a session nobody can attribute
  // is exactly the one worth seeing.
  const NO_PROJECT = '__none__';

  // Comparison key for a name: lowercase, every separator dropped, so
  // "CompanyManager", "company-manager" and "company_manager" are one name.
  const slug = (s) => String(s || '').toLowerCase().replace(/[^a-z0-9]+/g, '');
  // The agent prefix a hand-started session carries. `zellij::session_name`
  // builds "cl-<slug of the directory>" and the build rows below derive exactly
  // that from a cwd; this is the same rule read backwards, not a second guess.
  const stripAgent = (name) => String(name || '').replace(/^(cx|cl|pi|xnaut)-/, '');

  // Which project a running row belongs to, as a project key, or '' when
  // nothing can say. Three sources, used in this order, and never a guess past
  // the last one:
  //
  //   1. The run registry. `run_registry_list` gives each run's `project` and
  //      `zellij_session`; joining on the session name is exact for anything
  //      xNAUT dispatched.
  //   2. The working directory. A cwd inside a project's source_path belongs to
  //      that project, worktrees included, since they live under it. This is
  //      what covers build, local and sandbox rows.
  //   3. The session name. Strip the agent prefix and match the remainder
  //      against project keys, names and directory names, case-insensitively
  //      with separators collapsed. Most live sessions were started by hand,
  //      outside the registry, and `cx-CompanyManager` is all they carry.
  //
  // Prefix matching runs both ways on purpose: zellij truncates a session name
  // at 24 characters, so the name can be shorter than the project, and a name
  // like `cx-xnaut-safety-net` is longer than it. Three characters is the floor
  // for either direction, or a two-letter key would swallow half the machine.
  function attribute(row, ctx) {
    if (row.sess) {
      const fromRegistry = ctx.bySession.get(row.sess);
      if (fromRegistry) return fromRegistry;
    }
    const cwd = String(row.cwd || row.wt || '').replace(/\/+$/, '');
    if (cwd) {
      let best = '', bestLen = 0;
      for (const p of ctx.projects) {
        if (!p.path || p.path.length <= bestLen) continue;
        if (cwd === p.path || cwd.startsWith(p.path + '/')) { best = p.key; bestLen = p.path.length; }
      }
      if (best) return best;
    }
    if (row.sess) {
      const rest = slug(stripAgent(row.sess));
      if (rest.length >= 3) {
        let best = '', bestLen = 0;
        for (const p of ctx.projects) {
          for (const token of p.tokens) {
            if (token.length < 3 || token.length <= bestLen) continue;
            if (token === rest || token.startsWith(rest) || rest.startsWith(token)) { best = p.key; bestLen = token.length; }
          }
        }
        if (best) return best;
      }
    }
    return '';
  }

  // Rows into groups, biggest first, "No project" always last. Each entry keeps
  // the row's index in the flat list so the Kill / open / attach handlers stay
  // keyed the way they already are.
  function groupRows(rows, ctx) {
    const by = new Map();
    rows.forEach((r, i) => {
      const key = attribute(r, ctx) || NO_PROJECT;
      if (!by.has(key)) by.set(key, []);
      by.get(key).push({ r, i });
    });
    const label = (key) => (key === NO_PROJECT ? 'No project' : (ctx.names.get(key) || key));
    return [...by.entries()]
      .sort((a, b) => {
        if (a[0] === NO_PROJECT) return 1;
        if (b[0] === NO_PROJECT) return -1;
        return b[1].length - a[1].length || label(a[0]).localeCompare(label(b[0]));
      })
      .map(([key, items]) => ({ key, label: label(key), items }));
  }

  function elapsed(ms) {
    const s = Math.max(0, Math.floor((Date.now() - ms) / 1000));
    const h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60), ss = s % 60;
    return (h ? h + ':' + String(m).padStart(2, '0') : String(m)) + ':' + String(ss).padStart(2, '0');
  }
  // How a human reattaches to a live session. `zellij attach` directly, not the
  // old `just -g cc` wrappers: those are gone from the launch path (XNAUT-38
  // Phase 0), and `just -g codex` defaulted to `resume --last`, so following the
  // hint resumed some unrelated session instead of this one.
  const attachCmd = (sess) => 'zellij attach ' + sess;
  // For a session xNAUT did not name (a plain interactive agent tab), the honest
  // hint is the runner itself.
  const runnerCmd = (model) => { const m = String(model || ''); return /^codex/.test(m) ? 'codexps' : /^pi/.test(m) ? 'pi' : 'claudeps'; };
  const headlessCmd = (model) => { const m = String(model || ''); return /^codex/.test(m) ? 'codex exec' : /^pi/.test(m) ? 'pi' : 'claude -p'; };

  let styled = false;
  function injectStyles() {
    if (styled) return; styled = true;
    const st = document.createElement('style');
    st.textContent = `
.obs { flex:1 1 0%; width:100%; height:100%; min-width:0; min-height:0; overflow-y:auto; display:flex; flex-direction:column; gap:18px; padding:26px 32px; background:var(--background,#0f1115); }
.obs-head { display:flex; align-items:center; gap:14px; }
.obs-title { display:flex; flex-direction:column; gap:3px; }
.obs-title h2 { margin:0; font-size:22px; font-weight:700; letter-spacing:-.01em; color:var(--foreground,#fafafa); }
.obs-title p { margin:0; font-size:12px; color:var(--muted-foreground,#a1a1a1); }
.obs-actions { margin-left:auto; display:flex; gap:10px; }
.obs-btn { display:flex; align-items:center; gap:7px; height:34px; padding:0 14px; border-radius:9px; border:1px solid var(--border,#262626); background:transparent; color:var(--foreground); font:inherit; font-size:12px; font-weight:600; cursor:pointer; }
.obs-btn.danger { border-color:rgba(233,139,131,.4); color:#e98b83; }
.obs-btn.danger:hover { background:rgba(233,139,131,.1); }
.obs-btn.primary { border:0; background:var(--xnaut-yellow,#f5b840); color:#171717; font-weight:700; font-size:12.5px; padding:0 16px; }
.obs-strip { display:flex; gap:12px; align-items:stretch; }
.obs-card { background:var(--card,#171717); border:1px solid var(--border,#262626); border-radius:12px; padding:14px 16px; display:flex; flex-direction:column; gap:8px; }
.obs-card .k { font-size:9.5px; letter-spacing:.09em; font-weight:650; color:var(--muted-foreground,#a1a1a1); text-transform:uppercase; }
.obs-big { display:flex; align-items:baseline; gap:6px; }
.obs-big b { font-family:ui-monospace,Menlo,monospace; font-size:30px; font-weight:700; color:#7ec98f; }
.obs-big b.warn { color:var(--xnaut-yellow,#f5b840); } .obs-big b.crit { color:#e98b83; }
.obs-big.small b { font-size:22px; color:var(--foreground,#fafafa); }
.obs-big span { font-size:10.5px; color:var(--muted-foreground,#a1a1a1); }
.obs-bar { height:6px; border-radius:3px; background:var(--secondary,#262626); overflow:hidden; }
.obs-bar i { display:block; height:100%; border-radius:3px; background:var(--xnaut-yellow,#f5b840); }
.obs-bar i.good { background:#7ec98f; } .obs-bar i.cyan { background:#5bd1c9; } .obs-bar i.crit { background:#e98b83; }
.obs-mrow { display:flex; align-items:center; gap:10px; }
.obs-mrow .nm { width:62px; flex-shrink:0; font-family:ui-monospace,Menlo,monospace; font-size:10.5px; color:var(--foreground); }
.obs-mrow .pc { width:34px; flex-shrink:0; text-align:right; font-family:ui-monospace,Menlo,monospace; font-size:10.5px; color:var(--muted-foreground); }
.obs-mrow .obs-bar { flex:1 1 auto; }
.obs-table { background:var(--card,#171717); border:1px solid var(--border,#262626); border-radius:12px; overflow-y:auto; display:flex; flex-direction:column; }
.obs-thead { display:flex; align-items:center; gap:10px; padding:12px 16px; border-bottom:1px solid var(--border,#262626); }
.obs-thead .k { font-size:10px; letter-spacing:.09em; font-weight:650; color:var(--muted-foreground); text-transform:uppercase; }
.obs-thead .n { font-family:ui-monospace,Menlo,monospace; font-size:10px; color:var(--xnaut-yellow,#f5b840); }
.obs-thead .r { margin-left:auto; font-size:10.5px; color:var(--muted-foreground); }
.obs-cols, .obs-row { display:flex; align-items:center; gap:12px; padding:8px 16px; }
.obs-cols { border-bottom:1px solid #1e2026; }
.obs-cols span { font-size:9.5px; font-weight:600; letter-spacing:.07em; color:#6b6f78; text-transform:uppercase; }
.obs-row { padding:11px 16px; border-bottom:1px solid #1e2026; }
.obs-row:last-child { border-bottom:0; }
.c-type { width:64px; flex-shrink:0; }
.c-name { flex:1 1 auto; min-width:0; display:flex; flex-direction:column; gap:2px; }
.c-name .t { font-size:12.5px; font-weight:600; color:var(--foreground); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
.c-name .s { font-size:10.5px; color:var(--muted-foreground); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
.c-model { width:110px; flex-shrink:0; font-family:ui-monospace,Menlo,monospace; font-size:10.5px; color:var(--foreground); }
.c-res { width:120px; flex-shrink:0; display:flex; align-items:center; gap:5px; font-family:ui-monospace,Menlo,monospace; font-size:9px; color:var(--muted-foreground); }
.c-res .obs-bar { width:40px; height:5px; }
.c-elapsed { width:66px; flex-shrink:0; font-family:ui-monospace,Menlo,monospace; font-size:11px; color:var(--foreground); }
.c-status { width:82px; flex-shrink:0; display:flex; align-items:center; gap:6px; font-size:11px; color:var(--foreground); }
.c-status .dot { width:7px; height:7px; border-radius:50%; background:#6b6f78; }
.c-status .dot.working { background:var(--xnaut-yellow,#f5b840); }
.c-status .dot.waiting, .c-status .dot.idle { background:#5bd1c9; }
.c-status .dot.blocked, .c-status .dot.permission { background:#e98b83; }
.c-status .dot.done { background:#7ec98f; }
.c-kill { width:58px; flex-shrink:0; }
.obs-chip { display:inline-block; font-family:ui-monospace,Menlo,monospace; font-size:9px; border-radius:5px; padding:2px 6px; }
.obs-chip.sandbox { color:#5bd1c9; border:1px solid #234a48; }
.obs-chip.local { color:#f5b840; border:1px solid rgba(245,184,64,.35); }
.obs-open { font-family:ui-monospace,Menlo,monospace; font-size:10px; color:var(--xnaut-yellow,#f5b840); background:rgba(245,184,64,.1); border:1px solid rgba(245,184,64,.3); border-radius:5px; padding:1px 6px; cursor:pointer; }
.obs-open:hover { background:rgba(245,184,64,.2); }
.c-name.obs-clickable { cursor:pointer; }
.c-name.obs-clickable:hover .t { color:var(--xnaut-yellow,#f5b840); }
.obs-chip.terminal { color:var(--xnaut-yellow,#f5b840); border:1px solid #4a3d22; }
.obs-chip.zellij { color:#9a9faa; border:1px solid #34373f; }
.obs-row .dot.open { background:#5bc8ff; }
.obs-kill { font-size:10px; font-weight:600; color:#e98b83; border:1px solid rgba(233,139,131,.35); border-radius:6px; padding:3px 9px; background:transparent; cursor:pointer; font-family:inherit; }
.obs-kill:hover { background:rgba(233,139,131,.12); }
.obs-empty { padding:22px 16px; font-size:12px; color:var(--muted-foreground); }
.obs-pills { display:flex; align-items:center; gap:8px; padding:13px 16px; flex-wrap:wrap; }
.obs-pill-group { display:flex; align-items:center; gap:6px; flex-wrap:wrap; }
.obs-pill-group > b { font-size:10.5px; font-weight:700; color:var(--muted-foreground,#a1a1a1); text-transform:uppercase; letter-spacing:.04em; margin-right:2px; }
.obs-pill { display:inline-flex; align-items:center; gap:7px; border:1px solid var(--border,#262626); border-radius:999px; padding:5px 12px; font-family:ui-monospace,Menlo,monospace; font-size:10.5px; color:var(--muted-foreground); }
.obs-pill .dot { width:7px; height:7px; border-radius:50%; background:#6b6f78; }
.obs-pill.running { border-color:#4a3d22; background:#1c1910; color:var(--foreground); }
.obs-pill.running .dot { background:var(--xnaut-yellow,#f5b840); }
.obs-pill.done { border-color:#24402c; color:#7ec98f; } .obs-pill.done .dot { background:#7ec98f; }
.obs-pill.failed { border-color:#3a2a2a; color:#e98b83; } .obs-pill.failed .dot { background:#e98b83; }
.obs-counts { margin-left:auto; display:flex; align-items:center; gap:14px; font-family:ui-monospace,Menlo,monospace; font-size:10.5px; }
.obs-counts .run { color:var(--xnaut-yellow,#f5b840); } .obs-counts .q { color:var(--muted-foreground); } .obs-counts .ok { color:#7ec98f; } .obs-counts .bad { color:#e98b83; }
.obs-counts .cap { font-family:inherit; font-size:10px; font-weight:600; color:var(--muted-foreground); }
.obs-why { padding:0 !important; font-size:10px; line-height:1.3; opacity:.75; display:block; overflow:hidden; text-overflow:ellipsis; }
/* Project groups. Same idiom as Delivery's release groups (.dlv-relgrp):
   caret, name, count on the right, open by default, click to fold. */
.obs-grp { display:flex; align-items:center; gap:7px; width:100%; text-align:left; padding:8px 16px;
  border:0; border-bottom:1px solid #1e2026; background:rgba(255,255,255,.02);
  color:var(--muted-foreground,#a1a1a1); font:inherit; font-size:12px; cursor:pointer; }
.obs-grp:hover { background:rgba(255,255,255,.05); }
.obs-grp.open { color:var(--foreground,#fafafa); }
.obs-grp b { color:var(--foreground,#fafafa); font-weight:600; }
.obs-caret { width:10px; flex-shrink:0; font-size:10px; }
.obs-grp-n { margin-left:auto; font-size:11px; color:var(--muted-foreground,#a1a1a1); }
.obs-sess-row { display:flex; align-items:center; gap:12px; padding:11px 16px; border-bottom:1px solid #1e2026; }
.obs-sess-row:last-child { border-bottom:0; }
.obs-sess-row .nm { flex:1 1 auto; min-width:0; display:flex; flex-direction:column; gap:2px; }
.obs-sess-row .nm .t { font-size:12.5px; font-weight:600; color:var(--foreground); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
.obs-sess-row .nm .s { font-size:10.5px; color:var(--muted-foreground); }
.obs-sess-new { display:flex; align-items:center; gap:8px; padding:12px 16px; flex-wrap:wrap; border-top:1px solid #1e2026; }
.obs-select { height:30px; border-radius:8px; border:1px solid var(--border,#262626); background:transparent; color:var(--foreground); font:inherit; font-size:11.5px; padding:0 8px; }
.obs-hint { font-size:10.5px; color:var(--muted-foreground); }
/* The instance card and the ledger band (XNAUT-370). The signature chip is
   deliberately the same object in both places: the reader learns "this is me"
   from the card and then recognises it, or fails to, on every line below. */
.obs-sig { display:inline-flex; align-items:center; gap:5px; font-family:ui-monospace,Menlo,monospace;
  font-size:9px; border-radius:5px; padding:2px 6px; border:1px solid #34373f; color:#9a9faa; white-space:nowrap; }
.obs-sig.mine { border-color:rgba(245,184,64,.35); color:var(--xnaut-yellow,#f5b840); }
.obs-sig.drift { border-color:rgba(233,139,131,.45); color:#e98b83; }
.obs-sig.unknown { border-style:dashed; }
.obs-role { font-size:11px; font-weight:650; color:var(--foreground,#fafafa); text-transform:capitalize; }
.obs-led { display:flex; align-items:center; gap:10px; padding:9px 16px; border-bottom:1px solid #1e2026; }
.obs-led:last-child { border-bottom:0; }
.obs-led .kd { width:132px; flex-shrink:0; font-family:ui-monospace,Menlo,monospace; font-size:10px; color:#9a9faa;
  white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
.obs-led .kd.refused { color:#e98b83; }
.obs-led .tk { width:88px; flex-shrink:0; font-family:ui-monospace,Menlo,monospace; font-size:10.5px; color:var(--xnaut-yellow,#f5b840); }
.obs-led .dt { flex:1 1 auto; min-width:0; font-size:11.5px; color:var(--foreground); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
.obs-led .wh { width:52px; flex-shrink:0; text-align:right; font-family:ui-monospace,Menlo,monospace; font-size:10px; color:var(--muted-foreground); }`;
    document.head.appendChild(st);
  }

  async function createObservatoryPanel(tabId, parentContainer, opts) {
    injectStyles();
    const pane = document.createElement('div');
    pane.className = 'obs';
    pane.innerHTML = `
      <div class="obs-head">
        <div class="obs-title"><h2>Observatory</h2><p>Every agent, every sandbox — one deck.</p></div>
        <div class="obs-actions">
          <button class="obs-btn" data-refresh title="Refetch plan usage and the running-agent list now">↻ Refresh</button>
          <button class="obs-btn danger" data-stopall>■ Stop all</button>
        </div>
      </div>
      <div class="obs-strip" data-strip></div>
      <div class="obs-table" data-sessions>
        <div class="obs-thead"><span class="k">Sessions</span><span class="n" data-sess-count></span>
          <span class="r"><select class="obs-select" data-sess-project aria-label="Project for these sessions"></select></span></div>
        <div data-sess-list></div>
      </div>
      <div class="obs-table">
        <div class="obs-thead"><span class="k">Running agents</span><span class="n" data-count></span><span class="r">terminal + sandbox · live</span></div>
        <div class="obs-cols">
          <span class="c-type">Type</span><span style="flex:1 1 auto">Agent / task</span>
          <span class="c-model">Model</span><span class="c-res">Resources</span>
          <span class="c-elapsed">Elapsed</span><span class="c-status">Status</span><span class="c-kill">Kill</span>
        </div>
        <div data-rows></div>
      </div>
      <div class="obs-table">
        <div class="obs-thead"><span class="k">Dispatched runs</span><span class="n" data-swarm-label></span>
          <span class="obs-counts" data-swarm-counts></span></div>
        <div class="obs-pills" data-swarm-pills><span class="obs-empty" style="padding:0">Nothing dispatched. Ask NautBot to work a project's open tickets.</span></div>
      </div>
      <div class="obs-table">
        <div class="obs-thead"><span class="k">Ledger</span><span class="n" data-ledger-count></span>
          <span class="r">every line signed by the instance and build that wrote it</span></div>
        <div data-ledger-list></div>
      </div>`;
    parentContainer.appendChild(pane);

    pane.querySelector('[data-stopall]').onclick = async () => {
      if (window.xnautBuild && window.xnautBuild.stopAll) await window.xnautBuild.stopAll();
      for (const r of lastRows) { await killRow(r); }
      refresh();
    };

    // ---- which instance this is (XNAUT-370) ----
    //
    // Held on the panel rather than fetched per row, because every ledger line
    // below is read AGAINST it: a line is "this machine" or it is somewhere
    // else, and that comparison is the whole point of the band. Null until the
    // first strip render answers, and a null compares equal to nothing, so an
    // unanswered stamp renders every line as unattributed rather than as mine.
    let me = null, meErr = null;

    // One instance's signature: the key, its role and the build it ran.
    //
    // Short key on purpose. A uuid is unreadable at a glance and the question
    // this answers is never "which uuid" but "the same one as the others, or a
    // different one" — eight characters settle that, and the full value is on
    // the title for anyone who needs to match it against settings.json.
    const shortId = (id) => String(id || '').slice(0, 8);
    function sig(row) {
      const id = String(row.instance || '').trim();
      const version = String(row.version || '').trim();
      if (!id && !version) {
        // Written before the stamp existed, or before the app minted a key.
        // Said plainly: an unsigned line is not this machine's line, and
        // rendering it as if it were is the attribution bug, not the fix.
        return '<span class="obs-sig unknown" title="This line was written before instance signatures existed, so which machine wrote it is not recorded.">unsigned</span>';
      }
      const mine = me && id && id === me.id;
      const drift = me && version && me.version && version !== me.version;
      const cls = drift ? 'drift' : mine ? 'mine' : '';
      const title = `instance ${id || 'unknown'}${row.role ? ' · ' + row.role : ''}`
        + `${version ? ' · xNAUT ' + version : ''}`
        + (drift ? ` — a different build from this one (${me.version})` : mine ? ' — this machine' : '');
      return `<span class="obs-sig ${cls}" title="${esc(title)}">${esc(shortId(id) || '?')}${version ? ' · ' + esc(version) : ''}</span>`;
    }

    function instanceCard() {
      if (!me) {
        return `<div class="obs-card" style="width:220px;flex:0 0 auto">
          <span class="k">This instance</span><div class="obs-big small"><b>—</b></div>
          ${why(meErr, 'the instance has not answered yet')}</div>`;
      }
      // The role is the headline, not the id: it is the thing that decides
      // whether this machine may start work, and the thing an owner looking at
      // an idle board needs to see first.
      const hint = me.role === 'fleet' ? 'dispatches and verifies'
        : me.role === 'sandbox' ? 'verifies only'
        : me.role === 'workstation' ? 'plans and reviews · never dispatches'
        : 'unknown role';
      return `<div class="obs-card" style="width:240px;flex:0 0 auto" title="Set instance.role in settings.json: fleet, workstation or sandbox.">
        <span class="k">This instance</span>
        <div class="obs-big small"><b class="obs-role" style="font-size:17px">${esc(me.role || 'unknown')}</b>
          <span>${esc(me.machine || '')}</span></div>
        <div style="font-size:10.5px;color:var(--muted-foreground,#a1a1a1)">${esc(hint)}</div>
        ${sig({ instance: me.id, role: me.role, version: me.version })}</div>`;
    }

    // ---- budget strip ----
    let budgetCritNotified = false;
    let lastC = null, lastX = null; // last GOOD values — a 429/timeout must not blank the cards
    let lastCErr = null, lastXErr = null; // why there is nothing to show
    // A blank card and a broken fetch used to look identical (XNAUT-257). Every
    // empty card now states which of the two it is, so "no data" is a claim we
    // can stand behind rather than a shrug.
    const why = (err, empty) => `<span class="obs-empty obs-why" title="${esc(err || empty)}">${esc(err ? String(err).slice(0, 60) : empty)}</span>`;
    async function renderStrip() {
      const host = pane.querySelector('[data-strip]'); if (!host) return;
      const [claude, codex, stamp] = await Promise.allSettled([
        invoke('max_usage', { account: null }), invoke('codex_usage'), invoke('instance_stamp'),
      ]);
      if (stamp.status === 'fulfilled' && stamp.value) {
        const first = !me;
        me = stamp.value; meErr = null;
        // The strip and the ledger band are on different clocks (60s and 5s),
        // and the band renders every line AGAINST `me`. Without this the first
        // paint — which happens before this slower call resolves — marks every
        // line as somewhere else, including this machine's own, and the reader
        // sees a fleet-wide drift that is really just a race.
        if (first) renderLedger();
      } else if (stamp.status === 'rejected') { meErr = String(stamp.reason); console.warn('[obs] instance_stamp:', stamp.reason); }
      if (claude.status === 'fulfilled' && claude.value) { lastC = claude.value; lastCErr = null; }
      else if (claude.status === 'rejected') { lastCErr = String(claude.reason); console.warn('[obs] max_usage:', claude.reason); }
      if (codex.status === 'fulfilled' && codex.value) { lastX = codex.value; lastXErr = null; }
      else if (codex.status === 'rejected') { lastXErr = String(codex.reason); console.warn('[obs] codex_usage:', codex.reason); }
      const c = lastC, x = lastX;
      const left = c ? Math.max(0, Math.round(100 - c.seven_day_pct)) : null;
      const cls = left == null ? '' : left <= 10 ? 'crit' : left <= 25 ? 'warn' : '';
      if (c && c.severity === 'critical' && !budgetCritNotified) {
        budgetCritNotified = true;
        if (window.xnautNotify) window.xnautNotify('MAX plan budget critical', 'Only ' + left + '% of the week left.');
      }
      const models = (c && c.per_model || []).map((m) => `
        <div class="obs-mrow"><span class="nm">${esc(m.name)}</span>
        <span class="obs-bar"><i class="cyan" style="width:${Math.min(100, Math.round(m.percent))}%"></i></span>
        <span class="pc">${Math.round(m.percent)}%</span></div>`).join('');
      host.innerHTML = `
        ${instanceCard()}
        <div class="obs-card" style="width:250px;flex:0 0 auto">
          <span class="k">MAX plan · week left</span>
          <div class="obs-big"><b class="${cls}">${left == null ? '—' : left + '%'}</b><span>${c && c.seven_day_resets_at ? 'resets ' + esc(String(c.seven_day_resets_at).slice(5, 16).replace('T', ' ')) : ''}</span></div>
          ${c ? `<div class="obs-bar"><i class="${cls || 'good'}" style="width:${left}%"></i></div>`
              : why(lastCErr, 'no plan data yet')}
        </div>
        <div class="obs-card" style="width:200px;flex:0 0 auto">
          <span class="k">5-hour window</span>
          <div class="obs-big small"><b>${c ? Math.round(c.five_hour_pct) + '%' : '—'}</b><span>${c ? 'used' + (c.five_hour_resets_at ? ' · resets ' + esc(String(c.five_hour_resets_at).slice(11, 16)) : '') : ''}</span></div>
          ${c ? `<div class="obs-bar"><i style="width:${Math.min(100, Math.round(c.five_hour_pct))}%"></i></div>`
              : why(lastCErr, 'no plan data yet')}
        </div>
        <div class="obs-card" style="flex:1 1 auto;min-width:0">
          <span class="k">Per model · weekly</span>${models
            || why(lastCErr, c ? 'no per-model limits on this plan' : 'no plan data yet')}
        </div>
        ${spendCard(c)}
        <div class="obs-card" style="width:200px;flex:0 0 auto">
          <span class="k">Codex${x && x.plan_type ? ' · ' + esc(x.plan_type) : ''}</span>
          <div class="obs-big small"><b>${x && x.secondary ? Math.round(x.secondary.used_percent) + '%' : '—'}</b><span>${x && x.secondary ? esc(x.secondary.window_label || 'weekly') + ' used' : ''}</span></div>
          ${x && x.secondary ? `<div class="obs-bar"><i style="width:${Math.min(100, Math.round(x.secondary.used_percent))}%"></i></div>`
              : why(lastXErr, x ? 'plan reports no weekly window' : 'no codex run logged yet')}
        </div>${lastProjectCard()}`;
      const lp = host.querySelector('.obs-lastproj');
      if (lp) lp.onclick = () => {
        let d = null; try { d = JSON.parse(localStorage.getItem('xnaut-nf-last') || 'null'); } catch (_) {}
        if (!d) return;
        try { window.xnautHomeContext && window.xnautHomeContext(); } catch (_) {}
        if (window.xnautAttachProjectManagementTab) window.xnautAttachProjectManagementTab({ project: d.key, section: 'nautflow', flowStage: d.stageKey });
      };
    }

    // The only real money on the Claude side: extra-usage credits actually
    // charged once plan limits are passed. Anthropic reports it; we do not
    // price anything ourselves. It is deliberately NOT labelled "cost of this
    // work" — on a MAX plan the marginal cost of ordinary work is zero, and a
    // headline "$0.00" against three busy agents would be a lie of framing.
    // The notional "what this would have cost on the API" figure is Codex-only
    // (codex_spend.rs) and stays in the footer where it is captioned as such.
    function spendCard(c) {
      const s = c && c.spend;
      const money = (v) => (c.spend.currency === 'USD' ? '$' : '') + v.toFixed(2)
        + (c.spend.currency === 'USD' ? '' : ' ' + esc(c.spend.currency));
      return `
        <div class="obs-card" style="width:210px;flex:0 0 auto" title="Extra-usage credits billed beyond your plan's included limits this period, as reported by Anthropic. Work inside the plan limits adds nothing here.">
          <span class="k">Extra usage · billed</span>
          ${!s
            ? `<div class="obs-big small"><b>—</b></div>`
              + why(lastCErr, c ? 'plan reports no spend block' : 'no plan data yet')
            : `<div class="obs-big small"><b class="${s.percent >= 85 ? 'crit' : s.percent >= 60 ? 'warn' : ''}">${money(s.used)}</b>`
              + `<span>${s.limit != null ? 'of ' + money(s.limit) : 'no cap set'}${s.enabled ? '' : ' · off'}</span></div>`
              + `<div class="obs-bar"><i class="${s.percent >= 85 ? 'crit' : s.percent >= 60 ? 'warn' : 'good'}" style="width:${Math.min(100, Math.round(s.percent))}%"></i></div>`}
        </div>`;
    }

    // Quick tile: jump straight back to the NAUT-Flow page of the last project.
    function fmtAgo(ms) {
      const s = Math.max(0, Math.floor((Date.now() - ms) / 1000));
      if (s < 90) return 'just now';
      if (s < 3600) return Math.round(s / 60) + 'm ago';
      if (s < 86400) return Math.round(s / 3600) + 'h ago';
      return Math.round(s / 86400) + 'd ago';
    }
    function lastProjectCard() {
      let d = null; try { d = JSON.parse(localStorage.getItem('xnaut-nf-last') || 'null'); } catch (_) {}
      if (!d || !d.key) return '';
      return `
        <div class="obs-card obs-lastproj" style="width:230px;flex:0 0 auto;cursor:pointer;border-color:rgba(245,184,64,.35)" title="Open the NAUT-Flow page of ${esc(d.name)}">
          <span class="k">Last project · continue</span>
          <div class="obs-big small"><b style="font-size:17px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;max-width:190px;">${esc(d.name)}</b></div>
          <div style="font-size:10.5px;color:var(--muted-foreground,#a1a1a1);overflow:hidden;text-overflow:ellipsis;white-space:nowrap;">${esc(d.stage || '')} · ${fmtAgo(d.at || Date.now())}</div>
          <span style="font-size:11px;font-weight:650;color:var(--xnaut-yellow,#f5b840);">Open NAUT-Flow →</span>
        </div>`;
    }

    // ---- the project index every attribution source joins against ----
    const ctx = { projects: [], names: new Map(), bySession: new Map() };
    let projectsAt = 0;
    let projectsPromise = null;
    // The PROMISE is what is cached, not the answer. The sessions band and the
    // row table both ask for this at panel start; caching the answer handed the
    // second caller the empty list the first was still fetching, and the band
    // then filled its picker with "No projects" and stayed that way.
    function loadProjects() {
      if (projectsPromise && Date.now() - projectsAt < PROJECTS_TTL_MS) return projectsPromise;
      projectsAt = Date.now();
      projectsPromise = (async () => {
        let list = [];
        try { list = (await invoke('pm_project_list')) || []; } catch (e) { console.warn('[obs] pm_project_list:', e); }
        ctx.names = new Map(list.map((p) => [p.key, p.name || p.key]));
        ctx.projects = list.map((p) => ({
          key: p.key,
          path: String(p.source_path || '').replace(/\/+$/, ''),
          tokens: [...new Set([slug(p.key), slug(p.name), slug(String(p.source_path || '').split('/').pop())].filter(Boolean))],
        }));
        return list;
      })();
      return projectsPromise;
    }

    // Attribution source 1. `run_registry_list` is the registry's own list
    // command (XNAUT-345). This used to be a directory walk over
    // <home>/.config/xnaut/registry/*.run.json, which made run_control.rs's
    // on-disk format a dependency of this panel: renaming a field there would
    // have emptied the grouping here with nothing to see. One call now, and no
    // cache, because one call is what the cache existed to avoid.
    async function loadRegistry() {
      try {
        const rows = (await invoke('run_registry_list', { limit: REGISTRY_ROWS })) || [];
        const map = new Map();
        for (const r of rows) {
          if (r && r.zellij_session && r.project) map.set(r.zellij_session, r.project);
        }
        ctx.bySession = map;
      } catch (_) { /* no registry here: sources 2 and 3 still answer */ }
    }

    // ---- sessions (moved here from the Projects Overview tab, XNAUT-340) ----
    // Starting a session is the same act as watching one, so the provider pair
    // and "Open another session" came along with the list.
    const LOCAL_PROVIDERS = ['lmstudio', 'ollama'];

    // Ported from project-management-panel.js: env goes through the config and
    // never into the command string, because zellij serializes what it ran to
    // disk and an API key must not land there.
    async function startShell(cwd, command, env) {
      const full = 'export PATH="/opt/homebrew/bin:/usr/local/bin:$HOME/.local/bin:$PATH"; ' + command;
      const cfg = { program: 'sh', args: ['-c', full], workingDir: cwd };
      if (env) cfg.env = env;
      const res = await invoke('create_command_session', { config: cfg });
      return res.session_id || res.sessionId || res.id;
    }

    async function providerEnvFor(provider, model) {
      if (!provider) return null;
      let st;
      try { st = await invoke('settings_get'); } catch (_) { return null; }
      const p = ((st && st.llm_providers) || []).find((x) => x && x.name === provider);
      const endpoint = (p && p.endpoint) || (st && st.llm && st.llm.provider === provider ? st.llm.endpoint : '');
      if (!endpoint) return null;
      const base = String(endpoint).replace(/\/+$/, '').replace(/\/v1$/, '');
      const key = (p && p.api_key) || (LOCAL_PROVIDERS.includes(provider) ? 'local' : '');
      if (!key) return null; // a remote provider with no key would fail obscurely
      const env = { ANTHROPIC_BASE_URL: base, ANTHROPIC_API_KEY: key };
      const chosen = model || (st && st.llm && st.llm.model) || '';
      if (chosen) env.ANTHROPIC_MODEL = chosen;
      return env;
    }

    // Attaching and OPENING are not the same command: `attach --create` makes a
    // session with a plain shell in it and never starts the agent.
    async function openNewSession(project, provider, model) {
      const name = 'cl-' + String((project && project.name) || 'session');
      const cwd = (project && project.source_path) || '~/';
      try {
        let env = await providerEnvFor(provider, model);
        if (!provider || provider === 'nautgate') {
          const runId = (globalThis.crypto && globalThis.crypto.randomUUID)
            ? globalThis.crypto.randomUUID()
            : `${Date.now()}-${Math.random().toString(16).slice(2)}`;
          const base = await invoke('nautgate_max_launch_register', { project: cwd, nativeSession: name, runId });
          // Keep Claude Code's OAuth authentication: injecting an API key here
          // recreates the cache-loss bug.
          env = { ANTHROPIC_BASE_URL: base };
          if (model) env.ANTHROPIC_MODEL = model;
        }
        const open = await invoke('zellij_open_command', {
          session: name, cwd, command: "zsh -ic 'claude; exec zsh'",
        });
        if (window.xnautFocusTabForSession && window.xnautFocusTabForSession(open.name)) return;
        const sessionId = await startShell(cwd, open.command, env);
        // open.name, not name: zellij caps session names at 24 characters.
        if (window.xnautAttachAgentTab) window.xnautAttachAgentTab(sessionId, open.name, open.name);
        if (project && project.name) {
          try {
            await invoke('tasks_create_project', { name: project.name, path: cwd === '~/' ? null : cwd });
            if (window.xnautSidebarRefresh) window.xnautSidebarRefresh();
          } catch (e) { console.warn('[obs] could not register project in the sidebar:', e); }
        }
      } catch (e) {
        console.error('[obs] open session failed:', e);
      }
    }

    const agentOf = (name) => {
      const m = /^([a-z]{2,4})-/.exec(String(name || ''));
      return ({ cl: 'Claude Code', cx: 'Codex', pi: 'Pi' })[m && m[1]] || (m && m[1]) || 'agent';
    };

    // The project this band is scoped to: the sidebar's selection when there is
    // one, otherwise the first project, and always overridable by the picker.
    function preferredProject(list) {
      const wantKey = (opts && opts.project)
        || (typeof window.xnautActiveProjectKey === 'function' ? window.xnautActiveProjectKey() : null);
      const byKey = wantKey && list.find((p) => p.key === wantKey);
      if (byKey) return byKey;
      const wantPath = typeof window.xnautActiveProjectPath === 'function' ? window.xnautActiveProjectPath() : null;
      const byPath = wantPath && list.find((p) => String(p.source_path || '').replace(/\/+$/, '') === String(wantPath).replace(/\/+$/, ''));
      return byPath || list[0] || null;
    }

    async function renderSessions() {
      const band = pane.querySelector('[data-sessions]'); if (!band) return;
      const sel = band.querySelector('[data-sess-project]');
      const list = band.querySelector('[data-sess-list]');
      const count = band.querySelector('[data-sess-count]');
      const projects = await loadProjects();
      // Filled once, so the reader's choice survives a repaint, but only once
      // there is something to fill it with. An empty first answer would
      // freeze the picker on "No projects" for the life of the panel.
      if (sel && sel.dataset.filled !== '1' && projects.length) {
        sel.innerHTML = projects.map((p) => `<option value="${esc(p.key)}">${esc(p.name || p.key)}</option>`).join('');
        sel.dataset.filled = '1';
        const want = preferredProject(projects);
        if (want) sel.value = want.key;
        sel.onchange = () => renderSessions();
      }
      const project = projects.find((p) => p.key === (sel && sel.value)) || projects[0] || null;
      if (!project) {
        if (count) count.textContent = '';
        list.innerHTML = '<div class="obs-empty">No projects yet, so no project sessions to show.</div>';
        return;
      }
      // Same name rule as the grouping below (source 3), so the band and the
      // table cannot disagree about which sessions are this project's.
      const tokens = (ctx.projects.find((p) => p.key === project.key) || { tokens: [] }).tokens;
      let sessions = [];
      try {
        sessions = ((await invoke('zellij_sessions_info')) || []).filter((z) => {
          const rest = slug(stripAgent(z && z.name));
          if (rest.length < 3) return false;
          return tokens.some((t) => t.length >= 3 && (t === rest || t.startsWith(rest) || rest.startsWith(t)));
        });
      } catch (_) { /* zellij absent, so fall through to the empty state */ }
      const running = sessions.filter((z) => !z.exited).length;
      if (count) count.textContent = sessions.length ? `${running} running · ${sessions.length - running} resumable` : 'none';

      const opener = `<div class="obs-sess-new">
        <select class="obs-select" data-sess-provider aria-label="Provider for a new session"><option value="">Default provider</option></select>
        <select class="obs-select" data-sess-model aria-label="Model for a new session"><option value="">Provider default</option></select>
        <button class="obs-btn${sessions.length ? '' : ' primary'}" data-sess-open>${sessions.length ? 'Open another session' : 'Open a new session'}</button>
        <span class="obs-hint" data-sess-hint></span></div>`;
      list.innerHTML = (sessions.length
        ? sessions.map((z) => `
          <div class="obs-sess-row">
            <span class="obs-chip zellij">${esc(String(z.name || '').slice(0, 2).toUpperCase())}</span>
            <div class="nm"><span class="t">${esc(z.name)}</span>
              <span class="s">${esc(agentOf(z.name))} · ${z.exited ? 'exited · resumable' : 'running'}</span></div>
            <button class="obs-btn" data-sess-attach="${esc(z.name)}">${z.exited ? 'Resume' : 'Connect'}</button>
            <button class="obs-kill" data-sess-kill="${esc(z.name)}" title="Delete this session">Kill</button>
          </div>`).join('')
        : '<div class="obs-empty">No session for this project yet.</div>') + opener;

      list.querySelectorAll('[data-sess-attach]').forEach((b) => {
        b.onclick = () => {
          const name = b.dataset.sessAttach;
          if (window.xnautFocusTabForSession && window.xnautFocusTabForSession(name)) return;
          if (window.xnautOpenZellijSession) window.xnautOpenZellijSession(name);
          else console.error('[obs] xnautOpenZellijSession missing');
        };
      });
      // Killing is destructive and confirm() is a no-op in Tauri's WKWebView, so
      // the button arms itself instead: first click asks, second click does it.
      // Killing an agent from here must not be easier than from the old place.
      list.querySelectorAll('[data-sess-kill]').forEach((b) => {
        b.onclick = async () => {
          const name = b.dataset.sessKill;
          if (b.dataset.armed !== '1') {
            b.dataset.armed = '1';
            b.textContent = 'Kill?';
            setTimeout(() => {
              if (!b.isConnected || b.dataset.armed !== '1') return;
              b.dataset.armed = '';
              b.textContent = 'Kill';
            }, 4000);
            return;
          }
          b.disabled = true;
          b.textContent = 'Killing…';
          try {
            await invoke('zellij_delete_session', { name });
            if (window.xnautCloseTabForSession) window.xnautCloseTabForSession(name);
            await renderSessions();
            if (window.xnautSidebarRefresh) window.xnautSidebarRefresh();
          } catch (e) {
            console.error('[obs] kill session failed:', e);
            b.disabled = false;
            b.textContent = 'Kill';
            b.dataset.armed = '';
          }
        };
      });

      const openBtn = list.querySelector('[data-sess-open]');
      if (openBtn) {
        const provSel = list.querySelector('[data-sess-provider]');
        const modelSel = list.querySelector('[data-sess-model]');
        const hint = list.querySelector('[data-sess-hint]');
        // The same provider list as the New project form, so neither can go
        // quietly missing an option the other has.
        if (window.xnautProviderList) {
          window.xnautProviderList().then((provs) => {
            if (!provSel.isConnected) return;
            provSel.innerHTML = '<option value="">Default provider</option>'
              + (provs || []).map((x) => `<option value="${esc(x.key)}">${esc(window.xnautProviderLabel ? window.xnautProviderLabel(x.key) : x.key)}${x.configured ? '' : ' · not configured'}</option>`).join('');
          }).catch(() => {});
        }
        const fillModels = () => {
          const cat = window.xnautModelCatalog;
          const models = (provSel.value && cat && cat.forProvider(provSel.value)) || [];
          modelSel.innerHTML = '<option value="">Provider default</option>'
            + models.map((m) => {
              const id = typeof m === 'string' ? m : (m.id || m.name || '');
              return id ? `<option value="${esc(id)}">${esc(id)}</option>` : '';
            }).join('');
          modelSel.disabled = !provSel.value;
          hint.textContent = provSel.value
            ? 'Claude Code runs against this provider and model.'
            : 'Default routes through the NautGate wrapper.';
        };
        provSel.onchange = fillModels;
        fillModels();
        openBtn.onclick = async () => {
          openBtn.disabled = true;
          try { await openNewSession(project, provSel.value, modelSel.value); }
          finally { if (openBtn.isConnected) openBtn.disabled = false; }
          renderSessions();
        };
      }
    }

    // ---- running agents ----
    let lastRows = [];
    // Folded groups, by project key. Tracking what is CLOSED rather than what is
    // open means a group that appears between two 5s repaints is open, which is
    // the default the reader expects.
    const closedGroups = new Set();
    async function killRow(r) {
      try {
        if (r.kind === 'terminal') {
          // Kill means kill. This used to call agent_session_interrupt, which
          // flipped the row to `interrupted` and left the process running —
          // proven on the tron rig with two presses and a live pid afterwards
          // (XNAUT-261). Interrupt first so the agent can bail out cleanly,
          // then actually end it: the PTY (which kills its child), and the
          // zellij session when the row is an adopted one, whose "process" is
          // a session the app does not own.
          await invoke('agent_session_interrupt', { sessionId: r.id }).catch(() => {});
          if (r.sess) {
            await invoke('zellij_delete_session', { name: r.sess }).catch(() => {});
          }
          await invoke('close_terminal', { sessionId: r.id }).catch(() => {});
        } else {
          if (r.pid) await invoke('loom_run_stop', { pid: r.pid });
          // Build shell: the agent lives in a Zellij session, not a tracked pid —
          // delete-session actually kills it (close/detach would leave it running).
          if (r.sess) await invoke('create_command_session', { config: { program: 'sh', args: ['-c', 'export PATH="/opt/homebrew/bin:/usr/local/bin:$PATH"; zellij delete-session ' + r.sess + ' --force 2>/dev/null'], workingDir: r.cwd || '/tmp' } }).catch(() => {});
          if (!r.zellij) await invoke('loom_run_mark', { id: r.id, status: 'cancelled' }); // plain zellij rows have no run record — don't invent one
          // Unregister from the build manager too, or its watchdog revives the
          // agent seconds after the session dies (the kill that doesn't stick).
          try { window.xnautKillBuild && window.xnautKillBuild(r.wt || r.cwd || ''); } catch (_) {}
        }
      } catch (_) {}
    }
    async function loadRows() {
      // Both are cheap after their first pass (TTL + per-file cache) and both
      // must be in hand before a row can be told which project it belongs to.
      await Promise.all([loadProjects(), loadRegistry()]);
      const rows = [];
      try {
        const sessions = (await invoke('agent_sessions_list')) || [];
        sessions.forEach((s) => {
          if (s.status === 'done') return;
          // An ADOPTED row IS a zellij session (its session_id is the session
          // name), so it must dedup against the zellij list below or the same
          // agent is counted twice — the tron rig saw "6 active" for three
          // real processes (XNAUT-260).
          // The row's zellij session, wherever it came from: adopted rows are
          // named after it, dispatched rows now carry it explicitly. Keying on
          // the name alone missed dispatched agents, which were then counted a
          // second time as their own zellij row (XNAUT-260).
          const sess = s.zellij_session
            || (typeof s.session_id === 'string' && s.session_id.startsWith('xnaut-') ? s.session_id : undefined);
          const adopted = !!sess;
          rows.push({ kind: 'terminal', id: s.session_id, sess,
            title: (s.agent_id || 'agent') + ' · ' + (s.label || 'terminal'),
            sub: adopted ? 'adopted zellij session' : 'Interactive terminal session',
            model: s.agent_id || '—', cmd: adopted ? attachCmd(s.session_id) : runnerCmd(s.agent_id),
            started: s.started_at_ms, status: s.status || 'working' });
        });
      } catch (_) {}
      try {
        // Live Zellij sessions — the durable truth for build shells (a webview
        // reload wipes JS state, but the sessions and runs.jsonl survive).
        let zj = [], zjErr = ''; try { zj = (await invoke('zellij_live_sessions')) || []; } catch (e) { zjErr = String(e); }
        const runs = (await invoke('loom_runs_list', { limit: RUN_WINDOW })) || [];
        console.log('[obs] v2 zj:', zj.length, zjErr || 'ok', '· started-build recs:', runs.filter((r) => r.status === 'started' && r.provider === 'build').length);
        for (const r of runs) {
          if (r.status !== 'started') continue;
          if (r.provider === 'build') {
            // Build worktree shell: alive while its Zellij session exists. The
            // name must be derived exactly as zellij::session_name does in Rust,
            // or this lookup misses and the run self-heals itself to "done".
            const sess = ('cl-' + String(r.cwd || '').replace(/\/+$/, '').split('/').pop())
              .toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '')
              .slice(0, 24).replace(/-+$/, '');
            if (!zj.includes(sess)) { invoke('loom_run_mark', { id: r.id, status: 'done' }).catch(() => {}); continue; } // self-heal: session gone
            rows.push({ kind: 'local', id: r.id, wt: r.cwd, cwd: r.cwd, sess, title: r.weave,
              sub: 'zellij · ' + sess, model: r.model || '—', cmd: attachCmd(sess), started: r.started_ms, status: 'working' });
            continue;
          }
          let alive = false; if (r.pid) { try { alive = await invoke('loom_run_alive', { pid: r.pid }); } catch (_) {} }
          if (!alive) continue;
          rows.push({ kind: r.provider === 'local' ? 'local' : 'sandbox', id: r.id, pid: r.pid, cwd: r.cwd, title: r.weave + (r.goal ? ' · ' + r.goal.split('\n')[0].slice(0, 60) : ''),
            sub: r.cwd ? r.cwd.split('/').slice(-2).join('/') : 'run', model: r.model || '—', cmd: headlessCmd(r.model), started: r.started_ms, status: 'working' });
        }
      } catch (_) {}
      // Every LIVE zellij session (zellij ls) — click to attach in a new tab.
      // ELAPSED shows time since last activity (resurrection-cache mtime).
      try {
        // Now includes exited-but-resurrectable sessions; this list is about
        // what is live, so keep it to those.
        const zs = ((await invoke('zellij_sessions_info')) || []).filter((z) => !z.exited);
        const known = new Set(rows.map((r) => r.sess).filter(Boolean));
        for (const z of zs) {
          if (known.has(z.name)) continue;
          rows.push({ kind: 'zellij', id: 'zellij:' + z.name, sess: z.name, zellij: true, title: z.name,
            sub: 'zellij session' + (z.created ? ' · created ' + z.created.replace(/\s*ago\s*$/i, '') + ' ago' : '') + ' · click to attach',
            // ELAPSED counted from CREATION. last_active_ms made every zellij
            // row read 0:04 while its own subtitle said "created 1h56m ago"
            // (XNAUT-260): the row contradicted itself on screen.
            model: '—', cmd: 'zellij attach ' + z.name, started: z.created_ms || z.last_active_ms || Date.now(), status: 'open' });
        }
      } catch (_) {}
      rows.sort((a, b) => b.started - a.started);
      lastRows = rows;
      paintRows();
    }

    function rowHtml(r, i) {
      return `
        <div class="obs-row" data-i="${i}">
          <span class="c-type"><span class="obs-chip ${r.kind}">${r.kind.toUpperCase()}</span></span>
          <div class="c-name${(r.sid || r.wt || r.zellij) ? ' obs-clickable' : ''}"${(r.sid || r.wt || r.zellij) ? ` data-term="${i}" title="Open / re-attach this session in a terminal tab"` : ''}><span class="t">${esc(r.title)}</span><span class="s">${esc(r.sub)}${r.cmd ? ` · <button class="obs-open" data-open="${i}" title="Copy the command to open this session">${esc(r.cmd)}</button>` : ''}</span></div>
          <span class="c-model">${esc(r.model)}</span>
          <span class="c-res" data-res="${esc(r.cwd || '')}"${r.kind === 'sandbox' ? '' : ' title="No resource figures for this row. CPU and memory are sampled from the sandbox host, which only sandbox runs have. Token cost is not available either: this agent is a claude/codex CLI process that talks to the provider directly, so xNAUT never sees its token counts. Plan-level usage is in the cards above."'}>${r.kind === 'sandbox' ? '<span>CPU</span><span class="obs-bar"><i style="width:0%"></i></span><span class="pc">…</span>' : '<span style="opacity:.6">not sampled</span>'}</span>
          <span class="c-elapsed">${elapsed(r.started)}</span>
          <span class="c-status"><span class="dot ${esc(r.status)}"></span>${esc(r.status)}</span>
          <span class="c-kill"><button class="obs-kill" data-kill="${i}">■ Kill</button></span>
        </div>`;
    }

    function paintRows() {
      const rows = lastRows;
      const host = pane.querySelector('[data-rows]'); if (!host) return;
      const cnt = pane.querySelector('[data-count]'); if (cnt) cnt.textContent = rows.length + ' active';
      if (!rows.length) { host.innerHTML = '<div class="obs-empty">Nothing running. Terminal agents and sandbox runs appear here live.</div>'; return; }
      const groups = groupRows(rows, ctx);
      host.innerHTML = groups.map((g) => {
        const open = !closedGroups.has(g.key);
        const head = `
        <button class="obs-grp${open ? ' open' : ''}" data-grp="${esc(g.key)}" aria-expanded="${open ? 'true' : 'false'}">
          <span class="obs-caret">${open ? '▾' : '▸'}</span><b>${esc(g.label)}</b>
          <span class="obs-grp-n">${g.items.length} session${g.items.length === 1 ? '' : 's'}</span>
        </button>`;
        return open ? head + g.items.map(({ r, i }) => rowHtml(r, i)).join('') : head;
      }).join('');
      host.querySelectorAll('[data-grp]').forEach((b) => {
        b.onclick = () => {
          const key = b.dataset.grp;
          if (closedGroups.has(key)) closedGroups.delete(key); else closedGroups.add(key);
          paintRows();
        };
      });
      host.querySelectorAll('[data-kill]').forEach((b) => {
        b.onclick = async () => { b.disabled = true; await killRow(rows[+b.dataset.kill]); refresh(); };
      });
      host.querySelectorAll('[data-open]').forEach((b) => {
        b.onclick = (e) => { e.stopPropagation(); try { navigator.clipboard.writeText(rows[+b.dataset.open].cmd); const o = b.textContent; b.textContent = 'copied ✓'; setTimeout(() => { if (b.isConnected) b.textContent = o; }, 1000); } catch (_) {} };
      });
      // Click a live shell → open it in a real xNaut terminal tab.
      host.querySelectorAll('[data-term]').forEach((el) => {
        el.onclick = (e) => {
          if (e.target.closest('.obs-open')) return;
          const r = rows[+el.dataset.term]; if (!r) return;
          const label = String(r.title).split(' · ')[0];
          if (r.wt && window.xnautOpenBuildShell) window.xnautOpenBuildShell(r.wt, label); // re-attach the persistent session
          else if (r.zellij && window.xnautOpenZellijSession) window.xnautOpenZellijSession(r.sess); // zellij attach in a new tab
          else if (r.sid && window.xnautAttachAgentTab) window.xnautAttachAgentTab(r.sid, label);
        };
      });
      // sandbox CPU (best effort, per row with a cwd)
      rows.forEach(async (r, i) => {
        if (r.kind !== 'sandbox' || !r.cwd) return;
        try {
          const st = await invoke('loom_sandbox_stats', { cwd: r.cwd });
          const cell = host.querySelector(`.obs-row[data-i="${i}"] .c-res`); if (!cell) return;
          const pct = Math.round(st.cpu_pct);
          cell.innerHTML = `<span>CPU</span><span class="obs-bar"><i style="width:${Math.min(100, pct)}%;${pct >= 85 ? 'background:#e98b83' : ''}"></i></span><span class="pc">${pct}%</span>`;
        } catch (_) {}
      });
    }

    // ---- dispatched runs, read from the RUN REGISTRY (XNAUT-354) ----
    //
    // This band used to mirror `window.xnautSwarm`, a queue the Multi-Agent
    // Manager pane kept in the webview. That made the deck a view of one
    // pane's memory: a swarm started before a reload, or from anywhere but
    // that pane, was invisible here, and a reload emptied the band while the
    // agents kept working.
    //
    // A swarm is ordinary dispatched runs now, so the registry already knows
    // all of them, survives a reload, and groups them by project the way the
    // ticket asks. Only tickets: a run with no ticket is a session somebody
    // opened, and the table above is where those belong.
    // run_control::RunState, as the four colours a pill has. Anything that is
    // not plainly running still shows its own word next to the dot — `blocked`
    // and `degraded` are the states worth noticing, and rounding them off to
    // "running" is how a stuck swarm looks healthy.
    const bandFor = (state) => {
      const s = String(state || '').toLowerCase();
      if (s === 'done') return 'done';
      if (s === 'failed' || s === 'undead') return 'failed';
      if (s === 'retired' || s === 'retiring') return 'retired';
      return 'running';
    };

    async function renderSwarm() {
      const pills = pane.querySelector('[data-swarm-pills]');
      const counts = pane.querySelector('[data-swarm-counts]');
      const label = pane.querySelector('[data-swarm-label]');
      if (!pills) return;
      let rows = [];
      try { rows = (await invoke('run_registry_list', { limit: REGISTRY_ROWS })) || []; } catch (_) { rows = []; }
      const dispatched = rows.filter((r) => r && r.ticket && r.kind === 'agent');
      if (!dispatched.length) {
        pills.innerHTML = '<span class="obs-empty" style="padding:0">Nothing dispatched. Ask NautBot to work a project\'s open tickets.</span>';
        if (counts) counts.innerHTML = ''; if (label) label.textContent = '';
        return;
      }
      const n = (band) => dispatched.filter((r) => bandFor(r.state) === band).length;
      const projects = [...new Set(dispatched.map((r) => r.project).filter(Boolean))];
      if (label) label.textContent = (projects.join(' · ') || 'unattributed') + ' · ' + dispatched.length + ' runs';
      if (counts) counts.innerHTML = `<span class="run">${n('running')} running</span><span class="ok">${n('done')} done</span>${n('failed') ? `<span class="bad">${n('failed')} failed</span>` : ''}`;
      // Grouped by project, because "which project is this swarm on" is the
      // question the band exists to answer.
      pills.innerHTML = projects.map((key) => {
        const mine = dispatched.filter((r) => r.project === key);
        const items = mine.map((r) => {
          const band = bandFor(r.state);
          const cls = band === 'retired' ? '' : band;
          const extra = band === 'done' ? ' ✓' : band === 'failed' ? ' ✗'
            : String(r.state).toLowerCase() === 'running' && r.started_at ? ' ' + elapsed(r.started_at)
            : ' · ' + r.state;
          return `<span class="obs-pill ${cls}" title="${esc(r.branch || '')}"><span class="dot"></span>${esc(r.ticket)}${esc(extra)}</span>`;
        }).join('');
        return `<span class="obs-pill-group"><b>${esc(ctx.names.get(key) || key || 'unattributed')}</b>${items}</span>`;
      }).join('');
    }

    // ---- the ledger, signed (XNAUT-370) ----
    //
    // The deck showed what is running and what was dispatched, and neither
    // answered "why is nothing happening" — a sweep that refuses to dispatch
    // writes a `sweep_refused` line and nothing on this page read it. With the
    // signature on each line it answers a second question the panes could not:
    // whether the machine you are looking at is the one doing the work.
    const hhmm = (at) => {
      const t = Date.parse(at || '');
      return Number.isNaN(t) ? '' : new Date(t).toTimeString().slice(0, 5);
    };
    async function renderLedger() {
      const host = pane.querySelector('[data-ledger-list]');
      const count = pane.querySelector('[data-ledger-count]');
      if (!host) return;
      let rows = null, err = null;
      try { rows = (await invoke('ledger_recent', { limit: LEDGER_ROWS })) || []; }
      catch (e) { err = String(e); }
      // A broken read and an empty log are different facts and must not render
      // the same way — the lesson XNAUT-257 taught the cards above.
      if (err) {
        if (count) count.textContent = '';
        host.innerHTML = `<div class="obs-empty">Could not read the ledger: ${esc(err.slice(0, 120))}</div>`;
        return;
      }
      if (!rows.length) {
        if (count) count.textContent = '';
        host.innerHTML = '<div class="obs-empty">The ledger is empty. Dispatches, refusals and verifications land here as they happen.</div>';
        return;
      }
      // How many DISTINCT instances wrote these lines. One is a quiet fleet;
      // two is the thing that went unnoticed on 2026-09-13, and it should be
      // readable without counting chips.
      const instances = new Set(rows.map((r) => String(r.instance || '').trim()).filter(Boolean));
      if (count) {
        count.textContent = instances.size > 1
          ? `${rows.length} lines · ${instances.size} instances`
          : `${rows.length} lines`;
      }
      host.innerHTML = rows.map((r) => {
        const kind = String(r.kind || '');
        return `<div class="obs-led">
          <span class="kd${/refused|failed|gave_up/.test(kind) ? ' refused' : ''}">${esc(kind)}</span>
          <span class="tk">${esc(r.ticket || '')}</span>
          <span class="dt" title="${esc(r.detail || '')}">${esc(r.detail || '')}</span>
          ${sig(r)}
          <span class="wh">${esc(hhmm(r.at))}</span>
        </div>`;
      }).join('');
    }

    // Budget is an external rate-limited API — poll it gently (60s); the
    // agents table + swarm are local and stay on the fast 5s tick.
    async function refreshFast() { await loadRows(); await renderSwarm(); await renderLedger(); }
    // `refresh` was called by Stop-all and by every Kill button but never
    // existed, so both threw ReferenceError and the table never repainted
    // (XNAUT-257). It refetches usage too, because the refresh a person wants
    // after killing an agent includes the numbers, not just the row list.
    // The sessions band is deliberately NOT on the 5s tick: repainting it that
    // often would disarm a Kill button mid-question and reset the two selects
    // under the reader's hand. It repaints on Refresh, after a kill or an open,
    // and on the slow clock.
    async function refresh() { await Promise.all([refreshFast(), renderStrip(), renderSessions()]); }
    pane.querySelector('[data-refresh]').onclick = async (e) => {
      const b = e.currentTarget; b.disabled = true; b.textContent = '↻ Refreshing…';
      try { await refresh(); } finally { b.disabled = false; b.textContent = '↻ Refresh'; }
    };
    renderStrip(); refreshFast(); renderSessions();
    const timer = setInterval(() => { if (pane.isConnected) refreshFast(); else clearInterval(timer); }, 5000);
    const slow = setInterval(() => { if (pane.isConnected) { renderStrip(); renderSessions(); } else clearInterval(slow); }, 60000);
  }

  window.xnautCreateObservatoryPanel = createObservatoryPanel;
})();
