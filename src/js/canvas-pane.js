// The agent's canvas, drawn as SVG.
//
// Ported from Cockpit (48Nauts, private), `src/components/concept-canvas-screen.tsx`.
// Cockpit renders with React Flow; xNAUT's frontend has no bundler, so this
// draws the same scene format directly in SVG — about a hundred lines against
// a dependency, and it means the canvas works with nothing installed.
//
// What travels from Cockpit is the CONTRACT, not the renderer: the agent sends
// the complete graph, boxes keep their positions by id, and the visible graph
// is what the agent is told it is looking at. Dragging a box writes its new
// position straight back, so the arrangement is the owner's, not the model's.
(function () {
  'use strict';

  const invoke = (...args) => window.__TAURI__.core.invoke(...args);
  const esc = (value) => String(value == null ? '' : value).replace(/[&<>"']/g, (c) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  }[c]));

  // Colour carries KIND, and nothing else. A box is not coloured by status,
  // freshness or importance — that is how a diagram turns into a traffic light.
  const KIND_COLOR = {
    actor: '#f5b840',
    component: '#6aa9ff',
    service: '#4ade80',
    agent: '#c084fc',
    'data-store': '#38bdf8',
    'external-system': '#94a3b8',
    decision: '#fb923c',
    document: '#e2e8f0',
    note: '#a3a3a3',
    'trust-boundary': '#ef4444',
  };
  const NODE_W = 168;
  const NODE_H = 62;

  function ensureStyles() {
    if (document.getElementById('canvas-pane-styles')) return;
    const style = document.createElement('style');
    style.id = 'canvas-pane-styles';
    style.textContent = `
      .cvs { display:flex; flex-direction:column; height:100%; min-height:0; background:var(--bg-primary,#0a0a0f); }
      .cvs-bar { display:flex; align-items:center; gap:8px; padding:9px 12px; border-bottom:1px solid var(--border,#26262c); }
      .cvs-title { flex:1; min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;
        color:var(--text-primary,#e8e8ec); font-size:12px; font-weight:600; }
      .cvs-btn { padding:4px 10px; border:1px solid var(--border,#2a2a2f); border-radius:7px; background:transparent;
        color:var(--text-secondary,#a0a0a0); font:inherit; font-size:11px; cursor:pointer; }
      .cvs-btn:hover { color:var(--text-primary,#e8e8ec); }
      .cvs-stage { flex:1 1 auto; min-height:0; overflow:auto; }
      .cvs-node { cursor:grab; }
      .cvs-node.dragging { cursor:grabbing; }
      .cvs-node rect { fill:#16161c; stroke-width:1.5; }
      .cvs-node text { fill:var(--text-primary,#e8e8ec); font-size:12px; font-weight:600; }
      .cvs-node .cvs-kind { fill:#8a8a94; font-size:9px; font-weight:600; letter-spacing:.08em; text-transform:uppercase; }
      .cvs-edge { stroke:#4a4a55; stroke-width:1.4; fill:none; }
      .cvs-edge-label { fill:#8a8a94; font-size:9px; }
      .cvs-empty { margin:auto; padding:30px; color:var(--text-secondary,#8a8a94); font-size:12px; text-align:center; }
    `;
    document.head.appendChild(style);
  }

  function centre(node) {
    return { x: (node.x || 0) + NODE_W / 2, y: (node.y || 0) + NODE_H / 2 };
  }

  function createCanvasPane(key, parent, options) {
    ensureStyles();
    const pane = document.createElement('section');
    pane.className = 'cvs';
    parent.appendChild(pane);

    let canvas = { title: '', nodes: [], edges: [] };
    let dragging = null;

    async function load() {
      canvas = (await invoke('canvas_get', { key }).catch(() => null)) || { title: '', nodes: [], edges: [] };
      render();
    }

    function svgMarkup() {
      const nodes = canvas.nodes || [];
      const edges = canvas.edges || [];
      if (!nodes.length) {
        return '<div class="cvs-empty">Nothing drawn yet. Ask the agent for a diagram and it appears here.</div>';
      }
      const width = Math.max(...nodes.map((n) => (n.x || 0) + NODE_W)) + 60;
      const height = Math.max(...nodes.map((n) => (n.y || 0) + NODE_H)) + 60;
      const byId = new Map(nodes.map((node) => [node.id, node]));
      const lines = edges.map((edge) => {
        const from = byId.get(edge.source);
        const to = byId.get(edge.target);
        if (!from || !to) return '';
        const a = centre(from);
        const b = centre(to);
        const mid = { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
        return `<path class="cvs-edge" d="M ${a.x} ${a.y} L ${b.x} ${b.y}" marker-end="url(#cvs-arrow)"/>`
          + (edge.label ? `<text class="cvs-edge-label" x="${mid.x}" y="${mid.y - 4}" text-anchor="middle">${esc(edge.label)}</text>` : '');
      }).join('');
      const boxes = nodes.map((node) => {
        const colour = KIND_COLOR[node.kind] || KIND_COLOR.component;
        return `<g class="cvs-node" data-node="${esc(node.id)}" transform="translate(${node.x || 0},${node.y || 0})">
          <rect width="${NODE_W}" height="${NODE_H}" rx="10" style="stroke:${colour}"/>
          <text class="cvs-kind" x="12" y="20">${esc(node.kind || 'component')}</text>
          <text x="12" y="41">${esc(String(node.label || node.id).slice(0, 22))}</text>
        </g>`;
      }).join('');
      return `<svg width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">
        <defs><marker id="cvs-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto">
          <path d="M 0 0 L 10 5 L 0 10 z" fill="#4a4a55"/></marker></defs>
        ${lines}${boxes}</svg>`;
    }

    function render() {
      pane.innerHTML = `<div class="cvs-bar">
          <span class="cvs-title">${esc(canvas.title || 'Canvas')}</span>
          <button class="cvs-btn" data-undo>Undo</button>
          <button class="cvs-btn" data-mermaid>Copy as Mermaid</button>
          ${options && options.onFullScreen ? '<button class="cvs-btn" data-full>Full screen</button>' : ''}
          ${options && options.onClose ? '<button class="cvs-btn" data-close aria-label="Close canvas">✕</button>' : ''}
        </div>
        <div class="cvs-stage" data-stage>${svgMarkup()}</div>`;
      wire();
    }

    function wire() {
      pane.querySelector('[data-undo]').onclick = async () => {
        try { await invoke('canvas_undo', { key }); await load(); }
        catch (error) { console.warn('[canvas] undo:', error); }
      };
      const full = pane.querySelector('[data-full]');
      if (full) full.onclick = () => options.onFullScreen();
      const close = pane.querySelector('[data-close]');
      if (close) close.onclick = () => options.onClose();
      pane.querySelector('[data-mermaid]').onclick = () => {
        const lines = ['flowchart LR'];
        (canvas.nodes || []).forEach((node) => lines.push(`  ${node.id}[${node.label}]`));
        (canvas.edges || []).forEach((edge) => lines.push(
          `  ${edge.source} ${edge.label ? `-- ${edge.label} -->` : '-->'} ${edge.target}`));
        if (navigator.clipboard) navigator.clipboard.writeText(lines.join('\n'));
      };

      // Dragging writes the position back: the arrangement belongs to whoever
      // is looking at it, and the agent's next redraw keeps it.
      const stage = pane.querySelector('[data-stage]');
      stage.querySelectorAll('[data-node]').forEach((group) => {
        group.addEventListener('mousedown', (event) => {
          const node = (canvas.nodes || []).find((item) => item.id === group.dataset.node);
          if (!node) return;
          dragging = { node, group, startX: event.clientX, startY: event.clientY, originX: node.x || 0, originY: node.y || 0 };
          group.classList.add('dragging');
          event.preventDefault();
        });
      });
      const move = (event) => {
        if (!dragging) return;
        const x = dragging.originX + (event.clientX - dragging.startX);
        const y = dragging.originY + (event.clientY - dragging.startY);
        dragging.group.setAttribute('transform', `translate(${Math.max(0, x)},${Math.max(0, y)})`);
        dragging.node.x = Math.max(0, x);
        dragging.node.y = Math.max(0, y);
      };
      const drop = async () => {
        if (!dragging) return;
        dragging.group.classList.remove('dragging');
        dragging = null;
        try { await invoke('canvas_set', { key, canvas: { ...canvas, previous: null } }); }
        catch (error) { console.warn('[canvas] save positions:', error); }
        render();
      };
      window.addEventListener('mousemove', move);
      window.addEventListener('mouseup', drop);
      pane.__cleanup = () => {
        window.removeEventListener('mousemove', move);
        window.removeEventListener('mouseup', drop);
      };
    }

    // The agent redraws; the pane follows.
    const changed = window.__TAURI__.event.listen('canvas-changed', (event) => {
      if (!event || !event.payload || event.payload.key !== key) return;
      load();
    });

    load();
    return {
      kind: 'canvas',
      label: `canvas-${key}`,
      pane,
      dispose() {
        if (pane.__cleanup) pane.__cleanup();
        Promise.resolve(changed).then((off) => { try { off(); } catch (_) {} }).catch(() => {});
      },
    };
  }

  window.xnautCreateCanvasPane = createCanvasPane;
  // The tab version: same pane, whole screen. The key is handed over by
  // whoever opened it.
  window.xnautCreateCanvasTab = (tabId, parent) =>
    createCanvasPane(window.xnautCanvasTabKey || 'nautbot', parent);
})();
