// Multi-Agent Manager — right-pane view 'multiagent' (XNAUT Observatory).
// A chat that plans SWARMS: "work on all open tickets for Project A" →
// validates the tickets against the PM (real data only, no invention), shows a
// confirm card, then dispatches up to N parallel NautLoom sandbox runs — one
// git worktree per run (GitVM allows one sandbox per directory). Each green
// run ships its branch + PR and moves the ticket to review.
// Publishes queue state as window.xnautSwarm (+ 'xnaut-swarm-update' events)
// so the Observatory page can render it.
(function () {
  'use strict';

  function register(key, view) {
    if (typeof window.xnautRightPaneRegisterView === 'function') window.xnautRightPaneRegisterView(key, view);
    else (window.__xnautRightPaneQueue = window.__xnautRightPaneQueue || []).push({ key, view });
  }
  const invoke = (...a) => window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke(...a);
  const esc = (s) => String(s == null ? '' : s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
  const notify = (t, b) => { if (window.xnautNotify) window.xnautNotify(t, b); };

  let styled = false;
  function injectStyles() {
    if (styled) return; styled = true;
    const st = document.createElement('style');
    st.textContent = `
.mag { height:100%; display:flex; flex-direction:column; background:var(--background,#0f1115); }
.mag-head { display:flex; align-items:center; gap:9px; padding:13px 14px; border-bottom:1px solid var(--border,#262626); flex:0 0 auto; }
.mag-glyph { width:24px; height:24px; border-radius:7px; background:#16302e; color:#5bd1c9; display:flex; align-items:center; justify-content:center; flex-shrink:0; }
.mag-head .t { display:flex; flex-direction:column; gap:1px; min-width:0; }
.mag-head .t b { font-size:13px; font-weight:700; color:var(--foreground,#fafafa); }
.mag-head .t span { font-size:10px; color:var(--muted-foreground,#a1a1a1); }
.mag-ctl { margin-left:auto; display:flex; align-items:center; gap:6px; }
.mag-ctl label { font-size:10px; color:var(--muted-foreground); }
.mag-par { width:44px; background:var(--secondary,#262626); border:1px solid var(--border,#262626); border-radius:7px; color:var(--foreground); font-family:ui-monospace,Menlo,monospace; font-weight:700; font-size:12px; padding:4px 6px; text-align:center; outline:none; }
.mag-model { background:var(--secondary,#262626); border:1px solid var(--border,#262626); border-radius:7px; color:var(--muted-foreground); font:inherit; font-size:10px; padding:4px 6px; outline:none; }
.mag-thread { flex:1 1 auto; min-height:0; overflow-y:auto; display:flex; flex-direction:column; gap:12px; padding:14px; }
.mag-u { display:flex; justify-content:flex-end; }
.mag-u > div { max-width:82%; background:var(--secondary,#262626); border-radius:11px; border-top-right-radius:3px; padding:9px 11px; font-size:12px; line-height:1.45; color:var(--foreground); white-space:pre-wrap; }
.mag-a { display:flex; gap:9px; }
.mag-a .av { width:24px; height:24px; flex-shrink:0; border-radius:7px; background:#16302e; color:#5bd1c9; display:flex; align-items:center; justify-content:center; font-size:11px; }
.mag-a .bub { background:var(--card,#171717); border:1px solid var(--border,#262626); border-radius:11px; border-top-left-radius:3px; padding:9px 11px; font-size:12px; line-height:1.45; color:var(--foreground); min-width:0; flex:0 1 auto; }
.mag-a:has(.mag-card) .bub { flex:1 1 auto; }
.mag-card { display:flex; flex-direction:column; gap:8px; }
.mag-card .h { font-weight:700; font-size:12.5px; color:var(--foreground); }
.mag-card .meta { font-family:ui-monospace,Menlo,monospace; font-size:10px; color:var(--muted-foreground); }
.mag-tk { display:flex; align-items:center; gap:8px; }
.mag-tk .n { width:14px; flex-shrink:0; font-family:ui-monospace,Menlo,monospace; font-size:10px; color:#6b6f78; }
.mag-tk .id { font-family:ui-monospace,Menlo,monospace; font-size:11px; color:var(--foreground); }
.mag-tk .ti { font-size:10.5px; color:var(--muted-foreground); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
.mag-checks { display:flex; flex-direction:column; gap:3px; border-top:1px solid #1e2026; padding-top:8px; font-size:11px; }
.mag-checks .ok { color:#7ec98f; } .mag-checks .warn { color:var(--xnaut-yellow,#f5b840); } .mag-checks .dim { color:var(--muted-foreground); } .mag-checks .bad { color:#e98b83; }
.mag-launch { display:flex; align-items:center; justify-content:center; gap:7px; height:36px; border:0; border-radius:9px; background:var(--xnaut-yellow,#f5b840); color:#171717; font:inherit; font-weight:700; font-size:12.5px; cursor:pointer; }
.mag-launch[disabled] { opacity:.5; cursor:default; }
.mag-composer { display:flex; align-items:flex-end; gap:8px; padding:10px 12px 12px; border-top:1px solid var(--border,#262626); flex:0 0 auto; }
.mag-input { flex:1 1 auto; min-width:0; min-height:44px; max-height:160px; resize:vertical; background:var(--secondary,#262626); border:1px solid var(--border,#262626); border-radius:11px; color:var(--foreground); font:inherit; font-size:12px; line-height:1.4; padding:11px 12px; outline:none; }
.mag-input:focus { border-color:var(--xnaut-yellow,#f5b840); }
.mag-send { height:44px; padding:0 16px; border:0; border-radius:11px; background:var(--xnaut-yellow,#f5b840); color:#171717; font:inherit; font-weight:700; font-size:12.5px; cursor:pointer; flex:0 0 auto; }`;
    document.head.appendChild(st);
  }

  // ---- swarm state (published for the Observatory) ---------------------------
  const swarm = {
    queue: [],        // [{id, title, project, root, status: queued|running|done|failed|cancelled, started, runId, pid, wt, pr}]
    project: '',
    maxParallel: 3,
    model: localStorage.getItem('xnaut-loom-model') || 'claude-fable-5',
    loomName: '',
    active: false,
    async stopAll() {
      for (const t of swarm.queue) {
        if (t.status === 'running' && t.pid) {
          try { await invoke('loom_run_stop', { pid: t.pid }); } catch (_) {}
          try { await invoke('loom_run_mark', { id: t.runId, status: 'cancelled' }); } catch (_) {}
          t.status = 'cancelled';
        } else if (t.status === 'queued') t.status = 'cancelled';
      }
      swarm.active = false;
      publish();
    },
  };
  window.xnautSwarm = swarm;
  function publish() { try { window.dispatchEvent(new CustomEvent('xnaut-swarm-update')); } catch (_) {} }

  // ---- orchestrator -----------------------------------------------------------
  async function resolveRoot(projectKey) {
    try { return (await window.xnautLoom.resolveProjectRoot(projectKey)) || ''; } catch (_) { return ''; }
  }

  async function closeTicket(id, ok, prUrl) {
    try {
      const tickets = (await invoke('pm_ticket_list', { project: null })) || [];
      const t = tickets.find((x) => x.id === id); if (!t) return;
      const stamp = new Date().toISOString().slice(0, 16).replace('T', ' ');
      const note = '\n\n> NautLoom swarm run ' + stamp + ' — ' + (ok ? '✓ acceptance green' : '✗ failed') + '.'
        + (prUrl ? ' PR: ' + prUrl : ok ? ' Shipped on branch nautloom/' + id.toLowerCase() + '.' : '');
      const req = { id: t.id, expected_revision: t.revision, body: (t.body || '') + note };
      if (ok && ['inbox', 'ready', 'in_progress'].indexOf(t.status) >= 0) req.status = 'review';
      await invoke('pm_ticket_update', { request: req });
    } catch (_) {}
  }

  async function runTicket(t, chosenLoom) {
    const L = window.xnautLoom;
    t.status = 'running'; t.started = Date.now(); publish();
    try {
      // 1. worktree — own directory → own sandbox
      const branch = 'nautloom/' + t.id.toLowerCase();
      const wt = await invoke('worktree_suggest_path', { repoPath: t.root, branch: branch });
      try {
        await invoke('worktree_add', { repoPath: t.root, worktreePath: wt, opts: { branch: branch, base: null, checkout_existing: false } });
      } catch (_) {
        // branch exists from an earlier run → check it out instead
        await invoke('worktree_add', { repoPath: t.root, worktreePath: wt, opts: { branch: branch, base: null, checkout_existing: true } });
      }
      t.wt = wt;
      // 2. compose the exact same run the single-run path would
      const ticket = t.full;
      const goal = L.enrichGoal(chosenLoom, L.ticketToGoal(ticket));
      const v = L.verifyWeave(chosenLoom);
      if (!v.ok) throw new Error('loom invalid: ' + v.issues.join('; '));
      const script = L.composeCommands(chosenLoom, v.provider).map((c) => 'echo "» ' + c.action + '"; ' + c.cmd).join('\n');
      const runId = 'run-' + Date.now() + '-' + t.id.toLowerCase();
      const h = await invoke('loom_run', { runId: runId, script: script, goal: goal, cwd: wt, model: swarm.model });
      t.runId = runId; t.pid = h.pid; publish();
      try { await invoke('loom_run_record', { runId: runId, weave: chosenLoom.metadata.name, goal: t.id + ': ' + t.title, provider: v.provider, pid: h.pid, model: swarm.model, cwd: wt }); } catch (_) {}
      // 3. wait for __LOOM_DONE__ (poll the log; bail if the driver dies)
      const code = await new Promise((resolve) => {
        let stale = 0;
        const step = async () => {
          if (t.status === 'cancelled') return resolve('cancelled');
          let txt = ''; try { txt = (await invoke('read_file', { path: h.log })) || ''; } catch (_) {}
          const m = txt.match(/__LOOM_DONE__ (\d+)/);
          if (m) return resolve(m[1]);
          if (++stale >= 20) {
            stale = 0;
            let alive = true; try { alive = await invoke('loom_run_alive', { pid: h.pid }); } catch (_) {}
            if (!alive) return resolve('dead');
          }
          setTimeout(step, 3000);
        };
        step();
      });
      if (code === 'cancelled') return;
      const ok = code === '0';
      try { await invoke('loom_run_mark', { id: runId, status: ok ? 'done' : 'failed' }); } catch (_) {}
      // 4. ship on green: commit in the worktree (branch already checked out) → push → PR
      let prUrl = '';
      if (ok) {
        try {
          const ship = await invoke('loom_ship', { cwd: wt, branch: branch, message: t.id + ': ' + t.title + '\n\nProduced by a NautLoom multi-agent swarm run — acceptance green.\nRefs ' + t.id + '\n\nCo-Authored-By: NautLoom Cloud Agent <noreply@48nauts.com>' });
          try {
            const pr = await invoke('forge_create_pr', { forgeIndex: 0, repo: ship.org_repo, head: ship.branch, base: 'main', title: t.id + ': ' + t.title, body: 'Automated NautLoom swarm run — acceptance green. Report + demo video in the run artifacts.\n\nRefs ' + t.id });
            const u = String(pr || '').match(/https?:\/\/\S+/); prUrl = u ? u[0] : '';
            t.pr = prUrl || true;
          } catch (_) { t.pr = ''; }
        } catch (_) {}
      }
      await closeTicket(t.id, ok, prUrl);
      t.status = ok ? 'done' : 'failed'; publish();
      notify('Swarm · ' + t.id + (ok ? ' ✓ done' : ' ✗ failed'), ok ? (prUrl ? 'PR created: ' + prUrl : 'Shipped on ' + branch) : 'Run did not finish green — worktree kept for debugging.');
      // 5. cleanup on success (keep failed worktrees for inspection)
      // force: loom leftovers (.loom-*, artifacts/) make the worktree dirty;
      // keep the local branch — the pushed remote branch backs the PR.
      if (ok) { try { await invoke('worktree_remove', { repoPath: t.root, worktreePath: wt, opts: { force: true, delete_branch: false } }); } catch (_) {} }
    } catch (e) {
      t.status = 'failed'; t.err = String((e && e.message) || e); publish();
      notify('Swarm · ' + t.id + ' ✗ error', t.err.slice(0, 140));
    }
  }

  async function launchSwarm(tickets, chosenLoom, addAgent) {
    swarm.queue = tickets.map((t) => ({ id: t.id, title: t.title, project: t.project, root: t._root, full: t, status: 'queued' }));
    swarm.project = tickets[0] ? tickets[0].project : '';
    swarm.loomName = chosenLoom.metadata.name;
    swarm.active = true;
    publish();
    let done = 0;
    const next = async () => {
      if (!swarm.active) return;
      const t = swarm.queue.find((x) => x.status === 'queued');
      if (!t) return;
      await runTicket(t, chosenLoom);
      done++;
      if (!swarm.queue.some((x) => x.status === 'queued' || x.status === 'running')) {
        const ok = swarm.queue.filter((x) => x.status === 'done').length;
        const prs = swarm.queue.filter((x) => x.pr).length;
        swarm.active = false; publish();
        notify('Swarm complete', ok + '/' + swarm.queue.length + ' green — ' + prs + ' PR' + (prs === 1 ? '' : 's') + '.');
        addAgent('Swarm complete: ' + ok + '/' + swarm.queue.length + ' green, ' + prs + ' PR(s) opened. Details in the Observatory.');
        return;
      }
      next(); // this slot takes the next queued ticket
    };
    const slots = Math.max(1, Math.min(20, swarm.maxParallel));
    for (let i = 0; i < slots; i++) next();
  }

  // ---- the manager chat view ---------------------------------------------------
  function createMultiagentView() {
    let container = null, root = null;
    let messages = [];
    let proposal = null; // {tickets:[full], loom, skipped:[], weekPct}

    function els() {
      return container && {
        thread: container.querySelector('[data-mag-thread]'),
        input: container.querySelector('[data-mag-input]'),
        par: container.querySelector('[data-mag-par]'),
        model: container.querySelector('[data-mag-model]'),
      };
    }

    function cardHtml(p) {
      const rows = p.tickets.map((t, i) => `<div class="mag-tk"><span class="n">${i + 1}</span><span class="id">${esc(t.id)}</span><span class="ti">${esc(t.title)}</span></div>`).join('');
      const budget = p.weekPct == null ? '<span class="dim">○ budget unknown</span>'
        : p.weekPct >= 80 ? `<span class="bad">✗ budget guard — ${Math.round(p.weekPct)}% of the week used. Consider a smaller swarm.</span>`
        : `<span class="ok">✓ budget guard — ${Math.round(p.weekPct)}% of week used, ok</span>`;
      const skipped = p.skipped.map((s) => `<span class="dim">○ ${esc(s)}</span>`).join('');
      return `<div class="mag-card">
        <span class="h">Swarm plan · ${esc(p.tickets[0] ? p.tickets[0].project : '')}</span>
        <span class="meta">${p.tickets.length} runs · loom ${esc(p.loom.metadata.name)} · ${esc(modelLabel())} · max ${swarm.maxParallel} parallel</span>
        ${rows}
        <div class="mag-checks">
          <span class="ok">✓ each run: own worktree · own sandbox · demo video · PR</span>
          ${budget}
          ${skipped}
        </div>
        <button class="mag-launch" data-launch>▸ Launch swarm — ${p.tickets.length} agent${p.tickets.length === 1 ? '' : 's'}</button>
      </div>`;
    }
    function modelLabel() {
      const M = (window.xnautLoom && window.xnautLoom.MODELS) || [];
      const m = M.find((x) => x[0] === swarm.model);
      return m ? m[1] : swarm.model;
    }

    function render() {
      const e = els(); if (!e || !e.thread) return;
      const msgs = messages.length ? messages : [{ role: 'agent', text: 'Tell me which project (or tickets) to work on — e.g. "work on all open tickets for ChessTrainer". I\'ll validate them against the PM, show you the swarm plan, and dispatch on your confirm.' }];
      e.thread.innerHTML = msgs.map((m) => {
        if (m.role === 'user') return `<div class="mag-u"><div>${esc(m.text)}</div></div>`;
        const body = m.kind === 'card' ? cardHtml(m.proposal) : esc(m.text);
        return `<div class="mag-a"><div class="av">✳</div><div class="bub">${body}</div></div>`;
      }).join('');
      e.thread.scrollTop = e.thread.scrollHeight;
      const lb = e.thread.querySelector('[data-launch]');
      if (lb) lb.onclick = () => {
        lb.disabled = true;
        const p = proposal; if (!p) return;
        messages.push({ role: 'agent', text: 'Dispatching ' + p.tickets.length + ' agents (max ' + swarm.maxParallel + ' parallel). Watch them in the Observatory.' });
        render();
        launchSwarm(p.tickets, p.loom, (txt) => { messages.push({ role: 'agent', text: txt }); render(); });
      };
    }

    // The manager LLM: real PM projects + tickets injected; strict JSON out.
    async function managerLlm() {
      const projects = (await invoke('pm_project_list').catch(() => [])) || [];
      const tickets = (await invoke('pm_ticket_list', { project: null }).catch(() => [])) || [];
      const open = tickets.filter((t) => ['inbox', 'ready', 'in_progress'].indexOf(t.status) >= 0);
      const looms = await window.xnautLoom.listLooms();
      const projCtx = projects.map((p) => `- ${p.key} ("${p.name}")${p.source_path || p.source_repo ? '' : ' [no local path — cannot run]'}`).join('\n');
      const tkCtx = open.map((t) => `- ${t.id} [${t.project}] "${t.title}" · ${t.status} · ${(t.body || '').slice(0, 120).replace(/\n/g, ' ')}`).join('\n');
      const loomCtx = looms.filter((l) => l.name !== 'blank').map((l) => `- "${l.name}": ${l.description || ''}`).join('\n');
      const sys = 'You are the Multi-Agent Manager in xNAUT. You plan SWARMS: batches of autonomous Cloud Agent runs, one sandbox per ticket.\n\n'
        + 'PROJECTS:\n' + (projCtx || '(none)') + '\n\nOPEN TICKETS (the only truth — never invent tickets):\n' + (tkCtx || '(none)') + '\n\nLOOMS:\n' + (loomCtx || '(none)') + '\n\n'
        + 'Rules:\n'
        + '- Questions get short answers ({"kind":"chat"}).\n'
        + '- A swarm request: pick the matching OPEN tickets from the list above (never invent, never include done tickets), pick the best loom, and propose. Skip tickets with empty/unusable descriptions and say why.\n'
        + 'Respond STRICT JSON only:\n'
        + '{"kind":"chat","reply":"..."}\n'
        + '{"kind":"swarm","tickets":["ID1","ID2"],"loom":"<name>","reply":"<one-line summary>","skipped":["ID3 — reason"]}';
      const msgs = [{ role: 'system', content: sys }];
      messages.forEach((m) => {
        if (m.kind === 'card') msgs.push({ role: 'assistant', content: '[showed swarm plan: ' + m.proposal.tickets.map((t) => t.id).join(', ') + ']' });
        else msgs.push({ role: m.role === 'user' ? 'user' : 'assistant', content: m.text });
      });
      const raw = await invoke('chat_send', { requestId: 'multiagent-' + Date.now(), messages: msgs });
      const jm = String(raw).match(/\{[\s\S]*\}/);
      return { decision: jm ? JSON.parse(jm[0]) : { kind: 'chat', reply: String(raw) }, open, looms };
    }

    async function dispatch() {
      const e = els(); if (!e) return;
      const text = (e.input.value || '').trim(); if (!text) { e.input.focus(); return; }
      messages.push({ role: 'user', text }); e.input.value = '';
      messages.push({ role: 'agent', text: '…', kind: 'busy' }); render();
      let out = null, err = '';
      try { out = await managerLlm(); } catch (ex) { err = String((ex && ex.message) || ex); }
      messages = messages.filter((m) => m.kind !== 'busy');
      if (!out) { messages.push({ role: 'agent', text: 'Manager LLM unreachable (' + err + ').' }); render(); return; }
      const d = out.decision;
      if (d.kind === 'swarm' && Array.isArray(d.tickets) && d.tickets.length) {
        // validate against the real PM list + resolve each project's local root
        const valid = [], skipped = (d.skipped || []).slice();
        for (const id of d.tickets) {
          const t = out.open.find((x) => x.id === id);
          if (!t) { skipped.push(id + ' — not an open ticket'); continue; }
          const rootPath = await resolveRoot(t.project);
          if (!rootPath) { skipped.push(id + ' — project ' + t.project + ' has no local path'); continue; }
          t._root = rootPath; valid.push(t);
        }
        const loom = out.looms.find((l) => l.name === d.loom && l.name !== 'blank') || out.looms.find((l) => l.name === 'build-verify') || out.looms.find((l) => l.name !== 'blank');
        if (!valid.length || !loom) {
          messages.push({ role: 'agent', text: 'Nothing runnable: ' + (skipped.join('; ') || 'no valid tickets/loom.') }); render(); return;
        }
        let full = null;
        try { full = await invoke('loom_read', { path: loom.path }); } catch (_) {}
        if (!full) { messages.push({ role: 'agent', text: 'Could not read loom ' + loom.name + '.' }); render(); return; }
        let weekPct = null;
        try { const u = await invoke('max_usage', { account: null }); weekPct = u.seven_day_pct; } catch (_) {}
        if (d.reply) messages.push({ role: 'agent', text: d.reply });
        proposal = { tickets: valid, loom: full, skipped, weekPct };
        messages.push({ role: 'agent', kind: 'card', proposal });
      } else {
        messages.push({ role: 'agent', text: d.reply || '(no reply)' });
      }
      render();
    }

    function mount(el, initialRoot) {
      container = el; root = initialRoot;
      injectStyles();
      container.innerHTML = `<div class="mag">
        <div class="mag-head">
          <div class="mag-glyph"><svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.3"><circle cx="5" cy="5" r="2"/><circle cx="11" cy="5" r="2"/><circle cx="8" cy="11.5" r="2"/><path d="M6.2 6.6L7.4 9.6M9.8 6.6L8.6 9.6"/></svg></div>
          <div class="t"><b>Multi-Agent Manager</b><span>plans swarms · dispatches Cloud Agents</span></div>
          <div class="mag-ctl">
            <select class="mag-model" data-mag-model title="Executor model"></select>
            <label>max parallel</label>
            <input class="mag-par" data-mag-par type="number" min="1" max="20" value="${swarm.maxParallel}">
          </div>
        </div>
        <div class="mag-thread" data-mag-thread></div>
        <div class="mag-composer">
          <textarea class="mag-input" data-mag-input placeholder="Work on all open tickets for Project…"></textarea>
          <button class="mag-send" data-mag-send>Send</button>
        </div>
      </div>`;
      const e = els();
      // model select
      const M = (window.xnautLoom && window.xnautLoom.MODELS) || [];
      e.model.innerHTML = M.map(([v, l]) => `<option value="${v}"${v === swarm.model ? ' selected' : ''}>${l}</option>`).join('');
      e.model.onchange = () => { swarm.model = e.model.value; try { localStorage.setItem('xnaut-loom-model', swarm.model); } catch (_) {} };
      // max parallel — persisted in settings.loops.max_parallel_runs
      invoke('settings_get').then((s) => {
        const v = s && s.loops && s.loops.max_parallel_runs;
        if (v) { swarm.maxParallel = Math.max(1, Math.min(20, v)); e.par.value = swarm.maxParallel; publish(); }
      }).catch(() => {});
      e.par.onchange = async () => {
        swarm.maxParallel = Math.max(1, Math.min(20, parseInt(e.par.value, 10) || 3));
        e.par.value = swarm.maxParallel; publish();
        try { const s = await invoke('settings_get'); s.loops.max_parallel_runs = swarm.maxParallel; await invoke('settings_set', { settings: s }); } catch (_) {}
      };
      container.querySelector('[data-mag-send]').onclick = () => dispatch();
      e.input.onkeydown = (ev) => { if (ev.key === 'Enter' && !ev.shiftKey) { ev.preventDefault(); dispatch(); } };
      render();
    }

    return {
      mount,
      setRoot(newRoot) { root = newRoot; },
      destroy() { container = null; },
    };
  }

  register('multiagent', createMultiagentView());
})();
