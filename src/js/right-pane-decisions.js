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

  // Chrome is monochrome; colour means state and nothing else. Amber is "open",
  // red is "a check failed". Nothing decorative gets a hue, so an amber edge in
  // the corner of your eye always means the same thing.
  const STYLES = `
  .dl-wrap { display: flex; flex-direction: column; height: 100%; overflow: hidden;
    font-family: var(--font-sans, sans-serif); color: var(--text-primary, #e8e6e1); }
  .dl-head { display: flex; align-items: baseline; gap: 8px; padding: 10px 14px;
    border-bottom: 1px solid var(--border, #24262c); flex: 0 0 auto; }
  .dl-project { font-size: 12px; font-weight: 600; letter-spacing: 0.01em; }
  .dl-note { font-size: 10px; color: var(--text-secondary, #8a8f98); margin-left: auto; }
  .dl-note.warn { color: #E8A33D; }
  .dl-body { flex: 1 1 auto; overflow-y: auto; padding: 14px 14px 28px; }

  .dl-section { margin-bottom: 22px; }
  .dl-section:last-child { margin-bottom: 0; }
  /* Label + hairline: the rule carries the eye across and gives each group a
     top edge, which is most of what stops a flat list reading as one blob. */
  .dl-label { display: flex; align-items: center; gap: 8px; margin-bottom: 10px;
    font-size: 9px; letter-spacing: 0.11em; text-transform: uppercase; font-weight: 600;
    color: var(--text-secondary, #8a8f98); }
  .dl-label::after { content: ''; flex: 1 1 auto; height: 1px;
    background: var(--border, #24262c); }
  .dl-label.warn { color: #E8A33D; }
  .dl-label.warn::after { background: rgba(232,163,61,0.28); }

  .dl-item { position: relative; padding: 6px 8px 8px 11px; margin: 0 -8px 2px;
    border-radius: 4px; border-left: 2px solid var(--border, #24262c); }
  .dl-item:hover { background: rgba(255,255,255,0.028); }
  .dl-item.open { border-left-color: #E8A33D; }
  .dl-item.fail { border-left-color: #E0524A; }
  .dl-meta { display: flex; align-items: baseline; gap: 7px; margin-bottom: 3px;
    font-size: 10px; font-family: var(--font-mono, monospace);
    color: var(--text-secondary, #8a8f98); }
  .dl-role { color: var(--text-primary, #e8e6e1); font-weight: 600; }
  .dl-boundary { opacity: 0.85; }
  .dl-time { margin-left: auto; opacity: 0.6; font-variant-numeric: tabular-nums; }
  .dl-why { font-size: 12px; line-height: 1.5; }
  .dl-why.missing { color: #E8A33D; font-style: italic; opacity: 0.85; }
  .dl-what, .dl-alt { font-size: 11px; line-height: 1.45; margin-top: 3px;
    color: var(--text-secondary, #8a8f98); }
  .dl-alt::before { content: 'rejected '; opacity: 0.65; }
  .dl-empty { font-size: 12px; line-height: 1.5; color: var(--text-secondary, #8a8f98);
    padding: 10px 2px; }
  .dl-body > .dl-empty { padding: 32px 8px; max-width: 34ch; }

  .dl-detail > summary { display: flex; align-items: center; gap: 8px; cursor: pointer;
    margin-bottom: 10px; font-size: 9px; letter-spacing: 0.11em; text-transform: uppercase;
    font-weight: 600; color: var(--text-secondary, #8a8f98); list-style: none; }
  .dl-detail > summary::-webkit-details-marker { display: none; }
  .dl-detail > summary::before { content: '\\203A'; font-size: 13px; line-height: 1;
    transform: translateY(-1px); transition: transform 120ms ease; }
  .dl-detail[open] > summary::before { transform: translateY(-1px) rotate(90deg); }
  .dl-detail > summary::after { content: ''; flex: 1 1 auto; height: 1px;
    background: var(--border, #24262c); }
  .dl-detail > summary:hover { color: var(--text-primary, #e8e6e1); }

  /* The summary is model prose over a mechanical list, so it is set apart rather
     than blended in: you should always be able to see where the writing stops
     and the record starts. */
  .dl-sum { padding: 11px 13px 9px; margin-bottom: 22px; border-radius: 5px;
    background: rgba(255,255,255,0.035); border: 1px solid var(--border, #24262c); }
  .dl-sum-label { display: block; margin-bottom: 7px; font-size: 9px; font-weight: 600;
    letter-spacing: 0.11em; text-transform: uppercase;
    color: var(--text-secondary, #8a8f98); opacity: 0.75; }
  /* Clamped by default. Five sentences of prose is taller than the open items
     underneath it, and burying the record under the summary of the record is
     exactly the wrong way round. */
  .dl-sum-text { font-size: 12px; line-height: 1.6; white-space: pre-wrap;
    max-height: 9.6em; overflow: hidden; }
  .dl-sum.expanded .dl-sum-text { max-height: none; }
  .dl-sum-text.pending { color: var(--text-secondary, #8a8f98); font-style: italic; }
  .dl-sum-foot { display: flex; align-items: center; gap: 8px; margin-top: 9px;
    padding-top: 8px; border-top: 1px solid var(--border, #24262c);
    font-size: 10px; color: var(--text-secondary, #8a8f98); opacity: 0.8; }
  /* Native button background is a pale box on this theme; reset it explicitly. */
  .dl-redo, .dl-more { background: none; border: 1px solid var(--border, #24262c);
    border-radius: 4px; color: var(--text-secondary, #8a8f98); font: inherit;
    font-size: 10px; padding: 2px 8px; cursor: pointer; }
  .dl-more { margin-left: auto; }
  .dl-redo:hover, .dl-more:hover { color: var(--text-primary, #e8e6e1);
    border-color: var(--text-secondary, #8a8f98); }
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
        <span class="dl-boundary">${esc(d.boundary)}</span>
        <span class="dl-time">${esc(time(d.ts))}</span></div>
      ${why}
      ${d.what && d.what.trim() ? `<div class="dl-what">${esc(d.what)}</div>` : ''}
      ${d.alternatives && d.alternatives.trim()
        ? `<div class="dl-alt">${esc(d.alternatives)}</div>` : ''}
    </div>`;
  }

  // The summariser's output, rendered ABOVE the mechanical sections and never
  // in place of them. The prose can be wrong; the list underneath it cannot,
  // and showing both is what makes a bad summary visible instead of load-bearing.
  function summaryBlock(brief, running, expanded) {
    const s = brief.summary;
    const total = (brief.detail || []).length;
    if (!s && !running) return '';
    if (!s || !s.text) {
      const msg = s && s.error
        ? `No summary: ${esc(s.error)}`
        : 'Summarising the log...';
      return `<div class="dl-sum"><span class="dl-sum-label">Summary</span>
        <div class="dl-sum-text pending">${msg}</div></div>`;
    }
    const behind = total - (s.n || 0);
    const foot = running
      ? 'rewriting...'
      : (behind > 0 ? `${behind} newer ${behind === 1 ? 'entry' : 'entries'} not yet in this`
                    : `from all ${total}`);
    return `<div class="dl-sum${expanded ? ' expanded' : ''}">
      <span class="dl-sum-label">Summary</span>
      <div class="dl-sum-text">${esc(s.text)}</div>
      <div class="dl-sum-foot"><span>${esc(foot)}</span>
        <button class="dl-more" type="button">${expanded ? 'less' : 'more'}</button>
        <button class="dl-redo" type="button">rewrite</button></div>
    </div>`;
  }

  function render(body, brief, running, expanded) {
    const detail = (brief.detail || []).slice().reverse();
    if (!detail.length) {
      body.innerHTML = `<div class="dl-empty">Nothing decided here yet. Agents write
        a line when they reach a boundary; this fills up as they work.</div>`;
      return;
    }
    const open = brief.open || [];
    body.innerHTML = `
      ${summaryBlock(brief, running, expanded)}
      <div class="dl-section">
        <div class="dl-label">Latest, one per agent</div>
        ${(brief.headline || []).map(item).join('')}
      </div>
      <div class="dl-section">
        <div class="dl-label${open.length ? ' warn' : ''}">Open (${open.length})</div>
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
    // One model call at a time. The backend decides WHETHER to rewrite (a
    // count, see is_stale); this only stops the 5s poll from stacking a second
    // request on top of one already in flight.
    let running = false;
    // The log length we last asked about. Without it, summarize() -> refresh()
    // -> summarize() is a loop: the backend answers "not stale, here is the
    // cached prose", the poll cannot tell that from a fresh write, and asks
    // again forever. The log is append-only, so its length is a safe identity.
    let asked = -1;
    // Survives the 5s poll: without it, expanding the summary collapses again a
    // few seconds later because refresh() rewrites the block.
    let expanded = false;

    // ponytail: asked once per new log length, not on a timer. The backend
    // decides whether that actually warrants a rewrite (is_stale, three
    // entries), so the rule stays in one place, in Rust, where it is tested.
    async function summarize(force) {
      if (running) return;
      running = true;
      try {
        await invoke('decision_log_summarize', { project, force: !!force });
      } catch (_) {
        // Fail quiet: the mechanical brief below is the part that must render.
      } finally {
        running = false;
      }
      refresh();
    }

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
      render(body, brief, running, expanded);
      const redo = body.querySelector('.dl-redo');
      if (redo) redo.onclick = () => { asked = -1; summarize(true); };
      const more = body.querySelector('.dl-more');
      if (more) more.onclick = () => { expanded = !expanded; refresh(); };
      const n_all = (brief.detail || []).length;
      if (!running && n_all && asked !== n_all) {
        asked = n_all;
        summarize(false);
      }
    }

    refresh();
    // ponytail: poll, no push. Boundaries are rare by construction (a handful an
    // hour), so an event channel would be more moving parts than the thing is
    // worth. Push if this ever needs to feel live.
    clearInterval(timer);
    timer = setInterval(refresh, 5000);
    setRootImpl = (next) => { project = projectOf(next); asked = -1; refresh(); };
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
