// Agent status strip — Phase 4 of the Orca port. Shows one pill per active
// agent session with a colored dot reflecting its state. Clicking a pill
// jumps to the tab hosting that session.
(function () {
  'use strict';

  const $ = (id) => document.getElementById(id);
  const invoke = () => (window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke);
  const listen = () => (window.__TAURI__ && window.__TAURI__.event && window.__TAURI__.event.listen);

  // In-memory cache keyed by session_id. Updated by events; re-rendered on every change.
  const sessions = new Map();
  // Shared so terminal-agent-status.js can put the same dot + provider mark on tabs.
  window.xnautAgentSessions = sessions;

  function render() {
    if (window.xnautRefreshTabAgentDots) window.xnautRefreshTabAgentDots();
    const strip = $('agent-status-strip');
    if (!strip) return;
    if (sessions.size === 0) {
      strip.innerHTML = '';
      strip.hidden = true;
      return;
    }
    strip.hidden = false;
    const rows = Array.from(sessions.values()).sort((a, b) => a.started_at_ms - b.started_at_ms);
    strip.innerHTML = rows.map((s) => `
      <button class="agent-pill" data-session="${escapeAttr(s.session_id)}" title="${escapeAttr(s.agent_id + ' — ' + s.status)}">
        <span class="agent-dot" data-state="${escapeAttr(s.status)}" data-size="sm"></span>
        <span class="agent-pill-label">${escapeText(s.label || s.agent_id)}</span>
      </button>`).join('');
    strip.querySelectorAll('.agent-pill').forEach((btn) => {
      btn.onclick = () => {
        const sid = btn.dataset.session;
        if (typeof window.xnautFocusAgentSession === 'function') window.xnautFocusAgentSession(sid);
      };
    });
  }

  async function loadInitial() {
    const inv = invoke();
    if (!inv) return;
    try {
      const list = await inv('agent_sessions_list');
      sessions.clear();
      for (const s of list) sessions.set(s.session_id, s);
      render();
    } catch (_e) {
      // command may not be registered yet during first paint — retry once.
      setTimeout(loadInitial, 500);
    }
  }

  async function subscribe() {
    const l = listen();
    if (!l) {
      // Tauri not ready yet; try again on next tick.
      setTimeout(subscribe, 200);
      return;
    }
    await l('agent-status-changed', (event) => {
      const meta = event && event.payload;
      if (!meta || !meta.session_id) return;
      sessions.set(meta.session_id, meta);
      render();
    });
    await l('agent-status-dropped', (event) => {
      const sid = event && event.payload && event.payload.sessionId;
      if (!sid) return;
      sessions.delete(sid);
      render();
    });
  }

  function escapeText(s) {
    return String(s).replace(/[&<>"']/g, (c) => ({ '&':'&amp;', '<':'&lt;', '>':'&gt;', '"':'&quot;', "'":'&#39;' }[c]));
  }
  function escapeAttr(s) { return escapeText(s); }

  function start() {
    loadInitial();
    subscribe();
  }
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', start);
  } else {
    start();
  }
})();
