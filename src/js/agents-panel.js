(function () {
  'use strict';

  const ACCESS_PRESETS = [
    { id: 'full-project', label: 'Full Project Access' },
  ];

  function createAgentsPanel(tabId, host, opts) {
    if (!host) return null;
    const seed = (opts && opts.agentFatherSeed) || {};
    host.innerHTML = `
      <div class="agents-panel" style="height:100%; display:flex; flex-direction:column; padding:18px; gap:12px; overflow:auto;">
        <div>
          <h2 style="margin:0 0 6px; font-size:20px;">Agents</h2>
          <div style="color:var(--text-secondary); font-size:13px;">AgentFather library loading...</div>
        </div>
        <div style="border:1px solid var(--border-color); border-radius:8px; padding:14px; background:var(--bg-secondary);">
          <div style="font-weight:600; margin-bottom:6px;">AgentFather</div>
          <div style="color:var(--text-secondary); font-size:13px;">Preset: ${ACCESS_PRESETS[0].label}</div>
        </div>
      </div>`;
    return { tabId, host, seed, accessPresets: ACCESS_PRESETS };
  }

  window.xnautCreateAgentsPanel = createAgentsPanel;
  window.xnautOpenAgentFather = function (seed) {
    if (window.xnautAttachAgentsTab) {
      return window.xnautAttachAgentsTab({ agentFatherSeed: seed || {} });
    }
    return null;
  };
})();
