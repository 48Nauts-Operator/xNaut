// Right-pane "Build run" view — the Build manager on the right, consistent with
// the other stages' agents. Reads window.xnautSwarm (published by
// multiagent-pane.js) and shows: execution tickets (worktree tasks) progress +
// the ON-GREEN integrator pipeline (merge → commit → PR → promote to Test).
// Opened by the Build stage when a build starts (xnautRightPaneShow('buildrun')).
(function () {
  'use strict';
  const invoke = (...a) => window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke(...a);
  const register = (key, view) => {
    if (window.xnautRightPaneRegisterView) window.xnautRightPaneRegisterView(key, view);
    else (window.__xnautRightPaneQueue = window.__xnautRightPaneQueue || []).push({ key, view });
  };
  const esc = (s) => String(s == null ? '' : s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));

  let styled = false;
  function injectStyles() {
    if (styled) return; styled = true;
    const st = document.createElement('style');
    st.textContent = `
.brun { height:100%; min-height:0; background:var(--bg-secondary,#17191f); }
.brun-head { display:flex; align-items:center; gap:9px; padding:13px 14px; border-bottom:1px solid var(--border,#2a2d34); flex:0 0 auto; }
.brun-glyph { width:26px; height:26px; border-radius:7px; background:#1b2b26; color:#5bd1c9; display:flex; align-items:center; justify-content:center; font:11px/1 "SF Mono",Menlo,monospace; flex-shrink:0; }
.brun-t { display:flex; flex-direction:column; gap:1px; min-width:0; }
.brun-t b { font-size:13px; color:var(--text-primary,#fff); }
.brun-t span { font-size:10px; color:var(--text-secondary,#9a9faa); }
.brun-body { flex:1 1 auto; min-height:0; overflow:auto; padding:14px; }
.brun-sec-h { display:flex; align-items:center; justify-content:space-between; color:var(--text-muted,#7f8590); font-size:10px; font-weight:700; letter-spacing:.06em; text-transform:uppercase; margin-bottom:8px; }
.brun-count { color:var(--text-secondary,#9a9faa); font-family:"SF Mono",Menlo,monospace; }
.brun-bar { height:4px; border-radius:3px; background:var(--bg-tertiary,#22252c); overflow:hidden; margin-bottom:12px; }
.brun-bar span { display:block; height:100%; background:#57b98a; transition:width .3s; }
.brun-list { display:flex; flex-direction:column; gap:9px; }
.brun-tk { display:flex; align-items:center; gap:9px; font-size:12px; }
.brun-dot { width:9px; height:9px; flex:0 0 auto; border-radius:50%; background:#4a4f57; box-shadow:inset 0 0 0 1.5px #33383f; }
.brun-dot.brun-running { background:#f5b840; box-shadow:none; }
.brun-dot.brun-done { background:#57b98a; box-shadow:none; }
.brun-dot.brun-failed { background:#e65a5a; box-shadow:none; }
.brun-tk-n { font:10px/1 "SF Mono",Menlo,monospace; color:var(--text-muted,#7f8590); }
.brun-tk-ti { color:var(--text-primary,#e4e6eb); overflow:hidden; text-overflow:ellipsis; white-space:nowrap; flex:1 1 auto; }
.brun-tk-tag { font-size:9px; letter-spacing:.05em; color:#57b98a; border:1px solid rgba(87,185,138,.4); border-radius:999px; padding:1px 6px; }
.brun-tk-tag.warn { color:#f5b840; border-color:rgba(245,184,64,.4); }
.brun-tk-st { margin:-4px 0 0 18px; padding-left:9px; border-left:1px solid var(--border,#2a2d34); color:var(--text-muted,#7f8590); font:10.5px/1.5 "SF Mono",Menlo,monospace; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.brun-mgr { margin-bottom:14px; padding:8px 10px; border:1px solid var(--border,#2a2d34); border-radius:7px; color:var(--text-secondary,#c9cdd6); font:11px/1.5 "SF Mono",Menlo,monospace; white-space:pre-wrap; word-break:break-word; }
.brun-empty { color:var(--text-muted,#7f8590); font-size:12px; }
.brun-foot { flex:0 0 auto; border-top:1px solid var(--border,#2a2d34); padding:12px 14px; display:flex; flex-direction:column; gap:7px; }
.brun-step { display:flex; align-items:center; gap:8px; color:var(--text-secondary,#9a9faa); font-size:12px; }
.brun-step .g { color:var(--text-muted,#5f646d); }
.brun-step.done { color:#7ec98f; }
.brun-go { margin-top:4px; height:34px; border:0; border-radius:8px; background:var(--xnaut-yellow,#f5b840); color:#171717; font:inherit; font-weight:700; font-size:12.5px; cursor:pointer; }
.brun-go[disabled] { opacity:.4; cursor:default; }
.brun-go2 { height:32px; border:1px solid var(--border,#2a2d34); border-radius:8px; background:transparent; color:var(--text-primary,#e4e6eb); font:inherit; font-weight:600; font-size:12px; cursor:pointer; }
.brun-go2:hover:not([disabled]) { border-color:#5bd1c9; color:#5bd1c9; }
.brun-go2[disabled] { opacity:.4; cursor:default; }`;
    document.head.appendChild(st);
  }

  function createBuildRunView() {
    let container = null, root = null, timer = null, cleanup = null;

    function render() {
      if (!container) return;
      const sw = window.xnautSwarm;
      const q = (sw && sw.queue) || [];
      const active = !!(sw && sw.active);
      const done = q.filter((t) => t.status === 'done').length;
      const proj = (sw && sw.project) || '';
      const started = q.reduce((m, t) => (t.started && (!m || t.started < m)) ? t.started : m, 0);
      const elapsed = started ? Math.max(0, Math.round((Date.now() - started) / 60000)) : 0;
      const allGreen = q.length > 0 && done === q.length;
      const pct = q.length ? Math.round(done / q.length * 100) : 0;
      const rows = q.length
        ? q.map((t, i) => {
          const last = (t.statusLines && t.statusLines.length) ? t.statusLines[t.statusLines.length - 1] : '';
          return `<div class="brun-tk"><span class="brun-dot brun-${esc(t.status)}"></span><span class="brun-tk-n">wt-${i + 1}</span><span class="brun-tk-ti">${esc(t.title || t.id)}</span>${t.pr ? '<span class="brun-tk-tag">PR</span>' : (t.status === 'running' ? '<span class="brun-tk-tag warn">RUNNING</span>' : '')}</div>`
            + (last ? `<div class="brun-tk-st" title="${esc((t.statusLines || []).slice(-6).join('\n'))}">${esc(last)}</div>` : '');
        }).join('')
        : '<div class="brun-empty">No build running. Start one from the Build stage.</div>';
      const step = (label, ok) => `<div class="brun-step${ok ? ' done' : ''}"><span class="g">${ok ? '✓' : '▸'}</span>${esc(label)}</div>`;
      const mgr = (sw && sw.managerStatus) || '';
      container.innerHTML = `
        <div class="brun-head"><div class="brun-glyph">&gt;_</div><div class="brun-t"><b>Build run</b><span>${q.length ? (active ? 'building · ' + elapsed + 'm' : (allGreen ? 'all green' : 'stopped')) : (mgr ? 'manager working' : 'no build running')}${proj ? ' · ' + esc(proj) : ''}</span></div></div>
        <div class="brun-body">
          ${mgr ? `<div class="brun-sec-h"><span>Build manager</span></div><div class="brun-mgr">${esc(mgr)}</div>` : ''}
          <div class="brun-sec-h"><span>Execution tickets</span><span class="brun-count">${done} / ${q.length}</span></div>
          <div class="brun-bar"><span style="width:${pct}%"></span></div>
          <div class="brun-list">${rows}</div>
        </div>
        <div class="brun-foot">
          <div class="brun-sec-h">On green · build agent</div>
          ${step('Merge worktrees', allGreen)}
          ${step('Commit & push branch', allGreen)}
          ${step('Open pull request', q.some((t) => t.pr))}
          <button class="brun-go2" data-consolidate${q.length ? '' : ' disabled'}>⛬ Consolidate → runnable product</button>
          <button class="brun-go" data-promote${allGreen ? '' : ' disabled'}>↑ Promote to Test</button>
        </div>`;
      const go = container.querySelector('[data-promote]');
      if (go) go.onclick = () => { if (window.xnautBuildPromote) window.xnautBuildPromote(); };
      const cons = container.querySelector('[data-consolidate]');
      if (cons) cons.onclick = () => { if (window.xnautBuildConsolidate) { cons.disabled = true; cons.textContent = 'Integrator running…'; window.xnautBuildConsolidate(); } };
    }

    return {
      mount(el, initialRoot) {
        // classList.add, NEVER replace className: the host's .rpane-view class
        // carries the show/hide contract — wiping it locks this view on screen.
        injectStyles(); container = el; root = initialRoot; container.classList.add('brun');
        render();
        const onUpdate = () => render();
        window.addEventListener('xnaut-swarm-update', onUpdate);
        timer = setInterval(render, 3000);
        cleanup = () => { window.removeEventListener('xnaut-swarm-update', onUpdate); if (timer) clearInterval(timer); };
      },
      setRoot(r) { root = r; },
      destroy() { if (cleanup) cleanup(); container = null; },
    };
  }

  register('buildrun', createBuildRunView());
})();
