// Terminal-tab agent status (Orca-style). Puts a provider mark + a live status
// dot on each terminal tab that's running an agent: a spinning mark while it
// works, a green dot when it's done. Reuses the shared .agent-dot states and
// the agent-session cache exposed by status-strip.js (window.xnautAgentSessions).
//
// Wiring: renderTabs() stamps each agent tab with data-agent-session-id and
// calls window.xnautRefreshTabAgentDots(); status-strip.js also calls it whenever
// an agent-status-changed / -dropped event updates the session cache.
(function () {
  'use strict';

  // Simplified, brand-coloured provider marks — stylised, not pixel-exact logos.
  const PROVIDER = {
    claude: '<svg viewBox="0 0 16 16" aria-hidden="true"><path fill="#D97757" d="M8 1.2l1.2 3.9 3.6-1.9-1.9 3.6 3.9 1.2-3.9 1.2 1.9 3.6-3.6-1.9L8 14.8l-1.2-3.9-3.6 1.9 1.9-3.6L1.2 8l3.9-1.2L3.2 3.2l3.6 1.9z"/></svg>',
    codex: '<svg viewBox="0 0 16 16" aria-hidden="true"><path fill="none" stroke="#10a37f" stroke-width="1.4" d="M8 2.4a2.8 2.8 0 0 1 2.7 2 2.8 2.8 0 0 1 .8 5 2.8 2.8 0 0 1-5.2 1.8 2.8 2.8 0 0 1-2.8-4.6A2.8 2.8 0 0 1 5.3 3 2.8 2.8 0 0 1 8 2.4Z"/></svg>',
    gemini: '<svg viewBox="0 0 16 16" aria-hidden="true"><path fill="#4285F4" d="M8 0c.3 4.2 3.5 7.5 8 8-4.5.5-7.7 3.8-8 8-.3-4.2-3.5-7.5-8-8 4.5-.5 7.7-3.8 8-8Z"/></svg>',
    grok: '<svg viewBox="0 0 16 16" aria-hidden="true"><path fill="currentColor" d="M3 2h2.6l7.4 12h-2.6L3 2Zm10 0h-2.4L7.5 6.9 8.8 9 13 2ZM3 14h2.4l2-3.2L6.1 8.7 3 14Z"/></svg>',
    opencode: '<svg viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="#a78bfa" stroke-width="1.4"><path d="M6 4 2.5 8 6 12M10 4l3.5 4L10 12"/></svg>',
    pi: '<svg viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="#22c55e" stroke-width="1.5"><path d="M3.5 5.5h9M6 5.5v7M11 5.5v5.5a1 1 0 0 0 1.6.8"/></svg>',
  };
  const DEFAULT = '<svg viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.3"><rect x="3" y="5" width="10" height="8" rx="2"/><circle cx="6" cy="9" r="1" fill="currentColor" stroke="none"/><circle cx="10" cy="9" r="1" fill="currentColor" stroke="none"/><path d="M8 5V3.2"/></svg>';

  function injectStyles() {
    if (document.getElementById('tab-agent-status-styles')) return;
    const s = document.createElement('style');
    s.id = 'tab-agent-status-styles';
    s.textContent = `
      .tab-provider-icon { display:inline-flex; align-items:center; width:13px; height:13px; flex:0 0 auto; margin-right:5px; }
      .tab-provider-icon svg { width:13px; height:13px; }
      .tab .agent-dot.tab-agent-dot { margin-right:5px; }
    `;
    document.head.appendChild(s);
  }

  // Decorate every terminal tab: agent tabs get a provider mark + status dot;
  // non-agent (or finished/removed) tabs get them stripped.
  window.xnautRefreshTabAgentDots = function () {
    injectStyles();
    const sessions = window.xnautAgentSessions;
    document.querySelectorAll('.tab').forEach((el) => {
      const sid = el.dataset.agentSessionId;
      const s = sid && sessions ? sessions.get(sid) : null;
      let icon = el.querySelector('.tab-provider-icon');
      let dot = el.querySelector('.tab-agent-dot');
      if (s) {
        const anchor = el.querySelector('.tab-name') || el.firstChild;
        if (!icon) {
          icon = document.createElement('span');
          icon.className = 'tab-provider-icon';
          el.insertBefore(icon, anchor);
        }
        icon.innerHTML = PROVIDER[s.agent_id] || DEFAULT;
        icon.title = s.agent_id || 'agent';
        if (!dot) {
          dot = document.createElement('span');
          dot.className = 'agent-dot tab-agent-dot';
          dot.dataset.size = 'sm';
          el.insertBefore(dot, anchor);
        }
        dot.dataset.state = s.status || 'idle';
        dot.title = s.status || '';
      } else {
        if (icon) icon.remove();
        if (dot) dot.remove();
      }
    });
  };
})();
