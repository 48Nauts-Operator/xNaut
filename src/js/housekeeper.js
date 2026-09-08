// Housekeeper panel (XNAUT-264) - the disk the agent worktrees fill.
//
// Sits inside the existing Worktrees modal rather than in a panel of its own:
// that is where a person already comes to think about worktrees, and a
// housekeeper in a fifth place is one more thing nobody opens.
//
// Two rules the rendering has to keep, because the Rust side keeps them:
//   - Every row shows, offered or kept, with the reason. A silent skip is the
//     failure this project keeps paying for, so there is no filter here that
//     hides a row.
//   - The Reclaim button carries an IDENTITY, never a permission. The backend
//     recomputes the verdict on apply, so a button drawn thirty seconds ago
//     cannot remove a checkout an agent started writing in since.
(function () {
  'use strict';

  const invoke = (...a) => window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke(...a);
  const listen = () => (window.__TAURI__ && window.__TAURI__.event && window.__TAURI__.event.listen);
  const $ = (id) => document.getElementById(id);

  function escapeText(s) {
    return String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
  }

  function gb(n) {
    const g = Number(n) / (1024 ** 3);
    if (g >= 1) return g.toFixed(1) + ' GB';
    return Math.round(Number(n) / (1024 ** 2)) + ' MB';
  }

  function injectStyles() {
    if ($('housekeeper-styles')) return;
    const st = document.createElement('style');
    st.id = 'housekeeper-styles';
    st.textContent = `
.hk-volume { display:flex; align-items:center; gap:8px; padding:6px 0 10px; font-size:12px; }
.hk-bar { flex:0 0 120px; height:5px; border-radius:3px; background:var(--bg-tertiary,#2a2a2f); overflow:hidden; }
.hk-bar span { display:block; height:100%; border-radius:3px; }
.hk-total { margin-left:auto; opacity:.75; font-variant-numeric:tabular-nums; }
.hk-row { display:flex; align-items:baseline; gap:8px; padding:5px 0; border-top:1px solid var(--border,rgba(255,255,255,.07)); font-size:12px; }
.hk-size { flex:0 0 68px; text-align:right; font-variant-numeric:tabular-nums; opacity:.9; }
.hk-what { flex:1 1 auto; min-width:0; }
.hk-name { display:block; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.hk-why { display:block; font-size:11px; opacity:.65; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.hk-row[data-offered="0"] .hk-name { opacity:.65; }
.hk-tag { font-size:10px; text-transform:uppercase; letter-spacing:.05em; opacity:.5; }
.hk-act { flex:0 0 auto; background:transparent; border:1px solid var(--border,rgba(255,255,255,.18)); border-radius:6px;
  color:inherit; font:inherit; font-size:11px; padding:2px 9px; cursor:pointer; }
.hk-act:hover { border-color:#f2555a; color:#f2555a; }
.hk-act:disabled { opacity:.4; cursor:default; }
.hk-kept-note { flex:0 0 auto; font-size:11px; opacity:.5; }
`;
    document.head.appendChild(st);
  }

  // The field the manager already filled in, then the open project.
  //
  // `xnautActiveProjectPath` is a FUNCTION, not a value. Calling it as a value
  // yields a truthy function object that then gets used as a path, with no
  // error anywhere; that exact shape has bitten this codebase twice. Hence the
  // typeof, matching worktree.js's own resolution rather than inventing a
  // second one.
  function repoPath() {
    const input = $('worktree-repo-path');
    const typed = input && input.value.trim();
    if (typed) return typed;
    return (typeof window.xnautActiveProjectPath === 'function'
      && window.xnautActiveProjectPath()) || '';
  }

  function fillColor(pct) {
    return pct >= 95 ? '#f2555a' : pct >= 90 ? '#e0902e' : pct >= 80 ? '#f5b840' : '#5aa469';
  }

  function volumeLine(volume) {
    if (!volume) {
      // An unknown disk and an empty one must not read the same.
      return '<div class="hk-volume">Disk usage could not be read on this platform.</div>';
    }
    const pct = Math.max(0, Math.min(100, volume.used_pct));
    return `<div class="hk-volume">
      <span class="hk-bar"><span style="width:${pct}%;background:${fillColor(pct)}"></span></span>
      <span>${pct}% full</span>
      <span class="hk-total">${gb(volume.free)} free of ${gb(volume.total)}</span>
    </div>`;
  }

  function rowHtml(item, index) {
    const label = item.kind === 'build_cache' ? 'build cache' : 'worktree';
    const name = item.branch || item.path;
    const action = item.offered
      ? `<button class="hk-act" data-i="${index}">Reclaim</button>`
      : '<span class="hk-kept-note">kept</span>';
    return `<div class="hk-row" data-offered="${item.offered ? 1 : 0}" title="${escapeText(item.path)}">
      <span class="hk-size">${gb(item.bytes)}</span>
      <span class="hk-what">
        <span class="hk-name"><span class="hk-tag">${label}</span> ${escapeText(name)}</span>
        <span class="hk-why">${escapeText(item.reason)}</span>
      </span>
      ${action}
    </div>`;
  }

  let current = null;

  function render(report, note) {
    const host = $('housekeeper-report');
    if (!host) return;
    current = report;
    if (!report) {
      host.innerHTML = `<div class="cap-meta">${escapeText(note || '')}</div>`;
      return;
    }
    // Offered first and biggest first, so the row worth pressing is at the top;
    // the kept rows follow rather than disappearing.
    const items = report.items
      .map((item, i) => ({ item, i }))
      .sort((a, b) => (b.item.offered - a.item.offered) || (b.item.bytes - a.item.bytes));
    const mainline = report.mainline
      ? `merged means: already in <strong>${escapeText(report.mainline)}</strong>`
      : 'no mainline ref resolved, so nothing counts as merged';
    host.innerHTML = volumeLine(report.volume)
      + `<div class="cap-meta">${gb(report.offered_bytes)} can be reclaimed; `
      + `${gb(report.kept_bytes)} is kept. ${mainline}. Removing a worktree never deletes its branch.</div>`
      + items.map(({ item, i }) => rowHtml(item, i)).join('')
      + (note ? `<div class="cap-meta">${escapeText(note)}</div>` : '');
    host.querySelectorAll('.hk-act').forEach((btn) => {
      btn.onclick = () => reclaim(report.items[Number(btn.dataset.i)], btn);
    });
  }

  async function scan(note) {
    const repo = repoPath();
    if (!repo) { render(null, 'No repo path'); return; }
    render(null, 'Measuring…');
    try {
      // Slow on purpose: it walks every build cache. Cheap enough on demand,
      // which is why the 60s tick reads only the volume.
      const report = await invoke('housekeeper_report', { repoPath: repo, mainline: null });
      render(report, note);
    } catch (e) {
      render(null, String(e));
    }
  }

  async function reclaim(item, btn) {
    if (!item) return;
    const what = item.kind === 'build_cache'
      ? `Clear the build cache in ${item.worktree}?\n\nThe next build regenerates it.`
      : `Remove the worktree ${item.branch || item.path}?\n\nThe branch stays; only the directory goes.`;
    if (!confirm(`${what}\n\nReclaims ${gb(item.bytes)}.`)) return;
    btn.disabled = true;
    btn.textContent = '…';
    try {
      const done = await invoke('housekeeper_reclaim', {
        repoPath: repoPath(),
        path: item.path,
        kind: item.kind,
        mainline: current && current.mainline ? current.mainline : null,
      });
      // Re-scan rather than splice the row out: the backend may have refused
      // something else in the meantime, and a stale list is how a button ends
      // up pointing at a checkout that changed.
      scan(`Reclaimed ${gb(done.bytes)}.`);
    } catch (e) {
      btn.disabled = false;
      btn.textContent = 'Reclaim';
      render(current, String(e));
    }
  }

  function wire() {
    injectStyles();
    const btn = $('btn-housekeeper-scan');
    if (btn) btn.onclick = () => scan();
    // The tick's warning opens straight onto the report, so the owner learns he
    // is at 90% from xNAUT instead of from a failing build.
    const l = listen();
    if (l) {
      // Never open a window on a warning. The pressure event fires on every
      // tick while the disk is over the band, and tron sat at 92% for a day:
      // the worktree manager opened on every start and on every tick after
      // (André, 2026-09-08: "every time you open xNaut on tron the worktrees
      // window opens and nothing is happening"). One notice per band change,
      // in the Mesh inbox where notices live; the manager stays a click away.
      let lastDetail = '';
      l('housekeeper://pressure', (event) => {
        const p = event && event.payload;
        if (!p) return;
        console.warn('[housekeeper]', p.detail);
        const detail = String(p.detail || '');
        if (detail === lastDetail) return;
        lastDetail = detail;
        invoke('inbox_post', { kind: 'notify', req: {
          project: 'xnaut', from: 'housekeeper', level: 'warn',
          title: 'Disk pressure: ' + detail.slice(0, 80),
          body: detail + '\n\nOpen Worktrees to reclaim, or let the hourly pass do it.',
        } }).catch(() => {});
      }).catch(() => {});
    }
  }

  window.xnautHousekeeperScan = scan;

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', wire);
  } else {
    wire();
  }
})();
