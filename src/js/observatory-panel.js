// Observatory — the command deck (main panel tab, left menu above Tasks).
// Shows the MAX-plan budget up top and every running agent with elapsed/model/
// status and a per-row kill switch: terminal sessions (agent_sessions_list),
// persona/sandbox loom runs (loom_runs_list + loom_run_alive), and build
// worktree shells (loom_runs_list provider "build" + zellij_sessions —
// durable, so they survive a webview reload and can be re-attached).
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
.obs-pill { display:inline-flex; align-items:center; gap:7px; border:1px solid var(--border,#262626); border-radius:999px; padding:5px 12px; font-family:ui-monospace,Menlo,monospace; font-size:10.5px; color:var(--muted-foreground); }
.obs-pill .dot { width:7px; height:7px; border-radius:50%; background:#6b6f78; }
.obs-pill.running { border-color:#4a3d22; background:#1c1910; color:var(--foreground); }
.obs-pill.running .dot { background:var(--xnaut-yellow,#f5b840); }
.obs-pill.done { border-color:#24402c; color:#7ec98f; } .obs-pill.done .dot { background:#7ec98f; }
.obs-pill.failed { border-color:#3a2a2a; color:#e98b83; } .obs-pill.failed .dot { background:#e98b83; }
.obs-counts { margin-left:auto; display:flex; align-items:center; gap:14px; font-family:ui-monospace,Menlo,monospace; font-size:10.5px; }
.obs-counts .run { color:var(--xnaut-yellow,#f5b840); } .obs-counts .q { color:var(--muted-foreground); } .obs-counts .ok { color:#7ec98f; } .obs-counts .bad { color:#e98b83; }
.obs-counts .cap { font-family:inherit; font-size:10px; font-weight:600; color:var(--muted-foreground); }
.obs-why { padding:0 !important; font-size:10px; line-height:1.3; opacity:.75; display:block; overflow:hidden; text-overflow:ellipsis; }`;
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
          <button class="obs-btn primary" data-multiagent>✦ Initialize Multi-Agent</button>
        </div>
      </div>
      <div class="obs-strip" data-strip></div>
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
        <div class="obs-thead"><span class="k">Multi-Agent swarm</span><span class="n" data-swarm-label></span>
          <span class="obs-counts" data-swarm-counts></span></div>
        <div class="obs-pills" data-swarm-pills><span class="obs-empty" style="padding:0">No swarm running — Initialize Multi-Agent to start one.</span></div>
      </div>`;
    parentContainer.appendChild(pane);

    pane.querySelector('[data-multiagent]').onclick = () => {
      if (window.xnautShowRightPane) window.xnautShowRightPane();
      if (window.xnautRightPaneShow) window.xnautRightPaneShow('multiagent');
    };
    pane.querySelector('[data-stopall]').onclick = async () => {
      if (window.xnautSwarm && window.xnautSwarm.stopAll) await window.xnautSwarm.stopAll();
      for (const r of lastRows) { await killRow(r); }
      refresh();
    };

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
      const [claude, codex] = await Promise.allSettled([
        invoke('max_usage', { account: null }), invoke('codex_usage'),
      ]);
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

    // ---- running agents ----
    let lastRows = [];
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
        const runs = (await invoke('loom_runs_list', { limit: 30 })) || [];
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
      const host = pane.querySelector('[data-rows]'); if (!host) return;
      const cnt = pane.querySelector('[data-count]'); if (cnt) cnt.textContent = rows.length + ' active';
      if (!rows.length) { host.innerHTML = '<div class="obs-empty">Nothing running. Terminal agents and sandbox runs appear here live.</div>'; return; }
      host.innerHTML = rows.map((r, i) => `
        <div class="obs-row" data-i="${i}">
          <span class="c-type"><span class="obs-chip ${r.kind}">${r.kind.toUpperCase()}</span></span>
          <div class="c-name${(r.sid || r.wt || r.zellij) ? ' obs-clickable' : ''}"${(r.sid || r.wt || r.zellij) ? ` data-term="${i}" title="Open / re-attach this session in a terminal tab"` : ''}><span class="t">${esc(r.title)}</span><span class="s">${esc(r.sub)}${r.cmd ? ` · <button class="obs-open" data-open="${i}" title="Copy the command to open this session">${esc(r.cmd)}</button>` : ''}</span></div>
          <span class="c-model">${esc(r.model)}</span>
          <span class="c-res" data-res="${esc(r.cwd || '')}"${r.kind === 'sandbox' ? '' : ' title="No resource figures for this row. CPU and memory are sampled from the sandbox host, which only sandbox runs have. Token cost is not available either: this agent is a claude/codex CLI process that talks to the provider directly, so xNAUT never sees its token counts. Plan-level usage is in the cards above."'}>${r.kind === 'sandbox' ? '<span>CPU</span><span class="obs-bar"><i style="width:0%"></i></span><span class="pc">…</span>' : '<span style="opacity:.6">not sampled</span>'}</span>
          <span class="c-elapsed">${elapsed(r.started)}</span>
          <span class="c-status"><span class="dot ${esc(r.status)}"></span>${esc(r.status)}</span>
          <span class="c-kill"><button class="obs-kill" data-kill="${i}">■ Kill</button></span>
        </div>`).join('');
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

    // ---- swarm section (state published by multiagent-pane.js) ----
    function renderSwarm() {
      const sw = window.xnautSwarm;
      const pills = pane.querySelector('[data-swarm-pills]');
      const counts = pane.querySelector('[data-swarm-counts]');
      const label = pane.querySelector('[data-swarm-label]');
      if (!pills) return;
      const q = (sw && sw.queue) || [];
      if (!q.length) {
        pills.innerHTML = '<span class="obs-empty" style="padding:0">No swarm running — Initialize Multi-Agent to start one.</span>';
        if (counts) counts.innerHTML = ''; if (label) label.textContent = '';
        return;
      }
      const n = (st) => q.filter((x) => x.status === st).length;
      if (label) label.textContent = (sw.project || '') + ' · ' + q.length + ' tickets';
      if (counts) counts.innerHTML = `<span class="run">${n('running')} running</span><span class="q">${n('queued')} queued</span><span class="ok">${n('done')} done</span>${n('failed') ? `<span class="bad">${n('failed')} failed</span>` : ''}<span class="cap">max parallel · ${sw.maxParallel || '?'}</span>`;
      pills.innerHTML = q.map((t) => {
        const cls = t.status === 'running' ? 'running' : t.status === 'done' ? 'done' : t.status === 'failed' ? 'failed' : '';
        const extra = t.status === 'running' && t.started ? ' ' + elapsed(t.started)
          : t.status === 'done' && t.pr ? ' ✓ PR' : t.status === 'queued' ? ' · queued' : t.status === 'failed' ? ' ✗' : '';
        return `<span class="obs-pill ${cls}"><span class="dot"></span>${esc(t.id)}${esc(extra)}</span>`;
      }).join('');
    }
    window.addEventListener('xnaut-swarm-update', renderSwarm);

    // Budget is an external rate-limited API — poll it gently (60s); the
    // agents table + swarm are local and stay on the fast 5s tick.
    async function refreshFast() { await loadRows(); renderSwarm(); }
    // `refresh` was called by Stop-all and by every Kill button but never
    // existed, so both threw ReferenceError and the table never repainted
    // (XNAUT-257). It refetches usage too, because the refresh a person wants
    // after killing an agent includes the numbers, not just the row list.
    async function refresh() { await Promise.all([refreshFast(), renderStrip()]); }
    pane.querySelector('[data-refresh]').onclick = async (e) => {
      const b = e.currentTarget; b.disabled = true; b.textContent = '↻ Refreshing…';
      try { await refresh(); } finally { b.disabled = false; b.textContent = '↻ Refresh'; }
    };
    renderStrip(); refreshFast();
    const timer = setInterval(() => { if (pane.isConnected) refreshFast(); else clearInterval(timer); }, 5000);
    const slow = setInterval(() => { if (pane.isConnected) renderStrip(); else clearInterval(slow); }, 60000);
  }

  window.xnautCreateObservatoryPanel = createObservatoryPanel;
})();
