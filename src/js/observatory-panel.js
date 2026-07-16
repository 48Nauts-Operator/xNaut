// Observatory — the command deck (main panel tab, left menu above Tasks).
// Shows the MAX-plan budget up top, every running agent (terminal sessions via
// agent_sessions_list + sandbox loom runs via loom_runs_list/loom_run_alive)
// with elapsed/model/status and a per-row kill switch, and the multi-agent
// swarm queue (fed by multiagent-pane.js through window.xnautSwarm).
(function () {
  'use strict';

  const invoke = (...a) => window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke(...a);
  const esc = (s) => String(s == null ? '' : s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));

  function elapsed(ms) {
    const s = Math.max(0, Math.floor((Date.now() - ms) / 1000));
    const h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60), ss = s % 60;
    return (h ? h + ':' + String(m).padStart(2, '0') : String(m)) + ':' + String(ss).padStart(2, '0');
  }

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
.obs-table { background:var(--card,#171717); border:1px solid var(--border,#262626); border-radius:12px; overflow:hidden; display:flex; flex-direction:column; }
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
.obs-chip.terminal { color:var(--xnaut-yellow,#f5b840); border:1px solid #4a3d22; }
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
.obs-counts .cap { font-family:inherit; font-size:10px; font-weight:600; color:var(--muted-foreground); }`;
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
    async function renderStrip() {
      const host = pane.querySelector('[data-strip]'); if (!host) return;
      const [claude, codex] = await Promise.allSettled([
        invoke('max_usage', { account: null }), invoke('codex_usage'),
      ]);
      if (claude.status === 'fulfilled' && claude.value) lastC = claude.value;
      if (codex.status === 'fulfilled' && codex.value) lastX = codex.value;
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
          <div class="obs-bar"><i class="${cls || 'good'}" style="width:${left == null ? 0 : left}%"></i></div>
        </div>
        <div class="obs-card" style="width:200px;flex:0 0 auto">
          <span class="k">5-hour window</span>
          <div class="obs-big small"><b>${c ? Math.round(c.five_hour_pct) + '%' : '—'}</b><span>used${c && c.five_hour_resets_at ? ' · resets ' + esc(String(c.five_hour_resets_at).slice(11, 16)) : ''}</span></div>
          <div class="obs-bar"><i style="width:${c ? Math.min(100, Math.round(c.five_hour_pct)) : 0}%"></i></div>
        </div>
        <div class="obs-card" style="flex:1 1 auto;min-width:0">
          <span class="k">Per model · weekly</span>${models || '<span class="obs-empty" style="padding:0">no per-model data</span>'}
        </div>
        <div class="obs-card" style="width:200px;flex:0 0 auto">
          <span class="k">Codex${x && x.plan_type ? ' · ' + esc(x.plan_type) : ''}</span>
          <div class="obs-big small"><b>${x && x.secondary ? Math.round(x.secondary.used_percent) + '%' : '—'}</b><span>${x && x.secondary ? esc(x.secondary.window_label || 'weekly') + ' used' : 'no data'}</span></div>
          <div class="obs-bar"><i style="width:${x && x.secondary ? Math.min(100, Math.round(x.secondary.used_percent)) : 0}%"></i></div>
        </div>`;
    }

    // ---- running agents ----
    let lastRows = [];
    async function killRow(r) {
      try {
        if (r.kind === 'terminal') await invoke('agent_session_interrupt', { sessionId: r.id });
        else {
          if (r.pid) await invoke('loom_run_stop', { pid: r.pid });
          await invoke('loom_run_mark', { id: r.id, status: 'cancelled' });
        }
      } catch (_) {}
    }
    async function loadRows() {
      const rows = [];
      try {
        const sessions = (await invoke('agent_sessions_list')) || [];
        sessions.forEach((s) => {
          if (s.status === 'done') return;
          rows.push({ kind: 'terminal', id: s.session_id, title: (s.agent_id || 'agent') + ' · ' + (s.label || 'terminal'),
            sub: 'Interactive terminal session', model: s.agent_id || '—', started: s.started_at_ms, status: s.status || 'working' });
        });
      } catch (_) {}
      try {
        const runs = (await invoke('loom_runs_list', { limit: 30 })) || [];
        for (const r of runs) {
          if (r.status !== 'started') continue;
          let alive = false; if (r.pid) { try { alive = await invoke('loom_run_alive', { pid: r.pid }); } catch (_) {} }
          if (!alive) continue;
          rows.push({ kind: 'sandbox', id: r.id, pid: r.pid, cwd: r.cwd, title: r.weave + (r.goal ? ' · ' + r.goal.split('\n')[0].slice(0, 60) : ''),
            sub: r.cwd ? r.cwd.split('/').slice(-2).join('/') : 'sandbox run', model: r.model || '—', started: r.started_ms, status: 'working' });
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
          <div class="c-name"><span class="t">${esc(r.title)}</span><span class="s">${esc(r.sub)}</span></div>
          <span class="c-model">${esc(r.model)}</span>
          <span class="c-res" data-res="${esc(r.cwd || '')}">${r.kind === 'sandbox' ? '<span>CPU</span><span class="obs-bar"><i style="width:0%"></i></span><span class="pc">…</span>' : '—'}</span>
          <span class="c-elapsed">${elapsed(r.started)}</span>
          <span class="c-status"><span class="dot ${esc(r.status)}"></span>${esc(r.status)}</span>
          <span class="c-kill"><button class="obs-kill" data-kill="${i}">■ Kill</button></span>
        </div>`).join('');
      host.querySelectorAll('[data-kill]').forEach((b) => {
        b.onclick = async () => { b.disabled = true; await killRow(rows[+b.dataset.kill]); refresh(); };
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
    renderStrip(); refreshFast();
    const timer = setInterval(() => { if (pane.isConnected) refreshFast(); else clearInterval(timer); }, 5000);
    const slow = setInterval(() => { if (pane.isConnected) renderStrip(); else clearInterval(slow); }, 60000);
  }

  window.xnautCreateObservatoryPanel = createObservatoryPanel;
})();
