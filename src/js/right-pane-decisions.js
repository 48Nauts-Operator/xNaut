// Decisions — the readable end of the decision log (decisions.rs).
//
// The log answers "why is it like this" rather than "what did it do", so this
// view is deliberately NOT a log tail. It renders the three layers the brief
// already computes, in the order you need them: headline (five seconds), open
// (before you touch anything), detail (when you actually care).
//
// The rule inherited from the backend and not negotiable here: this surface may
// SHORTEN and GROUP, it may never RESOLVE. Open items are rendered in full,
// never collapsed behind a count, and the number of boundaries recorded with no
// rationale is stated rather than quietly rounded away. A viewer that tidies an
// open disagreement into a tick is worse than no viewer.
//
// STYLING NOTE (same trap as right-pane-buildlog.js): undefined CSS vars are
// silent — --hover-bg, --input-bg and --text-muted are used elsewhere in this
// codebase and defined nowhere, so they resolve to nothing and produce
// invisible text. Every var below carries a literal fallback.
(function () {
  'use strict';

  const invoke = (cmd, args) => window.__TAURI__.core.invoke(cmd, args);

  // right-pane.js keeps its escape helper private, so this file has its own.
  function esc(s) {
    return String(s == null ? '' : s).replace(/[&<>"]/g, (c) =>
      ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]);
  }

  // Mirror of engram::project_from_cwd. The host hands views a filesystem root,
  // not a project name, and a worktree lives at <project>/.worktrees/<branch> —
  // so the leaf is a branch and the project is the component before it. Get this
  // wrong and every worktree reads an empty log of its own.
  function projectOf(root) {
    const parts = String(root || '').split('/').filter(Boolean);
    const i = parts.indexOf('.worktrees');
    if (i > 0) return parts[i - 1];
    return parts.length ? parts[parts.length - 1] : '';
  }

  const STYLES = `
  .dl-wrap { display: flex; flex-direction: column; height: 100%; overflow: hidden;
    font-family: var(--font-sans, sans-serif); color: var(--text-primary, #e8e6e1); }
  .dl-head { display: flex; align-items: center; gap: 8px; padding: 8px 10px;
    border-bottom: 1px solid var(--border, #24262c); flex: 0 0 auto; }
  .dl-project { font-size: 12px; font-weight: 600; }
  .dl-note { font-size: 11px; color: var(--text-secondary, #8a8f98); margin-left: auto; }
  .dl-note.warn { color: #E8A33D; }
  .dl-body { flex: 1 1 auto; overflow-y: auto; padding: 10px; }
  .dl-section { margin-bottom: 16px; }
  .dl-label { font-size: 10px; letter-spacing: 0.08em; text-transform: uppercase;
    color: var(--text-secondary, #8a8f98); margin-bottom: 6px; }
  .dl-item { border-left: 2px solid var(--border, #24262c); padding: 4px 0 6px 8px;
    margin-bottom: 6px; }
  .dl-item.open { border-left-color: #E8A33D; }
  .dl-item.fail { border-left-color: #E0524A; }
  .dl-meta { font-size: 10px; color: var(--text-secondary, #8a8f98);
    font-family: var(--font-mono, monospace); display: flex; gap: 6px; }
  .dl-role { color: #7FA6D9; }
  .dl-why { font-size: 12px; line-height: 1.45; margin-top: 2px; }
  .dl-why.missing { color: #E8A33D; font-style: italic; }
  .dl-what, .dl-alt { font-size: 11px; color: var(--text-secondary, #8a8f98);
    margin-top: 2px; line-height: 1.4; }
  .dl-empty { font-size: 12px; color: var(--text-secondary, #8a8f98); padding: 16px 4px; }
  .dl-detail summary { font-size: 10px; letter-spacing: 0.08em; text-transform: uppercase;
    color: var(--text-secondary, #8a8f98); cursor: pointer; margin-bottom: 6px; }
  `;

  function styleOnce() {
    if (document.getElementById('dl-styles')) return;
    const el = document.createElement('style');
    el.id = 'dl-styles';
    el.textContent = STYLES;
    document.head.appendChild(el);
  }

  function time(ts) {
    const d = new Date(ts);
    return isNaN(d) ? '' : d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  }

  function item(d) {
    const cls = d.open ? 'open' : (d.boundary === 'verify.fail' ? 'fail' : '');
    const why = d.why && d.why.trim()
      ? `<div class="dl-why">${esc(d.why)}</div>`
      : `<div class="dl-why missing">no rationale recorded</div>`;
    return `<div class="dl-item ${cls}">
      <div class="dl-meta"><span class="dl-role">${esc(d.role || 'agent')}</span>
        <span>${esc(d.boundary)}</span><span>${esc(time(d.ts))}</span></div>
      ${why}
      ${d.what && d.what.trim() ? `<div class="dl-what">${esc(d.what)}</div>` : ''}
      ${d.alternatives && d.alternatives.trim()
        ? `<div class="dl-alt">rejected: ${esc(d.alternatives)}</div>` : ''}
    </div>`;
  }

  function render(body, brief) {
    const detail = (brief.detail || []).slice().reverse();
    if (!detail.length) {
      body.innerHTML = `<div class="dl-empty">Nothing decided here yet. Agents write
        a line when they reach a boundary; this fills up as they work.</div>`;
      return;
    }
    const open = brief.open || [];
    body.innerHTML = `
      <div class="dl-section">
        <div class="dl-label">Latest, one per agent</div>
        ${(brief.headline || []).map(item).join('')}
      </div>
      <div class="dl-section">
        <div class="dl-label">Open (${open.length})</div>
        ${open.length ? open.map(item).join('')
          : '<div class="dl-empty">Nothing unresolved.</div>'}
      </div>
      <details class="dl-section dl-detail">
        <summary>Full log (${detail.length})</summary>
        ${detail.map(item).join('')}
      </details>`;
  }

  // The host calls destroy() with NO arguments (right-pane.js destroyHost), so
  // cleanup cannot be hung off the container and read back from a parameter —
  // that leaks the interval silently, which is what buildlog does today.
  let timer = null;
  let setRootImpl = null;

  function mount(container, root) {
    styleOnce();
    container.innerHTML = `<div class="dl-wrap">
      <div class="dl-head"><span class="dl-project"></span><span class="dl-note"></span></div>
      <div class="dl-body"></div>
    </div>`;
    const head = container.querySelector('.dl-project');
    const note = container.querySelector('.dl-note');
    const body = container.querySelector('.dl-body');
    let project = projectOf(root);

    async function refresh() {
      head.textContent = project || 'no project';
      if (!project) {
        body.innerHTML = '<div class="dl-empty">No project root selected.</div>';
        return;
      }
      let brief;
      try {
        brief = await invoke('decision_log_brief', { project });
      } catch (e) {
        body.innerHTML = `<div class="dl-empty">Could not read the log: ${esc(e)}</div>`;
        return;
      }
      const n = brief.unexplained || 0;
      note.textContent = n ? `${n} without a reason` : '';
      note.className = n ? 'dl-note warn' : 'dl-note';
      render(body, brief);
    }

    refresh();
    // ponytail: poll, no push. Boundaries are rare by construction (a handful an
    // hour), so an event channel would be more moving parts than the thing is
    // worth. Push if this ever needs to feel live.
    clearInterval(timer);
    timer = setInterval(refresh, 5000);
    setRootImpl = (next) => { project = projectOf(next); refresh(); };
  }

  const view = {
    mount,
    setRoot(root) { if (setRootImpl) setRootImpl(root); },
    destroy() { clearInterval(timer); timer = null; setRootImpl = null; },
  };

  window.xnautViews = window.xnautViews || {};
  window.xnautViews.decisions = view;
  if (window.xnautRightPaneRegisterView) window.xnautRightPaneRegisterView('decisions', view);
  else (window.__xnautRightPaneQueue = window.__xnautRightPaneQueue || []).push({ key: 'decisions', view });
})();
