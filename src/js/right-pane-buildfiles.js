// Build files — what each slice actually changed, and the diff.
//
// The Cursor-shaped half of the redesign: you should be able to read the code an
// agent is writing without switching to its terminal and scrolling.
//
// Everything is measured from the MERGE BASE, never the working tree. On the
// 2026-08-09 Guardian run `git status` in the engine-chains worktree showed three
// untracked files and nothing else, because the agent had already committed
// twelve times — a working-tree view would have shown an agent doing nothing
// while it had written 473 lines. slice_changes does the right thing; this is
// just the surface for it.
(function () {
  'use strict';

  const invoke = (cmd, args) => window.__TAURI__.core.invoke(cmd, args);

  const STYLES = `
.bf-host { display:flex; flex-direction:column; height:100%; min-height:0; font-family:var(--font-sans, sans-serif); }
.bf-head { display:flex; align-items:center; gap:8px; padding:10px 12px; border-bottom:1px solid var(--border, #24262c); flex:0 0 auto; }
.bf-title { font-size:13px; font-weight:600; color:var(--text-primary, #e8e6e1); }
.bf-pick { margin-left:auto; max-width:190px; font-family:var(--font-mono, monospace); font-size:11px; background:var(--bg-tertiary, #16181d); color:var(--text-secondary, #8a8f98); border:1px solid var(--border, #24262c); border-radius:4px; padding:3px 6px; }
.bf-meta { display:flex; flex-wrap:wrap; align-items:baseline; gap:8px; padding:8px 12px; border-bottom:1px solid var(--border, #24262c); font-family:var(--font-mono, monospace); font-size:10px; color:var(--text-secondary, #8a8f98); flex:0 0 auto; }
.bf-meta .branch { color:var(--text-primary, #c6cad2); font-size:11px; }
.bf-add { color:#4FB477; } .bf-del { color:#E0524A; }
.bf-list { flex:1 1 auto; min-height:0; overflow:auto; }
.bf-file { display:flex; align-items:center; gap:8px; width:100%; padding:6px 12px; background:transparent; border:none; border-bottom:1px solid var(--border, #1c1f26); cursor:pointer; text-align:left; font-family:var(--font-mono, monospace); font-size:11px; color:var(--text-primary, #c6cad2); }
.bf-file .ext { flex:0 0 22px; font-size:9px; color:var(--text-secondary, #8a8f98); }
.bf-file .p { flex:1 1 auto; min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; direction:rtl; text-align:left; }
.bf-file .n { flex:0 0 auto; font-size:10px; font-variant-numeric:tabular-nums; }
.bf-file .new { flex:0 0 auto; font-size:9px; color:#4FB477; }
.bf-diff { border-bottom:1px solid var(--border, #1c1f26); background:var(--editor-surface, #0f1116); overflow-x:auto; }
.bf-line { display:flex; gap:10px; padding:0 12px; font-family:var(--font-mono, monospace); font-size:10.5px; line-height:1.55; white-space:pre; }
.bf-line .ln { flex:0 0 30px; text-align:right; color:#454b55; font-variant-numeric:tabular-nums; }
.bf-line.add { background:rgba(79,180,119,.10); } .bf-line.add .tx { color:#a8d5b8; }
.bf-line.del { background:rgba(224,82,74,.10); } .bf-line.del .tx { color:#e0a8a2; }
.bf-line.hunk .tx { color:#7FA6D9; } .bf-line.ctx .tx { color:var(--text-secondary, #8a8f98); }
.bf-more { padding:8px 12px; font-size:11px; color:#7FA6D9; cursor:pointer; text-align:center; }
.bf-empty { padding:22px 14px; font-size:12px; color:var(--text-secondary, #8a8f98); line-height:1.5; }
`;

  function styleOnce() {
    if (document.getElementById('bf-styles')) return;
    const el = document.createElement('style');
    el.id = 'bf-styles';
    el.textContent = STYLES;
    document.head.appendChild(el);
  }

  const ext = (p) => {
    const e = (p.split('.').pop() || '').toLowerCase();
    return ({ mjs: 'js', cjs: 'js', jsx: 'js', ts: 'ts', tsx: 'ts', py: 'py', rs: 'rs', yaml: 'yml', yml: 'yml', json: '{}', md: 'md', html: '<>', css: 'css' })[e] || e.slice(0, 3);
  };

  function mount(container) {
    styleOnce();
    container.innerHTML = '';
    const host = document.createElement('div');
    host.className = 'bf-host';
    host.innerHTML = `
      <div class="bf-head"><span class="bf-title">Files</span><select class="bf-pick"></select></div>
      <div class="bf-meta"></div>
      <div class="bf-list"></div>`;
    container.appendChild(host);

    const pick = host.querySelector('.bf-pick');
    const meta = host.querySelector('.bf-meta');
    const list = host.querySelector('.bf-list');
    const state = { wt: '', open: new Set() };

    function slices() {
      const q = (window.xnautBuild && window.xnautBuild.queue) || [];
      return q.filter((w) => w && w.wt);
    }

    function renderPicker() {
      const s = slices();
      pick.innerHTML = s.map((w) => `<option value="${w.wt}">${(w.title || w.id || w.wt).slice(0, 34)}</option>`).join('')
        || '<option value="">no slices</option>';
      if (!state.wt || !s.some((w) => w.wt === state.wt)) state.wt = (s[0] && s[0].wt) || '';
      pick.value = state.wt;
      return s;
    }
    pick.onchange = () => { state.wt = pick.value; state.open.clear(); refresh(); };

    async function toggleDiff(path, row) {
      const existing = row.nextElementSibling;
      if (existing && existing.classList.contains('bf-diff')) { existing.remove(); state.open.delete(path); return; }
      state.open.add(path);
      const box = document.createElement('div');
      box.className = 'bf-diff';
      box.innerHTML = '<div class="bf-empty">loading…</div>';
      row.after(box);
      let d = null;
      try { d = await invoke('slice_file_diff', { worktree: state.wt, path, baseRef: null, maxLines: 600 }); } catch (e) {
        box.innerHTML = `<div class="bf-empty">could not diff this file — ${String((e && e.message) || e)}</div>`;
        return;
      }
      if (!d || !d.lines || !d.lines.length) { box.innerHTML = '<div class="bf-empty">no textual changes</div>'; return; }
      box.innerHTML = '';
      const frag = document.createDocumentFragment();
      for (const l of d.lines) {
        const el = document.createElement('div');
        el.className = 'bf-line ' + l.kind;
        const ln = document.createElement('span'); ln.className = 'ln'; ln.textContent = l.n ? String(l.n) : '';
        const tx = document.createElement('span'); tx.className = 'tx';
        tx.textContent = (l.kind === 'add' ? '+ ' : l.kind === 'del' ? '− ' : l.kind === 'hunk' ? '' : '  ') + l.text;
        el.append(ln, tx);
        frag.appendChild(el);
      }
      box.appendChild(frag);
      if (d.truncated) {
        const more = document.createElement('div');
        more.className = 'bf-more';
        more.textContent = `Large diff — showing the first 600 lines of +${d.added}/−${d.removed}`;
        box.appendChild(more);
      }
    }

    async function refresh() {
      const s = renderPicker();
      if (!s.length || !state.wt) {
        meta.textContent = '';
        list.innerHTML = '<div class="bf-empty">No build running. Start one and each slice’s changes appear here, '
          + 'measured against the commit it forked from.</div>';
        return;
      }
      let ch = null;
      try { ch = await invoke('slice_changes', { worktree: state.wt, baseRef: null }); } catch (e) {
        list.innerHTML = `<div class="bf-empty">${String((e && e.message) || e)}</div>`;
        return;
      }
      meta.innerHTML = `<span class="branch">${ch.branch || '(detached)'}</span>`
        + `<span>base ${String(ch.base || '').slice(0, 7)} · ${ch.base_ref || ''}</span>`
        + `<span>${ch.files.length} file${ch.files.length === 1 ? '' : 's'}</span>`
        + `<span class="bf-add">+${ch.added}</span><span class="bf-del">−${ch.removed}</span>`
        + `<span>${ch.commits} commit${ch.commits === 1 ? '' : 's'}</span>`;

      if (!ch.files.length) {
        list.innerHTML = '<div class="bf-empty">Nothing changed yet against '
          + `<code>${String(ch.base || '').slice(0, 7)}</code>.</div>`;
        return;
      }
      list.innerHTML = '';
      for (const f of ch.files) {
        const row = document.createElement('button');
        row.className = 'bf-file';
        row.type = 'button';
        row.innerHTML = `<span class="ext">${ext(f.path)}</span>`
          + `<span class="p" title="${f.path}">${f.path}</span>`
          + (f.status === 'new' ? '<span class="new">New</span>' : '')
          + `<span class="n bf-add">+${f.added}</span>`
          + `<span class="n bf-del">${f.removed ? '−' + f.removed : ''}</span>`;
        row.onclick = () => toggleDiff(f.path, row);
        list.appendChild(row);
        if (state.open.has(f.path)) toggleDiff(f.path, row);
      }
    }

    refresh();
    const onSwarm = () => renderPicker();
    window.addEventListener('xnaut-build-update', onSwarm);
    // Slow on purpose: this shells out to git several times, and a slice's diff
    // does not change fast enough to justify paying for it every second.
    const timer = setInterval(refresh, 15000);
    container.__bfCleanup = () => { clearInterval(timer); window.removeEventListener('xnaut-build-update', onSwarm); };
  }

  const view = {
    mount,
    setRoot() {},
    destroy(container) { if (container && container.__bfCleanup) container.__bfCleanup(); },
  };
  // Also exposed so the Build run pane can host it as a sub-tab: these are
  // only meaningful inside a build, so they do not deserve a global icon.
  window.xnautViews = window.xnautViews || {};
  window.xnautViews.buildfiles = view;
  if (window.xnautRightPaneRegisterView) window.xnautRightPaneRegisterView('buildfiles', view);
  else (window.__xnautRightPaneQueue = window.__xnautRightPaneQueue || []).push({ key: 'buildfiles', view });
})();
