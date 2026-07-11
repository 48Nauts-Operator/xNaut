import { css, html, LitElement } from 'lit';
import { NodeEditor, ClassicPreset } from 'rete';
import { AreaPlugin, AreaExtensions } from 'rete-area-plugin';
import { ConnectionPlugin, Presets as ConnectionPresets } from 'rete-connection-plugin';
import { LitPlugin, Presets as LitPresets } from 'rete-lit-plugin/dist/rete-litv-plugin.esm.js';

const KIND_COLORS = {
  trigger: '#34d399',
  agent: '#a78bfa',
  action: '#60a5fa',
  decision: '#fbbf24',
  human_approval: '#f87171',
  transform: '#22d3ee',
  retry: '#fb923c',
  parallel: '#e879f9',
  subflow: '#818cf8',
  output: '#2dd4bf',
};

class XnautWorkflowNode extends LitElement {
  static properties = {
    data: { attribute: false },
    emit: { attribute: false },
    seed: {},
  };

  constructor() {
    super();
    this.data = null;
    this.emit = () => {};
    this.seed = '';
  }

  static styles = css`
    :host { display:block; width:220px; font-family:Inter,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif; letter-spacing:0; }
    .node { overflow:hidden; border:1px solid #444953; border-radius:7px; background:#20232a; color:#e7e9ee; box-shadow:0 8px 22px rgba(0,0,0,.26); cursor:pointer; user-select:none; }
    .node:hover { border-color:#626975; }
    .node.selected { border-color:var(--kind-color); box-shadow:0 0 0 1px var(--kind-color),0 10px 28px rgba(0,0,0,.32); }
    .head { display:flex; align-items:center; gap:9px; min-height:42px; padding:8px 10px; border-bottom:1px solid #343840; }
    .kind { width:8px; height:26px; flex:0 0 auto; border-radius:3px; background:var(--kind-color); }
    .copy { min-width:0; flex:1 1 auto; }
    .title { overflow:hidden; font-size:12px; font-weight:700; text-overflow:ellipsis; white-space:nowrap; }
    .type { margin-top:2px; color:#8d939f; font-size:9px; font-weight:650; text-transform:uppercase; }
    .status { width:8px; height:8px; flex:0 0 auto; border-radius:50%; background:#69707c; }
    .status[data-status="ready"] { background:#a78bfa; }
    .status[data-status="running"] { border:2px solid rgba(52,211,153,.3); border-top-color:#34d399; background:transparent; animation:spin .8s linear infinite; }
    .status[data-status="waiting_for_approval"] { background:#fbbf24; }
    .status[data-status="completed"] { background:#60a5fa; }
    .status[data-status="failed"] { background:#f87171; }
    .ports { display:grid; grid-template-columns:minmax(0,1fr) minmax(0,1fr); gap:8px; padding:8px 0 9px; }
    .inputs,.outputs { display:flex; flex-direction:column; gap:6px; }
    .port { display:flex; align-items:center; min-height:20px; color:#aab0ba; font-size:10px; }
    .output { justify-content:flex-end; text-align:right; }
    .label { overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
    ref-element { display:inline-flex; align-items:center; }
    .input ref-element { margin-left:-9px; margin-right:5px; }
    .output ref-element { margin-left:5px; margin-right:-9px; }
    .empty { min-height:18px; }
    @keyframes spin { to { transform:rotate(360deg); } }
  `;

  portEntries(side) {
    return Object.entries(this.data?.[side] || {}).sort((a, b) => (a[1]?.index || 0) - (b[1]?.index || 0));
  }

  socketRef(side, key, port) {
    return html`<ref-element
      .data=${{ type: 'socket', side, key, nodeId: this.data?.id, payload: port.socket }}
      .emit=${this.emit}
    ></ref-element>`;
  }

  render() {
    const meta = this.data?.xnaut || {};
    const kind = meta.kind || 'action';
    const color = KIND_COLORS[kind] || '#60a5fa';
    const inputs = this.portEntries('inputs');
    const outputs = this.portEntries('outputs');
    return html`<div class="node ${this.data?.selected ? 'selected' : ''}" style="--kind-color:${color}">
      <div class="head">
        <span class="kind"></span>
        <span class="copy"><span class="title">${this.data?.label || meta.name || 'Node'}</span><span class="type">${kind.replaceAll('_', ' ')}</span></span>
        <span class="status" data-status=${meta.runStatus || 'pending'}></span>
      </div>
      <div class="ports">
        <div class="inputs">${inputs.length ? inputs.map(([key, port]) => html`<div class="port input">${this.socketRef('input', key, port)}<span class="label">${port.label || key}</span></div>`) : html`<span class="empty"></span>`}</div>
        <div class="outputs">${outputs.length ? outputs.map(([key, port]) => html`<div class="port output"><span class="label">${port.label || key}</span>${this.socketRef('output', key, port)}</div>`) : html`<span class="empty"></span>`}</div>
      </div>
    </div>`;
  }
}

function clone(value) {
  return JSON.parse(JSON.stringify(value));
}

function normalizeNode(definition) {
  return {
    id: definition.id,
    kind: definition.kind || 'action',
    name: definition.name || definition.kind || 'Node',
    inputs: clone(definition.inputs || []),
    outputs: clone(definition.outputs || []),
    config: clone(definition.config ?? null),
    permissions: clone(definition.permissions || []),
    model_policy: clone(definition.model_policy ?? null),
    timeout_seconds: definition.timeout_seconds ?? null,
    max_retries: Number(definition.max_retries || 0),
  };
}

async function createEditor(container, hooks = {}) {
  const editor = new NodeEditor();
  const area = new AreaPlugin(container);
  const connection = new ConnectionPlugin();
  const render = new LitPlugin();
  const sockets = new Map();
  let loading = false;
  let selectedId = '';

  render.addPreset(LitPresets.classic.setup({
    customize: { node: () => XnautWorkflowNode },
  }));
  connection.addPreset(ConnectionPresets.classic.setup());
  editor.use(area);
  area.use(connection);
  area.use(render);
  AreaExtensions.simpleNodesOrder(area);
  const selector = AreaExtensions.selector();
  const accumulating = AreaExtensions.accumulateOnCtrl();
  AreaExtensions.selectableNodes(area, selector, { accumulating });

  const notifyChange = () => {
    if (!loading) hooks.onChange?.();
  };
  editor.addPipe((context) => {
    if (['nodecreated', 'noderemoved', 'connectioncreated', 'connectionremoved'].includes(context.type)) notifyChange();
    return context;
  });
  area.addPipe((context) => {
    if (context.type === 'nodepicked') {
      selectedId = context.data.id;
      hooks.onSelect?.(selectedId, getNode(selectedId));
    }
    if (context.type === 'nodetranslated' || context.type === 'zoomed' || context.type === 'translated') notifyChange();
    return context;
  });

  function socket(dataType) {
    const key = dataType || 'any';
    if (!sockets.has(key)) sockets.set(key, new ClassicPreset.Socket(key));
    return sockets.get(key);
  }

  function toReteNode(definition) {
    const normalized = normalizeNode(definition);
    const node = new ClassicPreset.Node(normalized.name);
    node.id = normalized.id;
    node.xnaut = normalized;
    for (const input of normalized.inputs) {
      node.addInput(input.id, new ClassicPreset.Input(socket(input.data_type), input.id, true));
    }
    for (const output of normalized.outputs) {
      node.addOutput(output.id, new ClassicPreset.Output(socket(output.data_type), output.id, true));
    }
    return node;
  }

  function getNode(id) {
    const node = editor.getNode(id);
    return node ? clone(node.xnaut) : null;
  }

  async function load(definition) {
    loading = true;
    selectedId = '';
    await editor.clear();
    sockets.clear();
    const nodes = new Map();
    for (const item of definition.nodes || []) {
      const node = toReteNode(item);
      nodes.set(node.id, node);
      await editor.addNode(node);
      const position = definition.presentation?.nodes?.[node.id] || { x: 80 + nodes.size * 36, y: 80 + nodes.size * 24 };
      await area.translate(node.id, { x: Number(position.x || 0), y: Number(position.y || 0) });
    }
    for (const item of definition.connections || []) {
      const source = nodes.get(item.from_node);
      const target = nodes.get(item.to_node);
      if (!source || !target || !source.outputs[item.from_port] || !target.inputs[item.to_port]) continue;
      const edge = new ClassicPreset.Connection(source, item.from_port, target, item.to_port);
      edge.id = item.id;
      await editor.addConnection(edge);
    }
    const viewport = definition.presentation || {};
    await area.area.translate(Number(viewport.viewport_x || 0), Number(viewport.viewport_y || 0));
    await area.area.zoom(Number(viewport.zoom || 1), 0, 0);
    if ((definition.nodes || []).length) await AreaExtensions.zoomAt(area, editor.getNodes(), { scale: 0.86 });
    loading = false;
  }

  async function addNode(definition, position) {
    const node = toReteNode(definition);
    await editor.addNode(node);
    const transform = area.area.transform;
    const x = position?.x ?? (container.clientWidth / 2 - transform.x) / transform.k - 110;
    const y = position?.y ?? (container.clientHeight / 2 - transform.y) / transform.k - 40;
    await area.translate(node.id, { x, y });
    await selector.unselectAll();
    selectedId = node.id;
    hooks.onSelect?.(selectedId, clone(node.xnaut));
    notifyChange();
    return clone(node.xnaut);
  }

  async function updateNode(id, definition) {
    const node = editor.getNode(id);
    if (!node) throw new Error(`Node not found: ${id}`);
    node.xnaut = normalizeNode({ ...node.xnaut, ...definition, id });
    node.label = node.xnaut.name;
    await area.update('node', id);
    hooks.onSelect?.(id, clone(node.xnaut));
    notifyChange();
  }

  async function removeSelected() {
    if (!selectedId) return;
    for (const edge of editor.getConnections().filter((item) => item.source === selectedId || item.target === selectedId)) {
      await editor.removeConnection(edge.id);
    }
    await editor.removeNode(selectedId);
    selectedId = '';
    hooks.onSelect?.('', null);
  }

  function serialize(base) {
    const presentationNodes = {};
    const nodes = editor.getNodes().map((node) => {
      const position = area.nodeViews.get(node.id)?.position || { x: 0, y: 0 };
      presentationNodes[node.id] = { x: position.x, y: position.y, collapsed: false };
      return clone(node.xnaut);
    });
    const connections = editor.getConnections().map((item) => ({
      id: item.id,
      from_node: item.source,
      from_port: item.sourceOutput,
      to_node: item.target,
      to_port: item.targetInput,
    }));
    return {
      ...clone(base),
      nodes,
      connections,
      presentation: {
        nodes: presentationNodes,
        viewport_x: area.area.transform.x,
        viewport_y: area.area.transform.y,
        zoom: area.area.transform.k,
      },
    };
  }

  async function setRun(run) {
    for (const node of editor.getNodes()) {
      node.xnaut.runStatus = run?.nodes?.[node.id]?.status || 'pending';
      await area.update('node', node.id);
    }
  }

  async function focus() {
    if (editor.getNodes().length) await AreaExtensions.zoomAt(area, editor.getNodes(), { scale: 0.86 });
  }

  function destroy() {
    accumulating.destroy();
    area.destroy();
  }

  return { load, addNode, updateNode, removeSelected, serialize, setRun, focus, getNode, destroy };
}

export { createEditor };
