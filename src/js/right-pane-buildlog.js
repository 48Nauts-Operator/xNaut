// Build log — the viewer for the durable master log written by build_log.rs.
//
// Modelled on Dozzle's ergonomics (level filters with live counts, a source rail,
// substring search, live tail) but over a log we OWN and keep, because Dozzle
// explicitly stores nothing and the requirement here is "every session is written
// to one master log you can always go back to".
//
// It replaces reading the build through `managerSay`, which held ONE string that
// the next event overwrote — the reason a whole night of manager decisions left
// no trace, and three agents could be killed with no record of why.
//
// STYLING NOTE: only CSS variables that are actually defined are used, and every
// one carries a literal fallback. `--hover-bg`, `--input-bg` and `--text-muted`
// appear elsewhere in this codebase but are defined nowhere, so they silently
// resolve to nothing; using them here would produce invisible text on some rows.
(function () {
  'use strict';

  const invoke = (cmd, args) => window.__TAURI__.core.invoke(cmd, args);

  const LEVELS = [
    { key: 'debug', label: 'DEBUG', color: '#6E7681' },
    { key: 'info', label: 'INFO', color: '#7FA6D9' },
    { key: 'warn', label: 'WARN', color: '#E8A33D' },
    { key: 'error', label: 'ERROR', color: '#E0524A' },
  ];
  const COLOR = Object.fromEntries(LEVELS.map((l) => [l.key, l.color]));

  const STYLES = `
.bl-host { display:flex; flex-direction:column; height:100%; min-height:0; font-family:var(--font-sans, sans-serif); }
.bl-head { display:flex; align-items:center; gap:10px; padding:10px 12px; border-bottom:1px solid var(--border, #24262c); flex:0 0 auto; }
.bl-title { font-size:13px; font-weight:600; color:var(--text-primary, #e8e6e1); }
.bl-live { font-family:var(--font-mono, monospace); font-size:10px; font-weight:600; letter-spacing:.06em; padding:2px 6px; border-radius:3px; background:#4FB477; color:#0d0e12; }
.bl-live.off { background:transparent; color:var(--text-secondary, #8a8f98); border:1px solid var(--border, #24262c); }
.bl-pick { margin-left:auto; max-width:210px; font-family:var(--font-mono, monospace); font-size:11px; background:var(--bg-tertiary, #16181d); color:var(--text-secondary, #8a8f98); border:1px solid var(--border, #24262c); border-radius:4px; padding:3px 6px; }
.bl-filters { display:flex; align-items:center; gap:6px; flex-wrap:wrap; padding:8px 12px; border-bottom:1px solid var(--border, #24262c); flex:0 0 auto; }
.bl-pill { display:flex; align-items:center; gap:6px; padding:3px 9px; border-radius:4px; border:1px solid var(--border, #24262c); background:transparent; cursor:pointer; font-family:var(--font-mono, monospace); font-size:10px; letter-spacing:.06em; color:var(--text-secondary, #8a8f98); }
.bl-pill .n { opacity:.75; }
.bl-pill[aria-pressed="true"] { background:var(--bg-tertiary, #1e2129); color:var(--text-primary, #e8e6e1); border-color:var(--text-secondary, #4a505a); }
.bl-dot { width:6px; height:6px; border-radius:3px; flex:0 0 auto; }
.bl-search { flex:1 1 150px; min-width:110px; font-family:var(--font-mono, monospace); font-size:11px; padding:4px 8px; border-radius:4px; border:1px solid var(--border, #24262c); background:var(--bg-tertiary, #16181d); color:var(--text-primary, #e8e6e1); }
.bl-body { display:flex; flex:1 1 auto; min-height:0; }
.bl-rail { width:132px; flex:0 0 auto; overflow-y:auto; border-right:1px solid var(--border, #24262c); padding:8px 0; }
.bl-rail h4 { margin:0 0 6px 10px; font-family:var(--font-mono, monospace); font-size:9px; letter-spacing:.12em; color:var(--text-secondary, #8a8f98); font-weight:500; }
.bl-src { display:flex; align-items:center; gap:6px; width:100%; padding:4px 10px; background:transparent; border:none; border-left:2px solid transparent; cursor:pointer; font-family:var(--font-mono, monospace); font-size:11px; color:var(--text-secondary, #8a8f98); text-align:left; }
.bl-src[aria-pressed="true"] { border-left-color:#E8A33D; color:var(--text-primary, #e8e6e1); background:var(--bg-tertiary, #16181d); }
.bl-src .n { margin-left:auto; opacity:.7; font-size:10px; }
.bl-stream { flex:1 1 auto; min-width:0; overflow:auto; }
.bl-row { display:flex; gap:10px; padding:3px 10px; border-left:2px solid transparent; align-items:baseline; }
.bl-row.warn { border-left-color:#E8A33D; background:rgba(232,163,61,.07); }
.bl-row.error { border-left-color:#E0524A; background:rgba(224,82,74,.09); }
.bl-t { flex:0 0 62px; font-family:var(--font-mono, monospace); font-size:10px; color:var(--text-secondary, #6b7079); font-variant-numeric:tabular-nums; }
.bl-l { flex:0 0 40px; font-family:var(--font-mono, monospace); font-size:9px; font-weight:600; letter-spacing:.06em; }
.bl-s { flex:0 0 62px; font-family:var(--font-mono, monospace); font-size:10px; color:var(--text-secondary, #8a8f98); overflow:hidden; text-overflow:ellipsis; }
.bl-m { flex:1 1 auto; min-width:0; font-family:var(--font-mono, monospace); font-size:11px; color:var(--text-primary, #c6cad2); overflow-wrap:anywhere; }
.bl-foot { display:flex; align-items:center; gap:8px; padding:6px 12px; border-top:1px solid var(--border, #24262c); font-family:var(--font-mono, monospace); font-size:10px; color:var(--text-secondary, #6b7079); flex:0 0 auto; }
.bl-foot .path { overflow:hidden; text-overflow:ellipsis; white-space:nowrap; direction:rtl; }
.bl-empty { padding:22px 14px; font-size:12px; color:var(--text-secondary, #8a8f98); line-height:1.5; }
`;

  function styleOnce() {
    if (document.getElementById('bl-styles')) return;
    const el = document.createElement('style');
    el.id = 'bl-styles';
    el.textContent = STYLES;
    document.head.appendChild(el);
  }

  const hhmmss = (ms) => {
    const d = new Date(ms);
    const p = (n) => String(n).padStart(2, '0');
    return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
  };

  function mount(container) {
    styleOnce();
    container.innerHTML = '';
    const host = document.createElement('div');
    host.className = 'bl-host';
    container.appendChild(host);

    // Off by default: a filter nobody set should never hide anything.
    const state = { buildId: '', levels: new Set(), source: '', query: '', live: true, seen: -1, rows: [] };

    host.innerHTML = `
      <div class="bl-head">
        <span class="bl-title">Build log</span>
        <span class="bl-live">LIVE</span>
        <select class="bl-pick" title="Earlier builds are kept — pick one to read it back"></select>
      </div>
      <div class="bl-filters"></div>
      <div class="bl-body">
        <div class="bl-rail"><h4>SOURCES</h4><div class="bl-srcs"></div></div>
        <div class="bl-stream"></div>
      </div>
      <div class="bl-foot"><span class="count"></span><span class="path"></span></div>`;

    const $ = (sel) => host.querySelector(sel);
    const liveEl = $('.bl-live');
    const pick = $('.bl-pick');
    const filters = $('.bl-filters');
    const srcs = $('.bl-srcs');
    const stream = $('.bl-stream');

    liveEl.onclick = () => {
      state.live = !state.live;
      liveEl.textContent = state.live ? 'LIVE' : 'PAUSED';
      liveEl.classList.toggle('off', !state.live);
      if (state.live) refresh(true);
    };
    liveEl.style.cursor = 'pointer';

    // ---- filter bar -------------------------------------------------------
    const pills = {};
    LEVELS.forEach((l) => {
      const b = document.createElement('button');
      b.className = 'bl-pill';
      b.type = 'button';
      b.setAttribute('aria-pressed', 'false');
      b.innerHTML = `<span class="bl-dot" style="background:${l.color}"></span>${l.label} <span class="n">0</span>`;
      b.onclick = () => {
        if (state.levels.has(l.key)) state.levels.delete(l.key); else state.levels.add(l.key);
        b.setAttribute('aria-pressed', String(state.levels.has(l.key)));
        refresh(true);
      };
      pills[l.key] = b;
      filters.appendChild(b);
    });
    const search = document.createElement('input');
    search.className = 'bl-search';
    search.placeholder = 'filter…  (source or message)';
    let debounce = 0;
    search.oninput = () => {
      clearTimeout(debounce);
      debounce = setTimeout(() => { state.query = search.value.trim(); refresh(true); }, 180);
    };
    filters.appendChild(search);

    // ---- data -------------------------------------------------------------
    async function loadBuilds() {
      let list = [];
      try { list = (await invoke('build_log_list')) || []; } catch (_) { list = []; }
      const active = (window.xnautSwarm && window.xnautSwarm.buildId) || '';
      if (active && !list.some((b) => b.build_id === active)) {
        list.unshift({ build_id: active, path: '', bytes: 0, modified_ms: Date.now() });
      }
      pick.innerHTML = list
        .map((b) => `<option value="${b.build_id}">${b.build_id}</option>`)
        .join('') || '<option value="">no builds yet</option>';
      if (!state.buildId) state.buildId = active || (list[0] && list[0].build_id) || '';
      pick.value = state.buildId;
      return list;
    }
    pick.onchange = () => { state.buildId = pick.value; state.seen = -1; state.rows = []; refresh(true); };

    function renderRows(reset) {
      if (reset) stream.innerHTML = '';
      const atBottom = stream.scrollTop + stream.clientHeight >= stream.scrollHeight - 40;
      const frag = document.createDocumentFragment();
      for (const e of state.rows) {
        const row = document.createElement('div');
        row.className = 'bl-row ' + (e.level === 'warn' || e.level === 'error' ? e.level : '');
        const lvl = document.createElement('span');
        lvl.className = 'bl-l';
        lvl.style.color = COLOR[e.level] || COLOR.info;
        lvl.textContent = (e.level || 'info').toUpperCase();
        const t = document.createElement('span'); t.className = 'bl-t'; t.textContent = hhmmss(e.t);
        const s = document.createElement('span'); s.className = 'bl-s'; s.textContent = e.source; s.title = e.source;
        const m = document.createElement('span'); m.className = 'bl-m'; m.textContent = e.event;
        row.append(t, lvl, s, m);
        frag.appendChild(row);
      }
      state.rows = [];
      stream.appendChild(frag);
      // Only auto-scroll if the reader was already at the live edge — yanking the
      // view while someone is reading scrollback is the classic log-viewer sin.
      if (reset || atBottom) stream.scrollTop = stream.scrollHeight;
    }

    async function refresh(reset) {
      if (!state.buildId) {
        stream.innerHTML = '<div class="bl-empty">No build log yet. Start a build — every event is written to '
          + '<code>~/.config/xnaut/looms/logs/&lt;build&gt;.jsonl</code> and kept.</div>';
        return;
      }
      let page = null;
      try {
        page = await invoke('build_log_read', {
          buildId: state.buildId,
          levels: state.levels.size ? [...state.levels] : null,
          source: state.source || null,
          query: state.query || null,
          afterSeq: reset ? null : (state.seen >= 0 ? state.seen : null),
          limit: 400,
        });
      } catch (_) { return; }
      if (!page) return;

      const c = page.counts || {};
      LEVELS.forEach((l) => { pills[l.key].querySelector('.n').textContent = c[l.key] || 0; });

      // The rail lists every source in the FILE, so a slice stays visible even
      // when the current filter hides all of its lines.
      const cur = state.source;
      srcs.innerHTML = '';
      const all = document.createElement('button');
      all.className = 'bl-src'; all.type = 'button';
      all.setAttribute('aria-pressed', String(!cur));
      all.innerHTML = `all <span class="n">${c.total || 0}</span>`;
      all.onclick = () => { state.source = ''; refresh(true); };
      srcs.appendChild(all);
      (page.sources || []).forEach((sname) => {
        const b = document.createElement('button');
        b.className = 'bl-src'; b.type = 'button';
        b.setAttribute('aria-pressed', String(cur === sname));
        b.textContent = sname;
        b.onclick = () => { state.source = state.source === sname ? '' : sname; refresh(true); };
        srcs.appendChild(b);
      });

      if (reset) state.seen = -1;
      const fresh = (page.events || []).filter((e) => e.seq > state.seen);
      if (fresh.length) state.seen = fresh[fresh.length - 1].seq;
      state.rows = fresh;
      if (reset || fresh.length) renderRows(reset);
      if (reset && !(page.events || []).length) {
        stream.innerHTML = '<div class="bl-empty">Nothing matches this filter. '
          + (c.total ? `The log holds ${c.total} events.` : 'The log is empty.') + '</div>';
      }

      host.querySelector('.count').textContent =
        `${c.total || 0} events · ${c.warn || 0} warn · ${c.error || 0} error`;
      host.querySelector('.path').textContent = page.path || '';
      host.querySelector('.path').title = page.path || '';
    }

    let timer = 0;
    (async () => { await loadBuilds(); await refresh(true); })();
    timer = setInterval(() => {
      if (!state.live) return;
      const active = (window.xnautSwarm && window.xnautSwarm.buildId) || '';
      if (active && active !== state.buildId) { state.buildId = active; state.seen = -1; loadBuilds(); refresh(true); return; }
      refresh(false);
    }, 2000);

    container.__blCleanup = () => clearInterval(timer);
  }

  const view = {
    mount,
    setRoot() {},
    destroy(container) { if (container && container.__blCleanup) container.__blCleanup(); },
  };
  if (window.xnautRightPaneRegisterView) window.xnautRightPaneRegisterView('buildlog', view);
  else (window.__xnautRightPaneQueue = window.__xnautRightPaneQueue || []).push({ key: 'buildlog', view });
})();
