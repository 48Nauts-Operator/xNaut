// Vault knowledge-graph pane — the Obsidian-style "orb", cosmic edition.
//
// Renders a force-directed graph of a vault's notes ([[wikilinks]] = edges) via
// force-graph (2D) / 3d-force-graph (3D), loaded lazily from the CDN. Data comes
// from the Rust graph_scan command. Features: color-by-top-level-folder (nebula
// palette), orphan dimming/hiding, slow 3D auto-rotation, and a timeline that
// reveals notes in file-date order so the universe assembles itself.
// Pure DOM/Canvas/WebGL inside the parent webview.
(function () {
  'use strict';

  const inv = () => (window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke) || window.__TAURI__?.invoke;

  const panes = new Map();
  let labelCounter = 0;
  const nextLabel = () => `graph-${Date.now().toString(36)}-${++labelCounter}`;

  const LINK_COLOR = 'rgba(120,130,150,0.13)';
  const BG_COLOR = 'rgba(4,6,12,0)'; // transparent — the CSS nebula fog on .graph-host shows through
  const STAR_COLOR = 'rgba(190,205,232,0.62)'; // orphan / unlinked notes — dim starfield, gives the orb depth
  const FLASH_COLOR = 'rgba(238,246,255,0.98)'; // an orphan mid-twinkle
  const PARTICLE_COLOR = '#a8f0ff';            // travelling "signal" pulses between linked files (bright cyan)
  const DEFAULT_PATH = '~/Knowledge/Obsidian/Business/Engram';
  const DEFAULT_CODE_PATH = '~/DevHub_Studio/factory/02-Development/xnaut/src';
  const isOrphan = (n) => (n.val || 1) <= 1;
  const nodeSize = (n) => (isOrphan(n) ? 0.5 : n.val + 1); // linked nodes brighter/bigger
  const fmtDate = (secs) => { if (!secs) return ''; const d = new Date(secs * 1000); return d.getFullYear() + '-' + String(d.getMonth() + 1).padStart(2, '0'); };

  // Explicit UMD dist (sets window.ForceGraph / window.ForceGraph3D). The bare
  // `npm/<pkg>` path can resolve to an ESM build that never sets a global and
  // fails silently in this WebKit (same class as the TipTap failure).
  const CDN = {
    '2d': 'https://cdn.jsdelivr.net/npm/force-graph@1/dist/force-graph.min.js',
    '3d': 'https://cdn.jsdelivr.net/npm/3d-force-graph@1/dist/3d-force-graph.min.js',
  };
  const loaded = {};
  function loadScript(url) {
    if (loaded[url]) return loaded[url];
    loaded[url] = new Promise((res, rej) => {
      const s = document.createElement('script');
      s.src = url; s.onload = res; s.onerror = () => rej(new Error('failed to load ' + url));
      document.head.appendChild(s);
    });
    return loaded[url];
  }

  function injectStyles() {
    if (document.getElementById('graph-pane-styles')) return;
    const st = document.createElement('style');
    st.id = 'graph-pane-styles';
    st.textContent = `
.graph-pane { display:flex; flex-direction:column; flex:1 1 0%; width:100%; height:100%; min-width:0; min-height:0; overflow:hidden; background:${BG_COLOR}; border-radius:var(--radius-md,8px); }
.graph-bar { display:flex; align-items:center; gap:8px; padding:8px 12px; border-bottom:1px solid var(--border,rgba(255,255,255,.1)); font-size:12px; color:var(--text-muted,#8a8f98); flex-shrink:0; flex-wrap:wrap; }
.graph-source { background:rgba(255,255,255,.06); color:var(--text,#d7dae0); border:1px solid var(--border,rgba(255,255,255,.16)); border-radius:6px; font:inherit; font-size:11px; padding:3px 6px; cursor:pointer; flex-shrink:0; }
.graph-path { flex:1 1 180px; min-width:120px; background:rgba(255,255,255,.04); border:1px solid var(--border,rgba(255,255,255,.14)); border-radius:6px; color:var(--text,#d7dae0); font:inherit; font-size:12px; padding:4px 8px; outline:none; }
.graph-path:focus { border-color:#4dffd0; }
.graph-btn { background:rgba(255,255,255,.06); color:var(--text,#d7dae0); border:1px solid var(--border,rgba(255,255,255,.16)); border-radius:6px; font:inherit; font-size:11px; padding:4px 10px; cursor:pointer; flex-shrink:0; }
.graph-btn.primary { background:#4dffd0; color:#04120e; border-color:#4dffd0; font-weight:600; }
.graph-btn:hover { filter:brightness(1.15); }
.graph-toggle { display:flex; border:1px solid var(--border,rgba(255,255,255,.16)); border-radius:6px; overflow:hidden; flex-shrink:0; }
.graph-toggle button { background:transparent; border:none; color:var(--text-muted,#8a8f98); font:inherit; font-size:11px; padding:3px 10px; cursor:pointer; }
.graph-toggle button[data-active="1"] { background:#4dffd0; color:#04120e; }
.graph-check { display:flex; align-items:center; gap:5px; cursor:pointer; user-select:none; flex-shrink:0; }
.graph-scrub { flex:1 1 140px; min-width:100px; accent-color:#4dffd0; cursor:pointer; }
.graph-date { min-width:56px; font-variant-numeric:tabular-nums; color:var(--text,#d7dae0); }
.graph-host { flex:1 1 0%; min-height:0; position:relative; overflow:hidden;
  background:
    radial-gradient(1100px 780px at 32% 34%, rgba(78,54,150,0.24), transparent 62%),
    radial-gradient(950px 720px at 68% 62%, rgba(24,92,132,0.22), transparent 62%),
    radial-gradient(760px 620px at 54% 48%, rgba(140,36,104,0.16), transparent 58%),
    radial-gradient(1400px 1000px at 50% 50%, rgba(12,18,40,0.6), transparent 70%),
    #04060c; }
.graph-status { position:absolute; inset:0; display:flex; align-items:center; justify-content:center; color:var(--text-muted,#8a8f98); font-size:13px; pointer-events:none; text-align:center; padding:16px; }
.graph-legend { position:absolute; top:10px; left:12px; display:flex; flex-direction:column; gap:3px; font-size:10px; color:var(--text,#d7dae0); pointer-events:none; text-shadow:0 1px 3px #000; }
.graph-legend span { display:flex; align-items:center; gap:5px; }
.graph-legend i { width:8px; height:8px; border-radius:2px; display:inline-block; }
`;
    document.head.appendChild(st);
  }

  async function createGraphPane(tabId, parentContainer, opts) {
    opts = opts || {};
    injectStyles();
    const label = nextLabel();

    const pane = document.createElement('div');
    pane.className = 'graph-pane';
    pane.dataset.graphLabel = label;
    pane.innerHTML = `
      <div class="graph-bar">
        <select class="graph-source" title="Data source"><option value="vault">Vault</option><option value="code">Code</option></select>
        <input class="graph-path" spellcheck="false" placeholder="~/Knowledge/Obsidian/<vault>" />
        <button class="graph-btn primary graph-scan">Scan</button>
        <label class="graph-check"><input type="checkbox" class="graph-orphans" checked> Hide orphans</label>
        <div class="graph-toggle"><button data-mode="3d" data-active="1">3D</button><button data-mode="2d">2D</button></div>
        <button class="graph-btn graph-play" title="Play timeline">▶ Timeline</button>
        <input type="range" class="graph-scrub" min="0" max="100" value="100" />
        <span class="graph-date"></span>
        <button class="graph-btn graph-close" title="Close pane" aria-label="Close graph pane">✕</button>
      </div>
      <div class="graph-host"><div class="graph-status">Enter a vault path and hit Scan.</div></div>`;
    parentContainer.appendChild(pane);

    const pathInput = pane.querySelector('.graph-path');
    const host = pane.querySelector('.graph-host');
    const orphansCb = pane.querySelector('.graph-orphans');
    const playBtn = pane.querySelector('.graph-play');
    const scrub = pane.querySelector('.graph-scrub');
    const dateEl = pane.querySelector('.graph-date');
    pathInput.value = opts.path || DEFAULT_PATH;

    const entry = { paneEl: pane, tabId, mode: '3d', instance: null, master: null, playing: false, raf: 0 };
    panes.set(label, entry);

    const status = (msg) => {
      let el = host.querySelector('.graph-status');
      if (!el) { el = document.createElement('div'); el.className = 'graph-status'; host.appendChild(el); }
      el.textContent = msg;
    };
    const clearStatus = () => { const el = host.querySelector('.graph-status'); if (el) el.remove(); };

    // --- color by connected cluster (each constellation its own hue) --------
    const nodeColor = (n) => {
      if (isOrphan(n)) return (entry.flash && entry.flash.has(n.id)) ? FLASH_COLOR : STAR_COLOR;
      return (entry.colorByNode && entry.colorByNode[n.id]) || '#7aa8ff';
    };
    // Random orphan twinkle — a few stars briefly flare each tick.
    function stopTwinkle() { if (entry.twinkleTimer) { clearInterval(entry.twinkleTimer); entry.twinkleTimer = 0; } }
    function startTwinkle() {
      stopTwinkle();
      entry.flash = new Set();
      entry.twinkleTimer = setInterval(() => {
        const g = entry.instance;
        if (!g) return;
        const nodes = g.graphData().nodes;
        if (nodes.length > 2500) return; // ponytail: skip on huge graphs — nodeColor refresh gets costly
        const orphans = nodes.filter(isOrphan);
        if (!orphans.length) return;
        const n = Math.max(3, (orphans.length * 0.012) | 0);
        for (let k = 0; k < n; k++) {
          const id = orphans[(Math.random() * orphans.length) | 0].id;
          entry.flash.add(id);
          setTimeout(() => entry.flash.delete(id), 200);
        }
        g.nodeColor(nodeColor); // re-apply to repaint the flared stars
      }, 160);
    }
    function computeClusterColors(master) {
      const parent = {};
      const find = (x) => { while (parent[x] !== x) { parent[x] = parent[parent[x]]; x = parent[x]; } return x; };
      master.nodes.forEach((n) => { parent[n.id] = n.id; });
      master.links.forEach((l) => {
        const a = linkId(l.source), b = linkId(l.target);
        if (parent[a] === undefined || parent[b] === undefined) return;
        parent[find(a)] = find(b);
      });
      const size = {};
      master.nodes.forEach((n) => { const r = find(n.id); size[r] = (size[r] || 0) + 1; });
      // biggest clusters first, skip singletons (those are orphans → grey dust)
      const roots = [...new Set(master.nodes.map((n) => find(n.id)))].filter((r) => size[r] > 1).sort((a, b) => size[b] - size[a]);
      const colorByRoot = {};
      // start hues in blue (210°) and walk the golden angle so the biggest
      // cluster isn't red and no two neighbours collide.
      roots.forEach((r, i) => { colorByRoot[r] = `hsl(${(210 + i * 137.508) % 360}, 78%, 64%)`; });
      const map = {};
      master.nodes.forEach((n) => { const r = find(n.id); if (colorByRoot[r]) map[n.id] = colorByRoot[r]; });
      return map;
    }
    // Code mode: color by file extension. Returns the node→color map plus a
    // legend (ext → color, by count) since here the color is meaningful.
    const extOf = (n) => { const m = /\.([a-z0-9]+)$/i.exec(n.id || n.label || ''); return m ? m[1].toLowerCase() : 'other'; };
    function computeExtColors(master) {
      const exts = [...new Set(master.nodes.map(extOf))].sort();
      const byExt = {};
      exts.forEach((e, i) => { byExt[e] = `hsl(${(30 + i * 137.508) % 360}, 72%, 63%)`; });
      const map = {};
      master.nodes.forEach((n) => { map[n.id] = byExt[extOf(n)]; });
      const counts = {};
      master.nodes.forEach((n) => { counts[extOf(n)] = (counts[extOf(n)] || 0) + 1; });
      const legend = Object.entries(counts).sort((a, b) => b[1] - a[1]).slice(0, 10)
        .map(([e, c]) => ({ name: '.' + e, color: byExt[e], count: c }));
      return { map, legend };
    }
    function renderLegend() {
      pane.querySelector('.graph-legend')?.remove();
      if (!entry.legend || !entry.legend.length) return;
      const lg = document.createElement('div');
      lg.className = 'graph-legend';
      lg.innerHTML = entry.legend.map((r) => `<span><i style="background:${r.color}"></i>${r.name} (${r.count})</span>`).join('');
      host.appendChild(lg);
    }

    // --- travelling "signal" pulses between linked files --------------------
    function stopEmitter() { if (entry.emitTimer) { clearInterval(entry.emitTimer); entry.emitTimer = 0; } }
    function startEmitter() {
      stopEmitter();
      entry.emitTimer = setInterval(() => {
        const g = entry.instance;
        if (!g || typeof g.emitParticle !== 'function') return;
        const ls = g.graphData().links;
        if (!ls.length) return;
        const bursts = 1 + ((Math.random() * 2) | 0); // 1–2 gentle signals per tick
        for (let k = 0; k < bursts; k++) {
          try { g.emitParticle(ls[(Math.random() * ls.length) | 0]); } catch (_) {}
        }
      }, 150);
    }

    // --- data helpers -------------------------------------------------------
    const linkId = (e) => (e && typeof e === 'object' ? e.id : e);
    function baseNodes() {
      if (!entry.master) return [];
      return orphansCb.checked ? entry.master.nodes.filter((n) => !isOrphan(n)) : entry.master.nodes;
    }
    function linksFor(nodes) {
      const ids = new Set(nodes.map((n) => n.id));
      return entry.master.links.filter((l) => ids.has(linkId(l.source)) && ids.has(linkId(l.target)));
    }
    function setData(nodes) {
      if (entry.instance) entry.instance.graphData({ nodes, links: linksFor(nodes) });
    }
    function disposeInstance() {
      stopTimeline();
      stopEmitter();
      stopTwinkle();
      if (entry.instance && typeof entry.instance._destructor === 'function') {
        try { entry.instance._destructor(); } catch (_) {}
      }
      entry.instance = null;
      host.querySelectorAll('canvas, .scene-container, .graph-tooltip').forEach((n) => n.remove());
    }

    async function render() {
      if (!entry.master) return;
      disposeInstance();
      clearStatus();
      const w = host.clientWidth || 600, h = host.clientHeight || 400;
      const nodes = baseNodes();
      try {
        if (entry.mode === '3d') {
          await loadScript(CDN['3d']);
          const g = window.ForceGraph3D({ controlType: 'orbit' })(host)
            .width(w).height(h).backgroundColor(BG_COLOR)
            .nodeLabel('label').nodeVal(nodeSize).nodeRelSize(4)
            .nodeColor(nodeColor).nodeOpacity(1)
            .linkColor(() => LINK_COLOR).linkOpacity(0.28)
            .linkDirectionalParticleColor(() => PARTICLE_COLOR)
            .linkDirectionalParticles(1).linkDirectionalParticleWidth(3).linkDirectionalParticleSpeed(0.006)
            .graphData({ nodes, links: linksFor(nodes) });
          const c = g.controls(); if (c) { c.autoRotate = true; c.autoRotateSpeed = 0.5; } // the slow nebula drift
          entry.instance = g;
        } else {
          await loadScript(CDN['2d']);
          const g = window.ForceGraph()(host)
            .width(w).height(h).backgroundColor(BG_COLOR)
            .nodeLabel('label').nodeVal(nodeSize).nodeRelSize(4)
            .nodeColor(nodeColor).linkColor(() => LINK_COLOR).linkWidth(1)
            .linkDirectionalParticleColor(() => PARTICLE_COLOR)
            .linkDirectionalParticles(1).linkDirectionalParticleWidth(3).linkDirectionalParticleSpeed(0.006)
            .graphData({ nodes, links: linksFor(nodes) });
          g.onEngineStop(() => g.zoomToFit(400, 40));
          entry.instance = g;
        }
        startEmitter();
        startTwinkle();
        renderLegend();
      } catch (e) {
        console.error('[graph-pane] render failed', e);
        status('Graph library failed to load: ' + String(e.message || e));
      }
    }

    // --- timeline build-up --------------------------------------------------
    function stopTimeline() {
      entry.playing = false;
      if (entry.raf) { cancelAnimationFrame(entry.raf); entry.raf = 0; }
      playBtn.textContent = '▶ Timeline';
    }
    function showAtProgress(p) {
      const base = baseNodes().slice().sort((a, b) => a.ts - b.ts);
      if (!base.length) return;
      const tMin = base[0].ts, tMax = base[base.length - 1].ts;
      const cut = tMin + (tMax - tMin) * p;
      setData(base.filter((n) => n.ts <= cut || !n.ts));
      dateEl.textContent = fmtDate(cut);
    }
    function playTimeline() {
      if (entry.playing) { stopTimeline(); return; }
      const base = baseNodes().slice().sort((a, b) => a.ts - b.ts);
      if (base.length < 2) return;
      entry.playing = true; playBtn.textContent = '⏸ Timeline';
      const tMin = base[0].ts, tMax = base[base.length - 1].ts || tMin + 1;
      const dur = 18000;
      let start = null, i = 0; const revealed = [];
      const step = (t) => {
        if (!entry.playing) return;
        if (start == null) start = t;
        const p = Math.min(1, (t - start) / dur);
        const cut = tMin + (tMax - tMin) * p;
        let added = false;
        while (i < base.length && (base[i].ts <= cut || !base[i].ts)) { revealed.push(base[i]); i++; added = true; }
        if (added) setData(revealed.slice()); // same node refs → positions persist, new ones fly in
        scrub.value = String(p * 100); dateEl.textContent = fmtDate(cut);
        if (p < 1) { entry.raf = requestAnimationFrame(step); }
        else { stopTimeline(); setData(baseNodes()); }
      };
      entry.raf = requestAnimationFrame(step);
    }

    // --- scan ---------------------------------------------------------------
    async function scan() {
      const invoke = inv();
      if (!invoke) { status('Tauri API not ready.'); return; }
      const path = pathInput.value.trim();
      if (!path) { status('Enter a vault path.'); return; }
      stopTimeline();
      const source = pane.querySelector('.graph-source').value;
      const cmd = source === 'code' ? 'code_scan' : 'graph_scan';
      const noun = source === 'code' ? 'files' : 'notes';
      status('Scanning ' + path + ' …');
      try {
        const data = await invoke(cmd, { path });
        entry.master = data;
        if (!data.nodes.length) { status(`No ${source === 'code' ? 'source files' : '.md notes'} found under ` + path); return; }
        if (source === 'code') {
          const ec = computeExtColors(data); // color by file type
          entry.colorByNode = ec.map; entry.legend = ec.legend;
        } else {
          entry.colorByNode = computeClusterColors(data); // one hue per connected constellation
          entry.legend = null;
        }
        const linked = data.nodes.filter((n) => !isOrphan(n)).length;
        status(`${data.nodes.length} ${noun} · ${linked} linked · ${data.links.length} links — laying out…`);
        scrub.value = '100'; dateEl.textContent = '';
        await render();
      } catch (e) {
        console.error('[graph-pane] scan failed', e);
        status('Scan failed: ' + String(e));
      }
    }

    // --- wiring -------------------------------------------------------------
    pane.querySelector('.graph-scan').onclick = scan;
    pane.querySelector('.graph-source').onchange = (e) => {
      const v = e.target.value;
      // swap the default path when the field still holds the other mode's default
      if (v === 'code' && pathInput.value === DEFAULT_PATH) pathInput.value = DEFAULT_CODE_PATH;
      else if (v === 'vault' && pathInput.value === DEFAULT_CODE_PATH) pathInput.value = DEFAULT_PATH;
      pathInput.placeholder = v === 'code' ? '~/path/to/repo' : '~/Knowledge/Obsidian/<vault>';
      scan();
    };
    pathInput.addEventListener('keydown', (e) => { if (e.key === 'Enter') { e.preventDefault(); e.stopPropagation(); scan(); } });
    orphansCb.onchange = () => { stopTimeline(); scrub.value = '100'; setData(baseNodes()); };
    playBtn.onclick = playTimeline;
    scrub.oninput = () => { stopTimeline(); showAtProgress(Number(scrub.value) / 100); };
    pane.querySelectorAll('.graph-toggle button').forEach((b) => {
      b.onclick = () => {
        if (entry.mode === b.dataset.mode) return;
        entry.mode = b.dataset.mode;
        pane.querySelectorAll('.graph-toggle button').forEach((x) => x.removeAttribute('data-active'));
        b.dataset.active = '1';
        render();
      };
    });
    pane.querySelector('.graph-close').onclick = async () => {
      if (window.xnautClosePaneByElement) await window.xnautClosePaneByElement(pane, tabId);
      else destroyGraphPane(label);
    };

    const ro = new ResizeObserver(() => {
      if (!entry.instance) return;
      const w = host.clientWidth, h = host.clientHeight;
      if (w > 0 && h > 0) entry.instance.width(w).height(h);
    });
    ro.observe(host);
    entry.resizeObs = ro;

    scan(); // auto-scan on open
    return { kind: 'graph', label, pane };
  }

  function destroyGraphPane(label) {
    const entry = panes.get(label);
    if (!entry) return;
    if (entry.raf) cancelAnimationFrame(entry.raf);
    if (entry.emitTimer) clearInterval(entry.emitTimer);
    if (entry.twinkleTimer) clearInterval(entry.twinkleTimer);
    try { entry.resizeObs && entry.resizeObs.disconnect(); } catch (_) {}
    if (entry.instance && typeof entry.instance._destructor === 'function') { try { entry.instance._destructor(); } catch (_) {} }
    if (entry.paneEl && entry.paneEl.parentNode) entry.paneEl.parentNode.removeChild(entry.paneEl);
    panes.delete(label);
  }

  window.xnautCreateGraphPane = createGraphPane;
  window.xnautDestroyGraphPane = destroyGraphPane;
  window.xnautForgetGraphPane = (label) => {
    const entry = panes.get(label);
    if (!entry) return;
    if (entry.raf) cancelAnimationFrame(entry.raf);
    if (entry.emitTimer) clearInterval(entry.emitTimer);
    if (entry.twinkleTimer) clearInterval(entry.twinkleTimer);
    try { entry.resizeObs && entry.resizeObs.disconnect(); } catch (_) {}
    if (entry.instance && typeof entry.instance._destructor === 'function') { try { entry.instance._destructor(); } catch (_) {} }
    panes.delete(label);
  };
})();
