// Agent quick view for the existing right pane. This deliberately shows only
// live or persisted data: no invented costs, activity, or terminal output.
(function () {
  'use strict';

  const invoke = (...args) => window.__TAURI__.core.invoke(...args);
  let selected = null;
  let container = null;
  let timer = null;

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
      .aqp-terminal { min-height:112px; max-height:190px; margin-top:9px; padding:10px; overflow:hidden; border:1px solid var(--border,#34343c); border-radius:7px;
        color:#bec5ce; background:#0d0e11; font:10px/1.45 var(--font-mono,"SF Mono",monospace); white-space:pre-wrap; overflow-wrap:anywhere; }
      .aqp-empty { color:#72727d; } .aqp-select { width:100%; min-width:0; padding:7px 8px; border:1px solid var(--border,#3a3a43); border-radius:6px;
        color:var(--text-primary,#e8e8ec); background:var(--input-bg,#232329); font:inherit; font-size:11px; }
      .aqp-button { width:100%; margin-top:12px; padding:8px; border:1px solid var(--border,#3a3a43); border-radius:7px; color:var(--text-primary,#e8e8ec);
        background:var(--bg-tertiary,#24242a); font:inherit; font-size:11px; cursor:pointer; }
      .aqp-button:hover { background:#2c2c33; } .aqp-status { display:inline-flex; align-items:center; gap:5px; text-transform:capitalize; }
      .aqp-dot { width:6px; height:6px; border-radius:50%; background:#71717a; }.aqp-dot.working { background:#4da3ff; }.aqp-dot.permission,.aqp-dot.blocked { background:#ff5f56; }
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
      <div class="aqp-row"><span class="aqp-label" style="margin:0">Artifact</span>
        <span style="display:flex;gap:10px"><button class="aqp-link" data-artifact-full>full screen</button><button class="aqp-link" data-artifact-close>close</button></span></div>
      <div class="aqp-artifact" data-artifact></div>
      <div class="aqp-tagline" title="${esc(url)}">${esc(url.replace(/^file:\/\//, ''))}</div></section>`;
  }

  async function mountArtifact() {
    const url = selected && artifacts.get(selected.handle);
    const box = container && container.querySelector('[data-artifact]');
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
    const full = container.querySelector('[data-artifact-full]');
    if (full) full.onclick = async () => {
      await unmountArtifact();
      if (window.xnautNewBrowserTab) window.xnautNewBrowserTab(url);
    };
    const close = container.querySelector('[data-artifact-close]');
    if (close) close.onclick = async () => {
      artifacts.delete(selected.handle);
      await unmountArtifact();
      render();
    };
  }

  // Called by browser-pane.js when an agent posts to /v1/open.
  window.xnautAgentArtifactOpen = (agentId, url) => {
    if (!agentId || !url) return false;
    artifacts.set(agentId, url);
    if (selected && selected.handle === agentId) render();
    return true;
  };

  async function currentState() {
    const sessions = (await invoke('agent_sessions_list').catch(() => [])) || [];
    const session = sessions.filter((item) => item.agent_id === selected.handle)
      .sort((left, right) => Number(right.last_output_at_ms || right.started_at_ms || 0) - Number(left.last_output_at_ms || left.started_at_ms || 0))[0] || null;
    return { session };
  }

  async function render() {
    // A throw in here left the slot empty, which is indistinguishable from the
    // pane being broken. Say what happened instead of showing nothing.
    try { await paint(); } catch (error) {
      console.error('[agent-quick-pane] render failed:', error);
      if (container) container.innerHTML = `<div class="rpane-empty">Could not render this agent: ${esc(String(error && error.message || error))}</div>`;
    }
  }

  async function paint() {
    if (!container) return;
    if (!selected) {
      container.innerHTML = '<div class="rpane-empty">Select an agent to see its live details.</div>';
      return;
    }
    const { session } = await currentState();
    if (!container || !selected) return;
    const models = window.xnautModelCatalog ? window.xnautModelCatalog.all() : [];
    const lines = session && window.xnautAgentSessionPreview ? window.xnautAgentSessionPreview(session.session_id, 14) : [];
    const status = session && session.status || 'idle';
    container.innerHTML = `<div class="aqp" style="--aqp-accent:${esc(selected.accent_color || '#f5b840')}">
      <section class="aqp-section"><div class="aqp-label">Agent</div><div class="aqp-ident"><div class="aqp-avatar">${esc(initials(selected))}</div><div><div class="aqp-name">${esc(selected.display_name)}</div><div class="aqp-handle">@${esc(selected.handle)}</div></div></div><div class="aqp-tagline">${esc(selected.tagline || selected.purpose)}</div></section>
      <section class="aqp-section"><div class="aqp-row"><span class="aqp-label" style="margin:0">Computer · ${esc(selected.execution || 'local')}</span>${session ? '<button class="aqp-link" data-terminal>open full screen</button>' : ''}</div>
        <div class="aqp-terminal">${lines.length ? esc(lines.join('\n')) : `<span class="aqp-empty">${session ? 'Terminal is attached; waiting for visible output.' : 'No active terminal for this agent.'}</span>`}</div>
        <div class="aqp-row" style="margin-top:9px"><span class="aqp-status"><span class="aqp-dot ${esc(status)}"></span>${esc(status)}</span><strong>${session ? 'Attached' : 'Not running'}</strong></div></section>
      <section class="aqp-section"><div class="aqp-label">Model</div><select class="aqp-select" data-model><option value="">Runtime default</option>${models.map((model) => `<option value="${esc(model.provider)}\t${esc(model.id)}" ${model.id === selected.model && model.provider === selected.provider ? 'selected' : ''}>${esc(model.provider)} · ${esc(model.name || model.id)}</option>`).join('')}</select></section>
      <section class="aqp-section"><div class="aqp-label">Cost</div><div class="aqp-row"><span>Per-agent attribution</span><strong>Not recorded</strong></div><div class="aqp-tagline">xNaut will not estimate or assign untagged provider usage to this agent.</div></section>
      ${artifactMarkup()}
      <section class="aqp-section" style="margin-top:auto"><button class="aqp-button" data-settings>Open settings</button></section>
    </div>`;
    await mountArtifact();
    const terminal = container.querySelector('[data-terminal]');
    if (terminal) terminal.onclick = () => window.xnautOpenAgentSession && window.xnautOpenAgentSession(session.session_id);
    container.querySelector('[data-settings]').onclick = () => window.xnautOpenAgentSettings && window.xnautOpenAgentSettings(selected.handle);
    container.querySelector('[data-model]').onchange = async (event) => {
      const [provider, model] = String(event.target.value || '').split('\t');
      const profile = { ...selected, provider:provider || selected.provider || 'global', model:model || '' };
      try {
        selected = await invoke('agent_profile_update', { handle:selected.handle, profile });
        window.dispatchEvent(new CustomEvent('xnaut:agent-profiles-changed', { detail:selected }));
        render();
      } catch (error) { console.error('[agent-quick-pane] model update failed:', error); }
    };
  }

  const view = {
    // The artifact preview is a native child webview: display:none on the slot
    // does not hide it, so it has to be taken down when this view is not the
    // one on screen and rebuilt when it is.
    hide() { unmountArtifact(); },
    show() { if (container) render(); },
    mount(element) {
      ensureStyles(); container = element; render();
      // Skip the refresh while an artifact is mounted: a re-render destroys
      // and recreates the child webview, which reloads the page under him.
      timer = setInterval(() => { if (container && container.isConnected && !mounted) render(); }, 3000);
    },
    setRoot() { render(); },
    destroy() {
      if (timer) clearInterval(timer);
      timer = null;
      container = null;
      unmountArtifact();
    },
  };

  if (window.xnautRightPaneRegisterView) window.xnautRightPaneRegisterView('agent', view);
  else (window.__xnautRightPaneQueue = window.__xnautRightPaneQueue || []).push({ key:'agent', view });

  window.xnautRightPaneOpenAgent = (profile) => {
    selected = profile || null;
    // xnautShowRightPane never existed (silent no-op — the CLAUDE.md
    // window.* trap); xnautEnsureRightPane opens AND mounts the host.
    if (window.xnautEnsureRightPane) window.xnautEnsureRightPane();
    if (window.xnautRightPaneShow) window.xnautRightPaneShow('agent');
    render();
  };
})();
