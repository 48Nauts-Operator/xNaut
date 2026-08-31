// Agent quick view for the existing right pane. This deliberately shows only
// live or persisted data: no invented costs, activity, or terminal output.
//
// André, 2026-08-30, reorganized around one information flow: "I am in an
// Agent window, say NautBot. On the right under the Agent icon we get a new
// date header 2026-08-30 that is collapsible. Inside we see the activities we
// currently show under Flow Watch. We also see the Cost part and the
// execution of any external sandbox we used, with the link to open it or view
// it full screen." So: identity on top, then ONE timeline grouped by date —
// today holds the live Flow Watch rows, account usage, the day's sandbox runs
// and ledger actions; past days keep their runs and actions, collapsed. The
// Flow Watch tab is gone (too many tabs); its view mounts here instead.
(function () {
  'use strict';

  const invoke = (...args) => window.__TAURI__.core.invoke(...args);
  const listen = (...args) => window.__TAURI__.event.listen(...args);
  let selected = null;
  let container = null;
  let identHost = null;
  let timelineHost = null;
  let fwSlot = null;      // stable element the Flow Watch view lives in
  let fwView = null;
  let timer = null;
  let usageTimer = null;
  let unlisten = null;

  const esc = (value) => String(value == null ? '' : value).replace(/[&<>"']/g, (character) => ({
    '&':'&amp;', '<':'&lt;', '>':'&gt;', '"':'&quot;', "'":'&#39;',
  }[character]));

  function ensureStyles() {
    if (document.getElementById('agent-quick-pane-styles')) return;
    const style = document.createElement('style');
    style.id = 'agent-quick-pane-styles';
    style.textContent = `
      .aqp { display:flex; flex-direction:column; min-height:100%; color:var(--text-primary,#e8e8ec); background:var(--bg-secondary,#17171c); }
      .aqp * { box-sizing:border-box; } .aqp-section { padding:15px 13px; border-bottom:1px solid var(--border,#303038); }
      .aqp-label { margin-bottom:10px; color:var(--text-secondary,#858590); font-size:9px; font-weight:750; letter-spacing:.1em; text-transform:uppercase; }
      .aqp-ident { display:flex; align-items:center; gap:10px; } .aqp-avatar { display:grid; place-items:center; width:38px; height:38px; flex:0 0 auto;
        border-radius:10px; color:#fff; background:var(--aqp-accent,#f5b840); font-size:13px; font-weight:750; }
      .aqp-name { color:var(--text-primary,#f0f0f3); font-size:14px; font-weight:680; } .aqp-handle { margin-top:2px; color:var(--text-secondary,#90909a); font-size:11px; }
      .aqp-tagline { margin-top:11px; color:var(--text-secondary,#a0a0aa); font-size:11px; line-height:1.5; }
      .aqp-row { display:flex; align-items:center; justify-content:space-between; gap:8px; color:var(--text-secondary,#92929d); font-size:11px; }
      .aqp-row strong { color:var(--text-primary,#e4e4e9); font-weight:600; text-align:right; }
      .aqp-link { padding:0; border:0; color:var(--agent-thinking,#f5b840); background:transparent; font:inherit; font-size:11px; cursor:pointer; }
      .aqp-artifact { position:relative; display:flex; min-height:230px; margin-top:9px; overflow:hidden;
        border:1px solid var(--border,#34343c); border-radius:7px; background:var(--bg-primary,#0a0a0f); }
      .aqp-artifact > * { flex:1 1 auto; min-width:0; }
      .aqp-empty { color:#72727d; }
      .aqp-button { width:100%; margin-top:12px; padding:8px; border:1px solid var(--border,#3a3a43); border-radius:7px; color:var(--text-primary,#e8e8ec);
        background:var(--bg-tertiary,#24242a); font:inherit; font-size:11px; cursor:pointer; }
      .aqp-button:hover { background:#2c2c33; } .aqp-status { display:inline-flex; align-items:center; gap:5px; text-transform:capitalize; }
      .aqp-dot { width:6px; height:6px; border-radius:50%; background:#71717a; }.aqp-dot.working,.aqp-dot.running { background:#4da3ff; }
      .aqp-dot.permission,.aqp-dot.blocked,.aqp-dot.failed { background:#ff5f56; }.aqp-dot.passed { background:#10b981; }
      .aqp-day { border-bottom:1px solid var(--border,#303038); }
      .aqp-day-head { display:flex; align-items:center; justify-content:space-between; gap:8px; padding:10px 13px; cursor:pointer; user-select:none; }
      .aqp-day-head:hover { background:var(--hover-bg,rgba(255,255,255,.04)); }
      .aqp-day-title { font-size:11px; font-weight:700; color:var(--text-primary,#e8e8ec); letter-spacing:.04em; }
      .aqp-day-count { font-size:10px; color:var(--text-secondary,#8a8f98); }
      .aqp-day-body { padding:0 13px 12px; display:flex; flex-direction:column; gap:10px; }
      .aqp-sub { margin:8px 0 4px; color:var(--text-secondary,#858590); font-size:9px; font-weight:750; letter-spacing:.1em; text-transform:uppercase; }
      .aqp-event { display:flex; gap:8px; align-items:baseline; font-size:11px; color:var(--text-primary,#d6d6dc); }
      .aqp-event time { flex:0 0 auto; font:10px var(--font-mono,monospace); color:var(--text-secondary,#8a8f98); }
      .aqp-event .k { flex:0 0 auto; color:var(--agent-thinking,#f5b840); }
      .aqp-event .d { flex:1 1 auto; min-width:0; color:var(--text-secondary,#a0a0aa); overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
      .aqp-sbx { display:flex; align-items:center; gap:8px; font-size:11px; }
      .aqp-sbx .t { flex:0 0 auto; font-weight:600; color:var(--text-primary,#e4e4e9); }
      .aqp-sbx .v { flex:1 1 auto; min-width:0; color:var(--text-secondary,#8a8f98); overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
    `;
    document.head.appendChild(style);
  }

  function initials(profile) {
    return String(profile && profile.display_name || '?').split(/\s+/).filter(Boolean).slice(0,2).map((part) => part[0].toUpperCase()).join('');
  }

  // ── Artifacts ───────────────────────────────────────────────────────────
  //
  // A page an agent builds is part of its answer, so it renders HERE, under
  // the agent that made it, rather than in a browser tab of its own. The
  // preview is a real child webview (browser-pane.js) floated over the box —
  // an iframe cannot load file:// from this origin, and a DOM overlay cannot
  // cover a native webview anyway.
  const artifacts = new Map(); // agent handle -> url
  let mounted = null; // { handle, url, label }

  async function unmountArtifact() {
    if (!mounted) return;
    const { label } = mounted;
    mounted = null;
    if (window.xnautDestroyBrowserPane) await window.xnautDestroyBrowserPane(label).catch(() => {});
  }

  function artifactMarkup() {
    const url = selected && artifacts.get(selected.handle);
    if (!url) return '';
    return `<section class="aqp-section">
      <div class="aqp-row"><span class="aqp-label" style="margin:0">${url.includes('.xterm.') ? 'Terminal' : 'Artifact'}</span>
        <span style="display:flex;gap:10px"><button class="aqp-link" data-artifact-full>full screen</button><button class="aqp-link" data-artifact-close>close</button></span></div>
      <div class="aqp-artifact" data-artifact></div>
      <div class="aqp-tagline" title="${esc(url)}">${esc(url.replace(/^file:\/\//, ''))}</div></section>`;
  }

  async function mountArtifact() {
    const url = selected && artifacts.get(selected.handle);
    const box = identHost && identHost.querySelector('[data-artifact]');
    if (!url || !box || !window.xnautCreateBrowserPane) { await unmountArtifact(); return; }
    await unmountArtifact();
    try {
      // RPANE_TAB keeps it visible across center-tab switches: the right pane
      // is window chrome, not a tab, so its webview must not follow tabs.
      const created = await window.xnautCreateBrowserPane('__rpane__', box, url);
      mounted = { handle: selected.handle, url, label: created && created.label };
    } catch (error) {
      console.error('[agent-quick-pane] artifact preview failed:', error);
      box.innerHTML = `<span class="aqp-empty">Could not preview this page. Open it full screen instead.</span>`;
    }
    const full = identHost.querySelector('[data-artifact-full]');
    if (full) full.onclick = async () => {
      await unmountArtifact();
      if (window.xnautNewBrowserTab) window.xnautNewBrowserTab(url);
    };
    const close = identHost.querySelector('[data-artifact-close]');
    if (close) close.onclick = async () => {
      artifacts.delete(selected.handle);
      await unmountArtifact();
      paintIdentity();
    };
  }

  // Called by browser-pane.js when an agent posts to /v1/open.
  window.xnautAgentArtifactOpen = (agentId, url) => {
    if (!agentId || !url) return false;
    artifacts.set(agentId, url);
    if (selected && selected.handle === agentId) paintIdentity();
    return true;
  };

  // ── exe.dev computers ───────────────────────────────────────────────────
  //
  // A VM an agent spins up is a computer it owns, but it lives in exe.dev's
  // control plane, so this asks exe.dev instead of tracking it. Clicking
  // terminal mounts the VM's web terminal in the same child webview the
  // artifact preview uses, so the machine is visible here, not just described
  // in the transcript.
  // ponytail: the whole account's machines, not this agent's. xNaut does not
  // create them, so it cannot know whose is whose; tag them per agent once
  // xNaut is the one calling `new`.
  let machines = [];
  let machinesAt = 0;

  async function refreshMachines() {
    if (machinesAt && Date.now() - machinesAt < 30000) return;
    machinesAt = Date.now();
    try { machines = (await invoke('exe_machines')) || []; } catch (error) { machines = []; }
  }

  function machinesMarkup() {
    if (!machines.length) return '';
    return `<section class="aqp-section"><div class="aqp-label">Computer · exe.dev</div>
      ${machines.map((vm) => `<div class="aqp-row" style="margin-top:9px">
        <span class="aqp-status"><span class="aqp-dot ${vm.status === 'running' ? 'working' : ''}"></span>${esc(vm.emoji || '')} ${esc(vm.vm_name)}</span>
        <span style="display:flex;gap:10px"><button class="aqp-link" data-vm-shell="${esc(vm.ssh_dest || `${vm.vm_name}.exe.xyz`)}">terminal</button><button class="aqp-link" data-vm-web="${esc(vm.https_url)}">web</button></span></div>
        <div class="aqp-tagline" style="margin-top:3px">${esc(vm.ssh_command || `ssh ${vm.vm_name}.exe.xyz`)} · ${esc(vm.status || '')}</div>`).join('')}
    </section>`;
  }

  // ── Timeline data ────────────────────────────────────────────────────────
  let ledgerEntries = [];
  let verifyRecords = [];
  let usage = { max: null, codex: null };
  const dayOpen = new Map(); // 'YYYY-MM-DD' -> bool

  function dateKey(iso) {
    const t = new Date(iso);
    if (Number.isNaN(t.getTime())) return null;
    const p = (n) => String(n).padStart(2, '0');
    return `${t.getFullYear()}-${p(t.getMonth() + 1)}-${p(t.getDate())}`;
  }
  const todayKey = () => dateKey(new Date().toISOString());
  const clock = (iso) => new Date(iso).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });

  async function refreshTimelineData() {
    try { ledgerEntries = (await invoke('ledger_recent', { limit: 400 })) || []; } catch (_) { ledgerEntries = []; }
    try { verifyRecords = (await invoke('sandbox_verify_records')) || []; } catch (_) { verifyRecords = []; }
  }

  async function refreshUsage() {
    try { usage.max = await invoke('max_usage', { account: null }); } catch (_) { usage.max = null; }
    try { usage.codex = await invoke('codex_usage'); } catch (_) { usage.codex = null; }
  }

  function verifyVerdict(record) {
    const done = (record.steps || []).filter((s) => s.exit_code !== null && s.exit_code !== undefined);
    const red = done.find((s) => s.exit_code !== 0);
    if (record.status === 'passed') return `passed · ${record.sandbox_id || record.provider_kind}`;
    if (record.status === 'failed') return red ? `failed at ${red.name} (exit ${red.exit_code})` : 'failed';
    const next = (record.steps || [])[done.length];
    return next ? `running: ${next.name}…` : 'starting…';
  }

  // Reaching a sandbox: exe.dev's web pages sit behind their login, and the
  // OAuth hop escapes any embedded webview (hit live 2026-08-30), so an exe
  // run opens a real xNAUT terminal SSH'd into the VM instead — this machine's
  // key is already authorized, no web auth exists on that path. gitvm's proxy
  // URL is public, so it keeps the web buttons.
  function sandboxRow(record) {
    const isExe = record.provider_kind === 'exe-ssh' && record.sandbox_id;
    const open = record.public_url || '';
    return `<div class="aqp-sbx"><span class="aqp-dot ${esc(record.status)}"></span>
      <span class="t">${esc(record.ticket_id || record.project || '?')}</span>
      <span class="v">${esc(verifyVerdict(record))}</span>
      ${isExe ? `<button class="aqp-link" data-sbx-shell="${esc(record.id)}">shell</button>` : ''}
      ${!isExe && open ? `<button class="aqp-link" data-sbx-view="${esc(open)}">view</button>` : ''}
      ${!isExe && open ? `<button class="aqp-link" data-sbx-open="${esc(open)}">full screen</button>` : ''}
    </div>`;
  }

  // Same slug the Rust driver uses for the VM-side workdir (sandbox.rs exe::workdir).
  function sandboxWorkdir(project) {
    const slug = String(project || '').split('').map((c) => (/[a-zA-Z0-9]/.test(c) ? c.toLowerCase() : '-')).join('');
    return `verify/${slug}`;
  }

  async function openSandboxShell(record) {
    const host = `${record.sandbox_id}.exe.xyz`;
    const workdir = sandboxWorkdir(record.project);
    try {
      const result = await invoke('create_command_session', {
        config: {
          program: 'zsh',
          // -t forces a PTY; land in the run's workdir, fall back to home.
          args: ['-lc', `ssh -t -o StrictHostKeyChecking=accept-new ${host} 'cd ${workdir} 2>/dev/null; exec bash -l'`],
          workingDir: '~/',
          env: {},
        },
      });
      if (window.xnautAttachAgentTab) window.xnautAttachAgentTab(result.session_id, `sandbox · ${record.sandbox_id}`);
    } catch (error) {
      console.error('[agent-quick-pane] sandbox shell failed:', error);
    }
  }

  function eventRow(entry) {
    return `<div class="aqp-event"><time>${esc(clock(entry.at))}</time>
      <span class="k">${esc(entry.kind)}</span>
      ${entry.agent ? `<span>@${esc(entry.agent)}</span>` : ''}
      ${entry.ticket ? `<span>${esc(entry.ticket)}</span>` : ''}
      <span class="d">${esc(entry.detail || '')}</span></div>`;
  }

  function costMarkup() {
    const parts = [];
    if (usage.max) {
      parts.push(`<div class="aqp-row"><span>Claude Max · 5h</span><strong>${Math.round(usage.max.five_hour_pct)}%</strong></div>
        <div class="aqp-row"><span>Claude Max · 7d</span><strong>${Math.round(usage.max.seven_day_pct)}%</strong></div>`);
    }
    if (usage.codex && usage.codex.primary) {
      parts.push(`<div class="aqp-row"><span>Codex · ${esc(usage.codex.primary.window_label)}</span><strong>${Math.round(usage.codex.primary.used_percent)}%</strong></div>`);
    }
    if (!parts.length) return '';
    return `<div><div class="aqp-sub">Cost · account-wide plan usage</div>${parts.join('')}</div>`;
  }

  function paintTimeline() {
    try {
      paintTimelineInner();
    } catch (error) {
      console.error('[agent-quick-pane] timeline paint failed:', error);
      if (timelineHost) timelineHost.innerHTML = `<div class="aqp-section" style="color:var(--alarm,#ff6568)">Timeline failed to render: ${esc(String(error && error.message || error))}</div>`;
    }
  }

  function paintTimelineInner() {
    if (!timelineHost) return;
    const today = todayKey();
    const byDay = new Map(); // date -> { events, runs }
    const bucket = (key) => {
      if (!key) return null;
      if (!byDay.has(key)) byDay.set(key, { events: [], runs: [] });
      return byDay.get(key);
    };
    for (const entry of ledgerEntries) {
      const b = bucket(dateKey(entry.at));
      if (b) b.events.push(entry);
    }
    for (const record of verifyRecords) {
      const b = bucket(dateKey(record.created_at));
      if (b) b.runs.push(record);
    }
    bucket(today); // today always exists: it holds the live view and cost.

    const dates = [...byDay.keys()].sort().reverse();
    timelineHost.textContent = '';
    for (const date of dates) {
      const { events, runs } = byDay.get(date);
      const isToday = date === today;
      if (!dayOpen.has(date)) dayOpen.set(date, isToday);
      const open = dayOpen.get(date);

      const group = document.createElement('div');
      group.className = 'aqp-day';
      const head = document.createElement('div');
      head.className = 'aqp-day-head';
      head.innerHTML = `<span class="aqp-day-title">${esc(date)}${isToday ? ' · today' : ''}</span>
        <span class="aqp-day-count">${runs.length ? `${runs.length} sandbox · ` : ''}${events.length} events ${open ? '▾' : '▸'}</span>`;
      const body = document.createElement('div');
      body.className = 'aqp-day-body';
      body.style.display = open ? 'flex' : 'none';
      head.onclick = () => {
        const now = !dayOpen.get(date);
        dayOpen.set(date, now);
        body.style.display = now ? 'flex' : 'none';
        paintTimeline();
      };

      if (isToday) {
        // The live Flow Watch view is a mounted component with its own event
        // subscriptions; it is MOVED into place, never rebuilt, so repainting
        // the timeline costs it nothing.
        const live = document.createElement('div');
        live.innerHTML = '<div class="aqp-sub">Live</div>';
        live.appendChild(fwSlot);
        body.appendChild(live);
        const cost = document.createElement('div');
        cost.innerHTML = costMarkup();
        body.appendChild(cost);
      }
      if (runs.length) {
        const sbx = document.createElement('div');
        sbx.innerHTML = `<div class="aqp-sub">Sandbox runs</div>${runs.map(sandboxRow).join('')}`;
        body.appendChild(sbx);
      }
      if (events.length) {
        const acts = document.createElement('div');
        acts.innerHTML = `<div class="aqp-sub">Actions</div>${events.map(eventRow).join('')}`;
        body.appendChild(acts);
      }
      group.append(head, body);
      timelineHost.appendChild(group);
    }

    timelineHost.querySelectorAll('[data-sbx-shell]').forEach((button) => {
      button.onclick = () => {
        const record = verifyRecords.find((r) => r.id === button.dataset.sbxShell);
        if (record) openSandboxShell(record);
      };
    });
    timelineHost.querySelectorAll('[data-sbx-view]').forEach((button) => {
      button.onclick = () => selected && window.xnautAgentArtifactOpen(selected.handle, button.dataset.sbxView);
    });
    timelineHost.querySelectorAll('[data-sbx-open]').forEach((button) => {
      button.onclick = () => window.xnautNewBrowserTab && window.xnautNewBrowserTab(button.dataset.sbxOpen);
    });
  }

  async function paintIdentity() {
    if (!identHost) return;
    if (!selected) {
      identHost.innerHTML = '<div class="rpane-empty">Select an agent to see its live details.</div>';
      return;
    }
    await refreshMachines();
    // The redesign dropped the "open full screen" affordance with the old
    // terminal section (André, 2026-08-31: "I can't open the session inside
    // the agent as I used to"). Restored here: newest session for this
    // agent, opened as a real terminal tab.
    let session = null;
    try {
      const sessions = (await invoke('agent_sessions_list')) || [];
      session = sessions
        .filter((item) => item.agent_id === selected.handle)
        .sort((a, b) => Number(b.last_output_at_ms || b.started_at_ms || 0) - Number(a.last_output_at_ms || a.started_at_ms || 0))[0] || null;
    } catch (_) {}
    if (!identHost || !selected) return;
    // The model is shown, not switched: changing an agent's model is a
    // deliberate act and lives in settings (André, 2026-08-30: "I don't see
    // the point to change the model inside the tab").
    identHost.innerHTML = `
      <section class="aqp-section"><div class="aqp-label">Agent</div><div class="aqp-ident"><div class="aqp-avatar">${esc(initials(selected))}</div><div><div class="aqp-name">${esc(selected.display_name)}</div><div class="aqp-handle">@${esc(selected.handle)}</div></div></div><div class="aqp-tagline">${esc(selected.tagline || selected.purpose)}</div>
      <div class="aqp-row" style="margin-top:11px"><span>Model</span><strong>${esc(selected.provider || 'global')} · ${esc(selected.model || 'runtime default')}</strong></div>
      ${session ? `<div class="aqp-row" style="margin-top:9px"><span class="aqp-status"><span class="aqp-dot ${esc(session.status)}"></span>${esc(session.status)}</span><button class="aqp-link" data-open-session="${esc(session.session_id)}">open full screen</button></div>` : ''}</section>
      ${machinesMarkup()}
      ${artifactMarkup()}
      <section class="aqp-section"><button class="aqp-button" data-settings>Open settings</button></section>`;
    await mountArtifact();
    // SSH, not the xterm web page: that page is private and its login's OAuth
    // hop escapes an embedded webview (same wall as the sandbox rows).
    identHost.querySelectorAll('[data-vm-shell]').forEach((button) => {
      button.onclick = async () => {
        try {
          const result = await invoke('create_command_session', {
            config: { program: 'zsh', args: ['-lc', `ssh -t -o StrictHostKeyChecking=accept-new ${button.dataset.vmShell}`], workingDir: '~/', env: {} },
          });
          if (window.xnautAttachAgentTab) window.xnautAttachAgentTab(result.session_id, `vm · ${button.dataset.vmShell.split('.')[0]}`);
        } catch (error) { console.error('[agent-quick-pane] vm shell failed:', error); }
      };
    });
    identHost.querySelectorAll('[data-vm-web]').forEach((button) => {
      button.onclick = () => window.xnautNewBrowserTab && window.xnautNewBrowserTab(button.dataset.vmWeb);
    });
    identHost.querySelectorAll('[data-open-session]').forEach((button) => {
      button.onclick = () => window.xnautOpenAgentSession && window.xnautOpenAgentSession(button.dataset.openSession, selected.display_name);
    });
    const settings = identHost.querySelector('[data-settings]');
    if (settings) settings.onclick = () => window.xnautOpenAgentSettings && window.xnautOpenAgentSettings(selected.handle);
  }

  async function render() {
    // A throw in here left the slot empty, which is indistinguishable from the
    // pane being broken. Say what happened instead of showing nothing.
    try {
      await paintIdentity();
      paintTimeline();
    } catch (error) {
      console.error('[agent-quick-pane] render failed:', error);
      if (identHost) identHost.innerHTML = `<div class="rpane-empty">Could not render this agent: ${esc(String(error && error.message || error))}</div>`;
    }
  }

  const view = {
    // The artifact preview is a native child webview: display:none on the slot
    // does not hide it, so it has to be taken down when this view is not the
    // one on screen and rebuilt when it is.
    hide() { unmountArtifact(); },
    show() { if (container) render(); },
    mount(element) {
      ensureStyles();
      container = element;
      const root = document.createElement('div');
      root.className = 'aqp';
      identHost = document.createElement('div');
      timelineHost = document.createElement('div');
      fwSlot = document.createElement('div');
      root.append(identHost, timelineHost);
      container.appendChild(root);
      // Flow Watch lives here now; its old tab is gone (too many tabs).
      if (window.xnautFlowWatchView && !fwView) {
        fwView = window.xnautFlowWatchView;
        try { fwView.mount(fwSlot); } catch (error) { console.warn('[agent-quick-pane] flow watch mount failed:', error); }
      }
      (async () => {
        await Promise.all([refreshTimelineData(), refreshUsage()]);
        render();
      })();
      listen('sandbox-verify-changed', async () => {
        await refreshTimelineData();
        paintTimeline();
      }).then((u) => { unlisten = u; }).catch(() => {});
      timer = setInterval(async () => {
        if (!container || !container.isConnected) return;
        await refreshTimelineData();
        paintTimeline();
      }, 15000);
      usageTimer = setInterval(async () => {
        if (!container || !container.isConnected) return;
        await refreshUsage();
        paintTimeline();
      }, 60000);
    },
    setRoot() { render(); },
    destroy() {
      if (timer) clearInterval(timer);
      if (usageTimer) clearInterval(usageTimer);
      timer = null; usageTimer = null;
      if (unlisten) { try { unlisten(); } catch (_) {} unlisten = null; }
      if (fwView) { try { fwView.destroy(); } catch (_) {} fwView = null; }
      container = null; identHost = null; timelineHost = null; fwSlot = null;
      unmountArtifact();
    },
  };

  if (window.xnautRightPaneRegisterView) window.xnautRightPaneRegisterView('agent', view);
  else (window.__xnautRightPaneQueue = window.__xnautRightPaneQueue || []).push({ key:'agent', view });

  window.xnautRightPaneOpenAgent = (profile) => {
    selected = profile || null;
    machinesAt = 0; // opening the pane is a deliberate look: re-ask exe.dev.
    // xnautShowRightPane never existed (silent no-op — the CLAUDE.md
    // window.* trap); xnautEnsureRightPane opens AND mounts the host.
    if (window.xnautEnsureRightPane) window.xnautEnsureRightPane();
    if (window.xnautRightPaneShow) window.xnautRightPaneShow('agent');
    render();
  };
})();
