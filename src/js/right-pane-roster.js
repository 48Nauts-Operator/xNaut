// Agent roster — one row per role: harness, provider, model.
//
// The choices already existed, one dropdown per stage toolbar, which meant you
// could not see who was playing what without walking nine stages. Same data,
// one table.
(function () {
  'use strict';

  const esc = (s) => String(s == null ? '' : s).replace(/[&<>"']/g, (c) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  }[c]));

  const HARNESS = [['claude', 'Claude Code'], ['codex', 'Codex'], ['pi', 'Pi']];
  const PROVIDER_LABEL = {
    anthropic: 'Anthropic', openai: 'OpenAI', openrouter: 'OpenRouter',
    lmstudio: 'LM Studio (local)', ollama: 'Ollama (local)', nautgate: 'NautGate',
  };

  function styles() {
    if (document.getElementById('rpros-styles')) return;
    const st = document.createElement('style');
    st.id = 'rpros-styles';
    st.textContent = `
      .rpros { padding: 12px 14px; display: flex; flex-direction: column; gap: 12px;
        color: var(--text-primary, #ddd); font-size: 13px; overflow-y: auto; height: 100%; }
      .rpros h3 { margin: 0; font-size: 13px; font-weight: 600; }
      .rpros-note { font-size: 11px; color: var(--text-muted, #777); line-height: 1.5; }
      .rpros-scope { display: flex; gap: 6px; align-items: center; flex-wrap: wrap; }
      .rpros-scope button { padding: 4px 10px; border-radius: 999px; cursor: pointer; font: inherit;
        font-size: 11px; border: 1px solid var(--border-color, #3a3d45); background: transparent; color: inherit; }
      .rpros-scope button.on { border-color: var(--accent, #4da3ff); color: var(--accent, #4da3ff); }
      .rpros-row { display: grid; grid-template-columns: 1fr 1fr 1fr 1.2fr; gap: 6px; align-items: center; }
      .rpros-head { font-size: 10px; letter-spacing: .08em; text-transform: uppercase;
        color: var(--text-muted, #777); }
      .rpros-role { display: flex; flex-direction: column; gap: 1px; min-width: 0; }
      .rpros-role b { font-weight: 600; font-size: 12.5px; }
      .rpros-role span { font-size: 10.5px; color: var(--text-muted, #777); }
      .rpros select { background: var(--input-bg, rgba(255,255,255,.05));
        border: 1px solid var(--border-color, #3a3d45); border-radius: 6px; color: inherit;
        font: inherit; font-size: 12px; padding: 4px 6px; outline: none; min-width: 0; }
      .rpros select:focus { border-color: var(--accent, #4da3ff); }
      .rpros-stale { grid-column: 1 / -1; font-size: 11px; color: #F5B840; padding: 0 0 4px 2px; }
      .rpros-actions { display: flex; gap: 8px; margin-top: 4px; }
      .rpros-btn { padding: 6px 12px; border-radius: 6px; border: 1px solid var(--border-color, #3a3d45);
        background: transparent; color: inherit; font: inherit; font-size: 12px; cursor: pointer; }
    `;
    document.head.appendChild(st);
  }

  function mount(container) {
    styles();
    let scope = 'global'; // 'global' | 'project'

    const project = () => (scope === 'project' && window.xnautActiveProjectKey
      && window.xnautActiveProjectKey()) || null;

    function providers() {
      const cat = window.xnautModelCatalog;
      const seen = new Set(((cat && cat.all()) || []).map((m) => m.provider).filter(Boolean));
      for (const k of Object.keys(PROVIDER_LABEL)) seen.add(k);
      return [...seen].sort();
    }

    function modelsFor(p) {
      const cat = window.xnautModelCatalog;
      return ((cat && cat.forProvider(p)) || []).map((m) => (typeof m === 'string' ? m : (m.id || m.name))).filter(Boolean);
    }

    function render() {
      const roster = window.xnautAgentRoster;
      if (!roster) { container.innerHTML = '<div class="rpros">Roster module not loaded.</div>'; return; }
      const proj = project();
      const rows = roster.all(proj);

      container.innerHTML = `<div class="rpros">
        <h3>Agent roster</h3>
        <div class="rpros-note">Who plays each role. A different provider for the Reviewer than the Architect is
          deliberate — different training, different blind spots. Setting everything to the strongest model loses that.</div>
        <div class="rpros-scope">
          <button data-scope="global" class="${scope === 'global' ? 'on' : ''}">Default for all projects</button>
          <button data-scope="project" class="${scope === 'project' ? 'on' : ''}">This project only</button>
          ${scope === 'project' && !proj ? '<span class="rpros-note">No project selected — showing defaults.</span>' : ''}
        </div>
        <div class="rpros-row rpros-head"><span>Role</span><span>Harness</span><span>Provider</span><span>Model</span></div>
        ${rows.map((r) => {
          const b = r.binding;
          return `${b.stale ? `<div class="rpros-stale">⚠ ${esc(r.label)} was pinned to “${esc(b.pinned)}”, which is no longer in the catalogue — using ${esc(b.model || 'the provider default')} until you choose again.</div>` : ''}
          <div class="rpros-row" data-role="${esc(r.id)}">
            <div class="rpros-role"><b>${esc(r.label)}</b>${r.note ? `<span>${esc(r.note)}</span>` : ''}</div>
            <select data-f="harness">${HARNESS.map(([v, l]) => `<option value="${v}"${b.harness === v ? ' selected' : ''}>${esc(l)}</option>`).join('')}</select>
            <select data-f="provider">${providers().map((p) => `<option value="${esc(p)}"${b.provider === p ? ' selected' : ''}>${esc(PROVIDER_LABEL[p] || p)}</option>`).join('')}</select>
            <select data-f="model"><option value="">Provider default</option>${modelsFor(b.provider).map((m) => `<option value="${esc(m)}"${b.model === m ? ' selected' : ''}>${esc(m)}</option>`).join('')}</select>
          </div>`;
        }).join('')}
        <div class="rpros-actions"><button class="rpros-btn rpros-reset">Reset to defaults</button></div>
        <div class="rpros-note">Defaults come from the live model catalogue, not a hardcoded list — so a retired
          model does not leave a role pointing at something that no longer exists.</div>
      </div>`;

      container.querySelectorAll('.rpros-scope button').forEach((b) => {
        b.onclick = () => { scope = b.dataset.scope; render(); };
      });
      container.querySelectorAll('.rpros-row[data-role]').forEach((row) => {
        const roleId = row.dataset.role;
        row.querySelectorAll('select').forEach((sel) => {
          sel.onchange = () => {
            const get = (f) => row.querySelector(`select[data-f="${f}"]`).value;
            const provider = get('provider');
            // Changing the provider invalidates the model, so clear rather than
            // carry a model the new provider does not serve.
            const model = sel.dataset.f === 'provider' ? '' : get('model');
            roster.setRole(roleId, { harness: get('harness'), provider, model }, project());
            render();
          };
        });
      });
      const reset = container.querySelector('.rpros-reset');
      if (reset) reset.onclick = () => {
        rows.forEach((r) => roster.setRole(r.id, null, project()));
        render();
      };
    }

    render();
    const onCatalogue = () => render();
    window.addEventListener('xnaut-model-catalog-update', onCatalogue);
    if (window.xnautModelCatalog) window.xnautModelCatalog.refreshIfStale();
    container.__rprosCleanup = () => window.removeEventListener('xnaut-model-catalog-update', onCatalogue);
  }

  const view = {
    mount,
    setRoot() {},
    destroy(container) { if (container && container.__rprosCleanup) container.__rprosCleanup(); },
  };
  if (window.xnautRightPaneRegisterView) window.xnautRightPaneRegisterView('roster', view);
  else (window.__xnautRightPaneQueue = window.__xnautRightPaneQueue || []).push({ key: 'roster', view });
})();
