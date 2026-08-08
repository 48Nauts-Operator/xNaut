// Agent session cache.
//
// This began as the Orca status strip: one pill per live agent across the top
// bar. The pills were removed 2026-08-08 — the left sidebar already shows every
// project's agent state on its own row, so the strip restated it in a second
// place, and a row of them read as a Christmas tree.
//
// The module stays because the CACHE is what tab dots read
// (terminal-agent-status.js via window.xnautAgentSessions); only the rendering
// went. Deleting the file would have silently taken the tab dots with it.
(function () {
  'use strict';

  const invoke = () => (window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke);
  const listen = () => (window.__TAURI__ && window.__TAURI__.event && window.__TAURI__.event.listen);

  // In-memory cache keyed by session_id. Updated by events; re-rendered on every change.
  const sessions = new Map();
  // Shared so terminal-agent-status.js can put the same dot + provider mark on tabs.
  window.xnautAgentSessions = sessions;

  // Every state change still fans out to the surfaces that DO show it: the tab
  // dots here, and the sidebar rows via app.js's poll.
  function render() {
    if (window.xnautRefreshTabAgentDots) window.xnautRefreshTabAgentDots();
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
