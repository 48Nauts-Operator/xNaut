// xNAUT visual Agent Loop workspace backed by the Rust runtime.
(function () {
  'use strict';

  const invoke = (...args) => window.__TAURI__.core.invoke(...args);
  const listen = (...args) => window.__TAURI__.event.listen(...args);
  const panes = new Map();
  let counter = 0;

  const VIEWS = [
    ['workflows', 'Agent Loops'],
    ['runs', 'Runs'],
    ['approvals', 'Approvals'],
    ['costs', 'Costs'],
    ['library', 'Node Library'],
    ['settings', 'Settings'],
  ];

  const NODE_TYPES = [
    ['trigger', 'Trigger', 'Starts a run', [], [['event', 'any']]],
    ['agent', 'Agent', 'Runs an Agent profile', [['input', 'any']], [['success', 'any'], ['error', 'any']]],
    ['action', 'Action', 'Invokes an xNAUT tool', [['input', 'any']], [['success', 'any'], ['error', 'any']]],
    ['decision', 'Decision', 'Routes structured output', [['input', 'any']], [['yes', 'any'], ['no', 'any']]],
    ['human_approval', 'Human approval', 'Waits for an authorized decision', [['input', 'any']], [['approved', 'any'], ['rejected', 'any']]],
    ['transform', 'Transform', 'Maps structured data', [['input', 'any']], [['output', 'any']]],
    ['retry', 'Retry', 'Starts a bounded cycle', [['input', 'any']], [['retry', 'any']]],
    ['parallel', 'Parallel', 'Starts independent branches', [['input', 'any']], [['branch_a', 'any'], ['branch_b', 'any']]],
    ['subflow', 'Sub-loop', 'Runs a versioned Agent Loop', [['input', 'any']], [['output', 'any'], ['error', 'any']]],
    ['output', 'Output', 'Completes the run', [['result', 'any']], []],
  ];

  const ICON = {
    plus: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M8 3v10M3 8h10"/></svg>',
    save: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4"><path d="M2.5 2.5h9l2 2v9h-11z"/><path d="M5 2.5v4h6v-4M5 13v-4h6v4"/></svg>',
    focus: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4"><path d="M5 2H2v3M11 2h3v3M5 14H2v-3M11 14h3v-3"/></svg>',
    trash: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4"><path d="M3 4h10M6 4V2.5h4V4M4.5 4l.7 9h5.6l.7-9"/></svg>',
    refresh: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M13 8a5 5 0 1 1-1.5-3.5"/><path d="M13 2v3h-3"/></svg>',
  };

  function esc(value) {
    return String(value == null ? '' : value).replace(/[&<>"']/g, (ch) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[ch]));
  }

  function clone(value) {
    return JSON.parse(JSON.stringify(value));
  }

  function uid(prefix) {
    const id = crypto.randomUUID ? crypto.randomUUID() : `${Date.now()}-${Math.random().toString(16).slice(2)}`;
    return `${prefix}-${id}`.slice(0, 80);
  }

  function relativeTime(iso) {
    const at = Date.parse(iso);
    if (!Number.isFinite(at)) return '';
    const seconds = Math.max(0, Math.floor((Date.now() - at) / 1000));
    for (const [unit, size] of [['day', 86400], ['hour', 3600], ['minute', 60]]) {
      if (seconds >= size) { const amount = Math.floor(seconds / size); return `${amount}${unit[0]} ago`; }
    }
    return 'now';
  }

  function injectStyles() {
    if (document.getElementById('loops-styles')) return;
    const style = document.createElement('style');
    style.id = 'loops-styles';
    style.textContent = `
.loops { display:flex; flex-direction:column; width:100%; height:100%; min-width:0; min-height:0; overflow:hidden; background:var(--editor-surface,#1b1d23); color:var(--text-primary,#e5e7eb); font-size:12px; }
.loops-head { display:flex; align-items:center; gap:8px; min-height:48px; padding:7px 12px; border-bottom:1px solid var(--border-color,#34363d); }.loops-title { margin-right:8px; font-size:14px; font-weight:700; }.loops-select,.loops-input,.loops-textarea { min-height:30px; padding:4px 8px; border:1px solid var(--border-color,#3a3d45); border-radius:6px; background:var(--input-bg,rgba(255,255,255,.05)); color:inherit; font:inherit; outline:none; }.loops-select:focus,.loops-input:focus,.loops-textarea:focus { border-color:var(--accent,#4f8cff); }.loops-workflow-select { width:230px; }.loops-spacer { flex:1 1 auto; }.loops-btn { min-height:30px; padding:4px 10px; border:1px solid var(--border-color,#3a3d45); border-radius:6px; background:transparent; color:inherit; font:inherit; cursor:pointer; }.loops-btn svg { width:14px; height:14px; margin-right:5px; vertical-align:middle; }.loops-btn:hover { border-color:var(--accent,#4f8cff); }.loops-btn:disabled { opacity:.45; cursor:default; }.loops-btn-primary { border-color:var(--accent,#4f8cff); background:var(--accent,#4f8cff); color:var(--accent-foreground,#fff); }.loops-icon { display:flex; align-items:center; justify-content:center; width:30px; height:30px; padding:0; border:1px solid transparent; border-radius:6px; background:transparent; color:var(--text-secondary,#9a9faa); cursor:pointer; }.loops-icon:hover { border-color:var(--border-color,#3a3d45); background:var(--hover-bg,rgba(255,255,255,.05)); color:var(--text-primary,#fff); }.loops-icon svg { width:15px; height:15px; }.loops-state { color:var(--text-muted,#7f8590); font-size:10px; }.loops-state.dirty { color:#fbbf24; }.loops-state.ok { color:#34d399; }.loops-nav { display:flex; flex:0 0 43px; min-height:43px; padding:0 15px; gap:20px; border-bottom:1px solid var(--border-color,#34363d); }.loops-nav button { padding:0; border:0; border-bottom:2px solid transparent; background:transparent; color:var(--text-secondary,#9a9faa); font:inherit; cursor:pointer; }.loops-nav button.active { border-bottom-color:var(--accent,#4f8cff); color:var(--text-primary,#fff); font-weight:700; }.loops-body { flex:1 1 auto; min-width:0; min-height:0; overflow:hidden; }.loops-page { height:100%; min-height:0; overflow:auto; padding:18px; }.loops-builder { display:grid; grid-template-columns:205px minmax(360px,1fr) 300px; height:100%; min-width:0; min-height:0; }.loops-palette,.loops-inspector { min-width:0; min-height:0; overflow:auto; background:var(--editor-surface,#1b1d23); }.loops-palette { border-right:1px solid var(--border-color,#34363d); padding:10px 8px; }.loops-inspector { border-left:1px solid var(--border-color,#34363d); padding:12px; }.loops-panel-title { margin:3px 5px 9px; color:var(--text-muted,#7f8590); font-size:9px; font-weight:750; text-transform:uppercase; }.loops-node-add { display:flex; align-items:center; gap:9px; width:100%; min-height:44px; margin-bottom:4px; padding:6px 7px; border:1px solid transparent; border-radius:6px; background:transparent; color:inherit; text-align:left; cursor:pointer; }.loops-node-add:hover { border-color:var(--border-color,#3a3d45); background:var(--hover-bg,rgba(255,255,255,.045)); }.loops-node-mark { width:7px; height:28px; flex:0 0 auto; border-radius:3px; background:var(--node-color,#60a5fa); }.loops-node-add strong { display:block; font-size:11px; }.loops-node-add span:last-child { display:block; margin-top:2px; color:var(--text-muted,#7f8590); font-size:9px; }.loops-center { display:flex; flex-direction:column; min-width:0; min-height:0; background:#181a20; }.loops-canvas-tools { display:flex; align-items:center; gap:5px; min-height:39px; padding:5px 8px; border-bottom:1px solid var(--border-color,#34363d); background:var(--editor-surface,#1b1d23); }.loops-canvas { position:relative; flex:1 1 auto; min-width:0; min-height:260px; overflow:hidden; background-color:#181a20; background-image:radial-gradient(circle,#3a3e47 1px,transparent 1px); background-size:20px 20px; }.loops-drawer { flex:0 0 150px; min-height:100px; overflow:auto; border-top:1px solid var(--border-color,#34363d); background:var(--editor-surface,#1b1d23); }.loops-drawer-head { position:sticky; top:0; display:flex; align-items:center; min-height:34px; padding:6px 10px; border-bottom:1px solid var(--border-color,#34363d); background:var(--editor-surface,#1b1d23); color:var(--text-muted,#7f8590); font-size:9px; font-weight:750; text-transform:uppercase; }.loops-findings { padding:5px 10px 12px; }.loops-finding { display:grid; grid-template-columns:58px minmax(0,1fr); gap:8px; padding:6px 0; border-bottom:1px solid var(--border-color,#303239); }.loops-finding:last-child { border-bottom:0; }.loops-finding-level { font-size:9px; font-weight:750; text-transform:uppercase; }.loops-finding-level.error { color:#f87171; }.loops-finding-level.warning { color:#fbbf24; }.loops-finding-copy { color:var(--text-secondary,#a0a5af); line-height:1.4; }.loops-finding-copy strong { color:var(--text-primary,#e5e7eb); }.loops-empty { padding:14px; color:var(--text-muted,#7f8590); }.loops-field { display:flex; flex-direction:column; gap:5px; margin-bottom:11px; }.loops-field label { color:var(--text-muted,#7f8590); font-size:9px; font-weight:700; text-transform:uppercase; }.loops-textarea { min-height:70px; resize:vertical; line-height:1.45; }.loops-field-grid { display:grid; grid-template-columns:repeat(2,minmax(0,1fr)); gap:8px; }.loops-port-list { display:flex; flex-wrap:wrap; gap:4px; }.loops-port { padding:2px 5px; border:1px solid var(--border-color,#3a3d45); border-radius:4px; color:var(--text-secondary,#a0a5af); font-size:9px; }.loops-inspector-actions { display:flex; gap:6px; margin-top:14px; }.loops-table { width:100%; border-collapse:collapse; }.loops-table th { padding:8px 10px; border-bottom:1px solid var(--border-color,#34363d); color:var(--text-muted,#7f8590); font-size:9px; text-align:left; text-transform:uppercase; }.loops-table td { padding:9px 10px; border-bottom:1px solid var(--border-color,#303239); color:var(--text-secondary,#a0a5af); }.loops-table strong { color:var(--text-primary,#e5e7eb); }.loops-run-state { display:inline-flex; align-items:center; gap:6px; text-transform:capitalize; }.loops-run-dot { width:8px; height:8px; border-radius:50%; background:#737985; }.loops-run-dot.running { border:2px solid rgba(52,211,153,.3); border-top-color:#34d399; background:transparent; animation:loops-spin .8s linear infinite; }.loops-run-dot.completed { background:#60a5fa; }.loops-run-dot.failed,.loops-run-dot.cancelled { background:#f87171; }.loops-run-dot.waiting_for_approval { background:#fbbf24; }.loops-library-grid { display:grid; grid-template-columns:repeat(auto-fill,minmax(220px,1fr)); gap:10px; }.loops-library-item { display:flex; gap:10px; padding:12px; border:1px solid var(--border-color,#34363d); border-radius:7px; background:var(--bg-secondary,#202229); }.loops-library-item h3 { margin:0; font-size:12px; }.loops-library-item p { margin:4px 0 0; color:var(--text-muted,#7f8590); font-size:10px; }.loops-summary-grid { display:grid; grid-template-columns:repeat(auto-fit,minmax(160px,1fr)); gap:10px; margin-bottom:16px; }.loops-summary { padding:13px; border:1px solid var(--border-color,#34363d); border-radius:7px; background:var(--bg-secondary,#202229); }.loops-summary label { display:block; color:var(--text-muted,#7f8590); font-size:9px; text-transform:uppercase; }.loops-summary strong { display:block; margin-top:6px; font-size:18px; }.loops-toast { position:absolute; z-index:30; left:50%; bottom:16px; transform:translateX(-50%); padding:7px 12px; border:1px solid var(--border-color,#444750); border-radius:6px; background:#262931; box-shadow:0 8px 25px rgba(0,0,0,.4); }.loops-toast.error { color:#f87171; }@keyframes loops-spin{to{transform:rotate(360deg)}}
@media(max-width:980px){.loops-builder{grid-template-columns:170px minmax(320px,1fr)}.loops-inspector{position:absolute;right:0;top:91px;bottom:0;width:300px;z-index:8;box-shadow:-12px 0 30px rgba(0,0,0,.32)}}
`;
    document.head.appendChild(style);
  }

  function nodeColor(kind) {
    return ({ trigger: '#34d399', agent: '#a78bfa', action: '#60a5fa', decision: '#fbbf24', human_approval: '#f87171', transform: '#22d3ee', retry: '#fb923c', parallel: '#e879f9', subflow: '#818cf8', output: '#2dd4bf' })[kind] || '#60a5fa';
  }

  function createNode(type) {
    const [kind, label, , inputs, outputs] = type;
    return {
      id: uid(kind), kind, name: label,
      inputs: inputs.map(([id, data_type]) => ({ id, data_type, required: true })),
      outputs: outputs.map(([id, data_type]) => ({ id, data_type, required: false })),
      config: { skills: [], tools: [], access_preset: 'read_only' },
      permissions: [], permission_layers: [], model_policy: kind === 'agent' ? { kind: 'local', provider: null, model: null } : null,
      timeout_seconds: ['trigger', 'output'].includes(kind) ? null : 300,
      max_retries: kind === 'retry' ? 1 : 0,
    };
  }

  function starterWorkflow() {
    const id = `workflow-${Date.now()}`;
    const trigger = createNode(NODE_TYPES[0]);
    const output = createNode(NODE_TYPES[NODE_TYPES.length - 1]);
    trigger.outputs[0].data_type = 'any';
    output.inputs[0].data_type = 'any';
    return {
      schema_version: 1, id, version: 1, name: 'New Agent Loop', description: '', project: null, status: 'draft',
      limits: { max_duration_seconds: 1800, max_node_executions: 100, max_agent_calls: null, max_tokens: null, max_cost_usd: null, on_budget_exhausted: 'fail' },
      governance: { require_frontier_approval: true, require_independent_review: false, require_delivery_evidence: false, independent_review: null, allowed_providers: [], permission_layers: [], model_rates: [] },
      nodes: [trigger, output],
      connections: [{ id: uid('connection'), from_node: trigger.id, from_port: 'event', to_node: output.id, to_port: 'result' }],
      presentation: { nodes: { [trigger.id]: { x: 100, y: 150, collapsed: false }, [output.id]: { x: 440, y: 150, collapsed: false } }, viewport_x: 0, viewport_y: 0, zoom: 1 },
      created_at: '', updated_at: '',
    };
  }

  function createPanel(tabId, parent, opts) {
    injectStyles();
    const label = `loops-${Date.now().toString(36)}-${++counter}`;
    const pane = document.createElement('div');
    pane.className = 'loops';
    pane.innerHTML = `<header class="loops-head"><span class="loops-title">Agent Loops</span><select class="loops-select loops-workflow-select" aria-label="Agent Loop"></select><span class="loops-state"></span><span class="loops-spacer"></span><button class="loops-icon loops-refresh" title="Refresh" aria-label="Refresh">${ICON.refresh}</button><button class="loops-btn loops-clone">Clone</button><button class="loops-btn loops-validate">Validate</button><button class="loops-btn loops-activate">Activate</button><button class="loops-btn loops-save">${ICON.save} Save</button><button class="loops-btn loops-btn-primary loops-new">New Agent Loop</button></header><nav class="loops-nav">${VIEWS.map(([id, name]) => `<button data-loops-view="${id}"${id === 'workflows' ? ' class="active"' : ''}>${name}</button>`).join('')}</nav><main class="loops-body"></main>`;
    parent.appendChild(pane);
    const $ = (selector) => pane.querySelector(selector);
    const state = { view: opts?.view || 'workflows', requestedWorkflowId: opts?.workflowId || '', workflows: [], definition: null, persisted: false, dirty: false, selectedNode: null, editor: null, runs: [], findings: [], estimate: null, unlisten: null };

    function toast(message, error) {
      const node = document.createElement('div');
      node.className = `loops-toast${error ? ' error' : ''}`;
      node.textContent = String(message);
      pane.appendChild(node);
      setTimeout(() => node.remove(), 3500);
    }

    function paintState(text, className) {
      $('.loops-state').textContent = text || '';
      $('.loops-state').className = `loops-state${className ? ` ${className}` : ''}`;
    }

    function setDirty(value) {
      state.dirty = value;
      paintState(value ? 'Unsaved changes' : state.definition ? `v${state.definition.version}` : '', value ? 'dirty' : '');
    }

    function paletteHtml() {
      return `<div class="loops-panel-title">Node library</div>${NODE_TYPES.map((type) => `<button class="loops-node-add" data-add-kind="${type[0]}"><span class="loops-node-mark" style="--node-color:${nodeColor(type[0])}"></span><span><strong>${type[1]}</strong><span>${type[2]}</span></span></button>`).join('')}`;
    }

    function findingsHtml() {
      const estimate = state.estimate ? `<div class="loops-finding"><span class="loops-finding-level">Estimate</span><span class="loops-finding-copy"><strong>$${Number(state.estimate.cost_usd || 0).toFixed(4)}</strong> · ${(state.estimate.input_tokens || 0) + (state.estimate.output_tokens || 0)} tokens</span></div>` : '';
      if (!state.findings.length) return `${estimate}<div class="loops-empty">No audit findings.</div>`;
      return estimate + state.findings.map((item) => `<div class="loops-finding"><span class="loops-finding-level ${esc(item.severity)}"${item.severity === 'critical' ? ' style="color:#f87171"' : ''}>${esc(item.severity)}</span><span class="loops-finding-copy"><strong>${esc(item.code)}</strong>${item.node_id ? ` · ${esc(item.node_id)}` : ''}<br>${esc(item.message)}</span></div>`).join('');
    }

    function workflowInspectorHtml() {
      const value = state.definition || starterWorkflow();
      const governance = value.governance || {};
      return `<div class="loops-panel-title">Agent Loop</div>
        <div class="loops-field"><label>Name</label><input class="loops-input loops-flow-name" value="${esc(value.name)}"></div>
        <div class="loops-field"><label>Description</label><textarea class="loops-textarea loops-flow-description">${esc(value.description || '')}</textarea></div>
        <div class="loops-field-grid"><div class="loops-field"><label>Duration (seconds)</label><input class="loops-input loops-flow-duration" type="number" min="1" value="${esc(value.limits.max_duration_seconds)}"></div><div class="loops-field"><label>Node executions</label><input class="loops-input loops-flow-executions" type="number" min="1" value="${esc(value.limits.max_node_executions)}"></div></div>
        <div class="loops-field-grid"><div class="loops-field"><label>Agent calls</label><input class="loops-input loops-flow-agent-calls" type="number" min="1" value="${value.limits.max_agent_calls ?? ''}"></div><div class="loops-field"><label>Token limit</label><input class="loops-input loops-flow-tokens" type="number" min="1" value="${value.limits.max_tokens ?? ''}"></div></div>
        <div class="loops-field-grid"><div class="loops-field"><label>Cost limit (USD)</label><input class="loops-input loops-flow-cost" type="number" min="0" step="0.01" value="${value.limits.max_cost_usd ?? ''}"></div><div class="loops-field"><label>Budget exhaustion</label><select class="loops-select loops-flow-budget-action"><option value="fail"${value.limits.on_budget_exhausted !== 'pause' ? ' selected' : ''}>Fail run</option><option value="pause"${value.limits.on_budget_exhausted === 'pause' ? ' selected' : ''}>Pause for approval</option></select></div></div>
        <div class="loops-field"><label>Allowed providers</label><input class="loops-input loops-flow-providers" value="${esc((governance.allowed_providers || []).join(', '))}" placeholder="Empty allows configured providers"></div>
        <div class="loops-field"><label>Model rates · provider | model | input/M | output/M</label><textarea class="loops-textarea loops-flow-rates" placeholder="openrouter | model-id | 1.00 | 2.00">${esc((governance.model_rates || []).map((rate) => `${rate.provider} | ${rate.model} | ${rate.input_usd_per_million} | ${rate.output_usd_per_million}`).join('\n'))}</textarea></div>
        <div class="loops-field"><label><input class="loops-flow-frontier-approval" type="checkbox"${governance.require_frontier_approval !== false ? ' checked' : ''}> Frontier models require approval</label></div>
        <div class="loops-field"><label><input class="loops-flow-independent-review" type="checkbox"${governance.require_independent_review ? ' checked' : ''}> Require independent Agent review</label></div>
        ${governance.independent_review ? `<div class="loops-field"><label>Latest independent review</label><div class="loops-port-list"><span class="loops-port">${esc(governance.independent_review.approved ? 'Approved' : 'Rejected')}</span><span class="loops-port">${esc(governance.independent_review.reviewer)}</span></div><div class="loops-finding-copy">${esc(governance.independent_review.summary || '')}</div></div>` : ''}
        <div class="loops-field"><label><input class="loops-flow-delivery-evidence" type="checkbox"${governance.require_delivery_evidence ? ' checked' : ''}> Require test, review, and release evidence</label></div>`;
    }

    function nodeInspectorHtml(node) {
      const config = node.config || {};
      const policy = node.model_policy || { kind: 'local', provider: '', model: '' };
      const ports = [...(node.inputs || []).map((item) => `in:${item.id} · ${item.data_type}`), ...(node.outputs || []).map((item) => `out:${item.id} · ${item.data_type}`)];
      const permissions = (node.permissions || []).map((item) => `${item.resource}:${item.action}`).join(', ');
      return `<div class="loops-panel-title">${esc(node.kind.replaceAll('_', ' '))}</div><div class="loops-field"><label>Name</label><input class="loops-input loops-node-name" value="${esc(node.name)}"></div><div class="loops-field"><label>Contracts</label><div class="loops-port-list">${ports.map((item) => `<span class="loops-port">${esc(item)}</span>`).join('')}</div></div><div class="loops-field-grid"><div class="loops-field"><label>Timeout</label><input class="loops-input loops-node-timeout" type="number" min="1" value="${node.timeout_seconds ?? ''}"></div><div class="loops-field"><label>Retries</label><input class="loops-input loops-node-retries" type="number" min="0" value="${node.max_retries || 0}"></div></div><div class="loops-field"><label>Model policy</label><select class="loops-select loops-node-policy"><option value="local"${policy.kind === 'local' ? ' selected' : ''}>Local</option><option value="balanced"${policy.kind === 'balanced' ? ' selected' : ''}>Balanced</option><option value="frontier"${policy.kind === 'frontier' ? ' selected' : ''}>Frontier</option><option value="fixed"${policy.kind === 'fixed' ? ' selected' : ''}>Fixed</option></select></div><div class="loops-field-grid"><div class="loops-field"><label>Provider</label><input class="loops-input loops-node-provider" value="${esc(policy.provider || '')}"></div><div class="loops-field"><label>Model</label><input class="loops-input loops-node-model" value="${esc(policy.model || '')}"></div></div><div class="loops-field-grid"><div class="loops-field"><label>Expected input tokens</label><input class="loops-input loops-node-input-tokens" type="number" min="0" value="${config.expected_input_tokens ?? ''}"></div><div class="loops-field"><label>Expected output tokens</label><input class="loops-input loops-node-output-tokens" type="number" min="0" value="${config.expected_output_tokens ?? ''}"></div></div><div class="loops-field"><label>Access</label><select class="loops-select loops-node-access"><option value="read_only"${config.access_preset === 'read_only' ? ' selected' : ''}>Read only</option><option value="draft_docs"${config.access_preset === 'draft_docs' ? ' selected' : ''}>Draft documents</option><option value="update_docs"${config.access_preset === 'update_docs' ? ' selected' : ''}>Update documents</option><option value="plan_execution"${config.access_preset === 'plan_execution' ? ' selected' : ''}>Plan execution</option><option value="launch_agents"${config.access_preset === 'launch_agents' ? ' selected' : ''}>Launch Agents</option><option value="full_project"${config.access_preset === 'full_project' ? ' selected' : ''}>Full project access</option></select></div><div class="loops-field"><label>Requested permissions</label><input class="loops-input loops-node-permissions" value="${esc(permissions)}" placeholder="vault:read, ticket:update"></div><div class="loops-field"><label>Skills</label><input class="loops-input loops-node-skills" value="${esc((config.skills || []).join(', '))}"></div><div class="loops-field"><label>Tools</label><input class="loops-input loops-node-tools" value="${esc((config.tools || []).join(', '))}"></div><div class="loops-inspector-actions"><button class="loops-icon loops-delete-node" title="Delete node" aria-label="Delete node">${ICON.trash}</button><span class="loops-spacer"></span><button class="loops-btn loops-btn-primary loops-apply-node">Apply</button></div>`;
    }

    function renderInspector() {
      const inspector = $('.loops-inspector');
      if (!inspector) return;
      inspector.innerHTML = state.selectedNode ? nodeInspectorHtml(state.selectedNode) : workflowInspectorHtml();
      if (!state.selectedNode) {
        inspector.querySelectorAll('input,textarea,select').forEach((input) => { input.oninput = () => setDirty(true); input.onchange = () => setDirty(true); });
        return;
      }
      inspector.querySelector('.loops-delete-node').onclick = async () => { await state.editor.removeSelected(); state.selectedNode = null; renderInspector(); setDirty(true); };
      inspector.querySelector('.loops-apply-node').onclick = async () => {
        const list = (selector) => inspector.querySelector(selector).value.split(',').map((value) => value.trim()).filter(Boolean);
        const kind = inspector.querySelector('.loops-node-policy').value;
        const updated = clone(state.selectedNode);
        updated.name = inspector.querySelector('.loops-node-name').value.trim() || updated.name;
        updated.timeout_seconds = inspector.querySelector('.loops-node-timeout').value ? Number(inspector.querySelector('.loops-node-timeout').value) : null;
        updated.max_retries = Number(inspector.querySelector('.loops-node-retries').value || 0);
        const permission = (value) => { const split = value.lastIndexOf(':'); return split > 0 ? { resource: value.slice(0, split).trim(), action: value.slice(split + 1).trim() } : null; };
        updated.config = { ...(updated.config || {}), access_preset: inspector.querySelector('.loops-node-access').value, skills: list('.loops-node-skills'), tools: list('.loops-node-tools'), expected_input_tokens: Number(inspector.querySelector('.loops-node-input-tokens').value || 0), expected_output_tokens: Number(inspector.querySelector('.loops-node-output-tokens').value || 0) };
        updated.permissions = list('.loops-node-permissions').map(permission).filter(Boolean);
        updated.model_policy = { kind, provider: inspector.querySelector('.loops-node-provider').value.trim() || null, model: inspector.querySelector('.loops-node-model').value.trim() || null };
        await state.editor.updateNode(updated.id, updated);
        state.selectedNode = updated;
        setDirty(true);
      };
    }

    function syncWorkflowFields(definition) {
      const name = $('.loops-flow-name');
      if (!name) return definition;
      definition.name = name.value.trim() || definition.name;
      definition.description = $('.loops-flow-description').value;
      definition.limits.max_duration_seconds = Number($('.loops-flow-duration').value || 1800);
      definition.limits.max_node_executions = Number($('.loops-flow-executions').value || 100);
      definition.limits.max_agent_calls = $('.loops-flow-agent-calls').value === '' ? null : Number($('.loops-flow-agent-calls').value);
      definition.limits.max_tokens = $('.loops-flow-tokens').value === '' ? null : Number($('.loops-flow-tokens').value);
      definition.limits.max_cost_usd = $('.loops-flow-cost').value === '' ? null : Number($('.loops-flow-cost').value);
      definition.limits.on_budget_exhausted = $('.loops-flow-budget-action').value;
      definition.governance = definition.governance || {};
      definition.governance.allowed_providers = $('.loops-flow-providers').value.split(',').map((value) => value.trim()).filter(Boolean);
      definition.governance.model_rates = $('.loops-flow-rates').value.split('\n').map((line) => line.split('|').map((value) => value.trim())).filter((parts) => parts.length === 4 && parts[0] && parts[1]).map(([provider, model, input, output]) => ({ provider, model, input_usd_per_million: Number(input || 0), output_usd_per_million: Number(output || 0) }));
      definition.governance.require_frontier_approval = $('.loops-flow-frontier-approval').checked;
      definition.governance.require_independent_review = $('.loops-flow-independent-review').checked;
      definition.governance.require_delivery_evidence = $('.loops-flow-delivery-evidence').checked;
      return definition;
    }

    async function currentDefinition(forSave) {
      if (!state.definition || !state.editor) throw new Error('No Agent Loop is open');
      let definition = state.editor.serialize(state.definition);
      definition = syncWorkflowFields(definition);
      if (forSave) definition.version = state.persisted ? state.definition.version + 1 : 1;
      return definition;
    }

    async function mountBuilder() {
      $('.loops-body').innerHTML = `<div class="loops-builder"><aside class="loops-palette">${paletteHtml()}</aside><section class="loops-center"><div class="loops-canvas-tools"><button class="loops-icon loops-focus" title="Fit Agent Loop" aria-label="Fit Agent Loop">${ICON.focus}</button><span class="loops-state">${state.definition ? esc(state.definition.id) : ''}</span><span class="loops-spacer"></span><button class="loops-btn loops-start-run">Start run</button></div><div class="loops-canvas"></div><div class="loops-drawer"><div class="loops-drawer-head">Validation and run activity</div><div class="loops-findings">${findingsHtml()}</div></div></section><aside class="loops-inspector"></aside></div>`;
      state.editor?.destroy();
      state.editor = await window.XnautRete.createEditor($('.loops-canvas'), {
        onChange: () => setDirty(true),
        onSelect: (_id, node) => { state.selectedNode = node; renderInspector(); },
      });
      if (state.definition) await state.editor.load(state.definition);
      renderInspector();
      $('.loops-focus').onclick = () => state.editor.focus();
      $('.loops-start-run').onclick = startRun;
      pane.querySelectorAll('[data-add-kind]').forEach((button) => {
        button.onclick = async () => {
          const type = NODE_TYPES.find((item) => item[0] === button.dataset.addKind);
          state.selectedNode = await state.editor.addNode(createNode(type));
          renderInspector(); setDirty(true);
        };
      });
    }

    function runRows(runs) {
      if (!runs.length) return '<tr><td colspan="7" class="loops-empty">No Agent Loop runs.</td></tr>';
      return runs.map((run) => `<tr data-run-id="${esc(run.id)}"><td><span class="loops-run-state"><span class="loops-run-dot ${esc(run.status)}"${run.status === 'paused' ? ' style="background:#fbbf24"' : ''}></span>${esc(run.status.replaceAll('_', ' '))}</span></td><td><strong>${esc(run.workflow_id)}</strong><br>v${run.workflow_version}</td><td>${esc(run.project || '')}</td><td>${run.node_executions} / ${run.agent_calls || 0}</td><td>${run.total_tokens || 0}<br>$${Number(run.total_cost_usd || 0).toFixed(4)}</td><td>${esc(relativeTime(run.updated_at))}</td><td><button class="loops-btn loops-open-run" data-run-id="${esc(run.id)}">Open</button>${run.status === 'paused' ? ` <button class="loops-btn loops-resume-run" data-run-id="${esc(run.id)}">Resume</button>` : ''}</td></tr>`).join('');
    }

    async function renderRuns() {
      state.runs = await invoke('loops_run_list', { workflowId: null });
      $('.loops-body').innerHTML = `<div class="loops-page"><div class="loops-summary-grid"><div class="loops-summary"><label>Active</label><strong>${state.runs.filter((run) => ['running', 'queued'].includes(run.status)).length}</strong></div><div class="loops-summary"><label>Approvals</label><strong>${state.runs.filter((run) => run.status === 'waiting_for_approval').length}</strong></div><div class="loops-summary"><label>Paused</label><strong>${state.runs.filter((run) => run.status === 'paused').length}</strong></div><div class="loops-summary"><label>Completed</label><strong>${state.runs.filter((run) => run.status === 'completed').length}</strong></div><div class="loops-summary"><label>Failed</label><strong>${state.runs.filter((run) => run.status === 'failed').length}</strong></div></div><table class="loops-table"><thead><tr><th>State</th><th>Agent Loop</th><th>Project</th><th>Nodes / Agent calls</th><th>Tokens / Cost</th><th>Updated</th><th></th></tr></thead><tbody>${runRows(state.runs)}</tbody></table></div>`;
      pane.querySelectorAll('.loops-open-run').forEach((button) => { button.onclick = () => openRun(button.dataset.runId); });
      pane.querySelectorAll('.loops-resume-run').forEach((button) => { button.onclick = async () => { try { await invoke('loops_run_resume', { request: { run_id: button.dataset.runId, actor: 'xNAUT user', comment: 'Approved in Runs', override_budget: true } }); await renderRuns(); } catch (error) { toast(error, true); } }; });
    }

    async function renderApprovals() {
      state.runs = await invoke('loops_run_list', { workflowId: null });
      const approvals = state.runs.flatMap((run) => Object.values(run.nodes || {}).filter((node) => node.status === 'waiting_for_approval').map((node) => ({ run, node })));
      $('.loops-body').innerHTML = `<div class="loops-page"><table class="loops-table"><thead><tr><th>Agent Loop</th><th>Run</th><th>Approval node</th><th>Started</th><th>Decision</th></tr></thead><tbody>${approvals.length ? approvals.map(({ run, node }) => `<tr><td><strong>${esc(run.workflow_id)}</strong></td><td>${esc(run.id.slice(0, 8))}</td><td>${esc(node.node_id)}</td><td>${esc(relativeTime(node.started_at))}</td><td><button class="loops-btn loops-approval" data-approved="false" data-workflow="${esc(run.workflow_id)}" data-run="${esc(run.id)}" data-node="${esc(node.node_id)}">Reject</button> <button class="loops-btn loops-btn-primary loops-approval" data-approved="true" data-workflow="${esc(run.workflow_id)}" data-run="${esc(run.id)}" data-node="${esc(node.node_id)}">Approve</button></td></tr>`).join('') : '<tr><td colspan="5" class="loops-empty">No pending approvals.</td></tr>'}</tbody></table></div>`;
      pane.querySelectorAll('.loops-approval').forEach((button) => { button.onclick = async () => { try { const approved = button.dataset.approved === 'true'; if (button.dataset.workflow === 'system-ticket-triage') await invoke('ticket_triage_decide', { runId: button.dataset.run, actor: 'xNAUT user', approved, comment: '' }); else await invoke('loops_run_approve', { request: { run_id: button.dataset.run, node_id: button.dataset.node, actor: 'xNAUT user', approved, comment: '' } }); await renderApprovals(); } catch (error) { toast(error, true); } }; });
    }

    async function renderCosts() {
      state.runs = await invoke('loops_run_list', { workflowId: null });
      const usage = state.runs.flatMap((run) => Object.values(run.nodes || {}).map((node) => ({ run, node, usage: node.usage || {} })).filter((item) => item.usage.input_tokens || item.usage.output_tokens || item.usage.cost_usd));
      const totalTokens = state.runs.reduce((sum, run) => sum + (run.total_tokens || 0), 0);
      const totalCost = state.runs.reduce((sum, run) => sum + Number(run.total_cost_usd || 0), 0);
      $('.loops-body').innerHTML = `<div class="loops-page"><div class="loops-summary-grid"><div class="loops-summary"><label>Total cost</label><strong>$${totalCost.toFixed(4)}</strong></div><div class="loops-summary"><label>Total tokens</label><strong>${totalTokens}</strong></div><div class="loops-summary"><label>Agent calls</label><strong>${state.runs.reduce((sum, run) => sum + (run.agent_calls || 0), 0)}</strong></div></div><table class="loops-table"><thead><tr><th>Agent Loop / Run</th><th>Node / Agent</th><th>Provider</th><th>Model</th><th>Tokens</th><th>Cost</th></tr></thead><tbody>${usage.length ? usage.map(({ run, node, usage: item }) => `<tr><td><strong>${esc(run.workflow_id)}</strong><br>${esc(run.id.slice(0, 8))}</td><td>${esc(node.node_id)}<br>${esc(item.agent || '')}</td><td>${esc(item.provider || '')}</td><td>${esc(item.model || '')}</td><td>${(item.input_tokens || 0) + (item.output_tokens || 0)}</td><td>$${Number(item.cost_usd || 0).toFixed(4)}</td></tr>`).join('') : '<tr><td colspan="6" class="loops-empty">No model usage has been recorded.</td></tr>'}</tbody></table></div>`;
    }

    function renderLibrary() {
      $('.loops-body').innerHTML = `<div class="loops-page"><div class="loops-library-grid">${NODE_TYPES.map((type) => `<div class="loops-library-item"><span class="loops-node-mark" style="--node-color:${nodeColor(type[0])}"></span><div><h3>${esc(type[1])}</h3><p>${esc(type[2])}</p><div class="loops-port-list" style="margin-top:8px">${[...type[3].map((p) => `in:${p[0]}`), ...type[4].map((p) => `out:${p[0]}`)].map((port) => `<span class="loops-port">${esc(port)}</span>`).join('')}</div></div></div>`).join('')}</div></div>`;
    }

    function renderSettings() {
      $('.loops-body').innerHTML = `<div class="loops-page"><div style="max-width:760px"><div class="loops-summary-grid"><div class="loops-summary"><label>Schema</label><strong>v1</strong></div><div class="loops-summary"><label>Editor</label><strong>Rete 2</strong></div><div class="loops-summary"><label>Runtime</label><strong>Rust</strong></div></div><div class="loops-field-grid"><div class="loops-field"><label>Default duration</label><input class="loops-input" value="1800" disabled></div><div class="loops-field"><label>Default node executions</label><input class="loops-input" value="100" disabled></div></div></div></div>`;
    }

    async function showView(view) {
      state.view = view;
      pane.querySelectorAll('[data-loops-view]').forEach((button) => button.classList.toggle('active', button.dataset.loopsView === view));
      if (view !== 'workflows') { state.editor?.destroy(); state.editor = null; }
      if (view === 'workflows') await mountBuilder();
      else if (view === 'runs') await renderRuns();
      else if (view === 'approvals') await renderApprovals();
      else if (view === 'costs') await renderCosts();
      else if (view === 'library') renderLibrary();
      else renderSettings();
    }

    async function refreshWorkflows(selectId) {
      state.workflows = await invoke('loops_workflow_list', { project: null });
      const select = $('.loops-workflow-select');
      select.innerHTML = state.workflows.length ? state.workflows.map((item) => `<option value="${esc(item.id)}">${esc(item.name)} · v${item.latest_version}</option>`).join('') : '<option value="">No saved Agent Loops</option>';
      const target = selectId || state.definition?.id || state.workflows[0]?.id || '';
      select.value = state.workflows.some((item) => item.id === target) ? target : (state.workflows[0]?.id || '');
    }

    async function openWorkflow(id) {
      if (!id) return;
      state.definition = await invoke('loops_workflow_get', { id, version: null });
      state.persisted = true; state.selectedNode = null; state.findings = []; state.estimate = null; setDirty(false);
      if (state.view === 'workflows') await mountBuilder();
    }

    async function newWorkflow() {
      state.definition = starterWorkflow(); state.persisted = false; state.selectedNode = null; state.findings = []; state.estimate = null; setDirty(true);
      await showView('workflows');
    }

    async function cloneWorkflow() {
      if (!state.definition) return;
      if ((!state.persisted || state.dirty) && !(await saveWorkflow())) return;
      try {
        const suffix = Date.now().toString(36);
        const cloned = await invoke('loops_workflow_clone', { request: {
          source_id: state.definition.id,
          source_version: state.definition.version,
          new_id: `${state.definition.id}-copy-${suffix}`.slice(0, 80),
          new_name: `${state.definition.name} copy`,
          project: state.definition.project,
        } });
        state.definition = cloned; state.persisted = true; state.selectedNode = null; state.findings = []; state.estimate = null; setDirty(false);
        await refreshWorkflows(cloned.id);
        await showView('workflows');
        toast(`Cloned as ${cloned.name}`);
      } catch (error) { toast(error, true); }
    }

    async function validateWorkflow() {
      try {
        const definition = await currentDefinition(false);
        const report = await invoke('loops_workflow_audit', { definition });
        state.findings = report.findings || [];
        state.estimate = report.estimate || null;
        const target = $('.loops-findings'); if (target) target.innerHTML = findingsHtml();
        toast(report.valid ? 'Agent Loop is valid' : `${state.findings.length} validation finding${state.findings.length === 1 ? '' : 's'}`, !report.valid);
        return report.valid;
      } catch (error) { toast(error, true); return false; }
    }

    async function saveWorkflow() {
      try {
        const definition = await currentDefinition(true);
        state.definition = await invoke('loops_workflow_save', { definition });
        state.persisted = true; setDirty(false);
        await refreshWorkflows(state.definition.id);
        toast(`Saved ${state.definition.name} v${state.definition.version}`);
        return true;
      } catch (error) { toast(error, true); return false; }
    }

    async function activateWorkflow() {
      if (state.dirty && !(await saveWorkflow())) return;
      if (!(await validateWorkflow())) return;
      try {
        await invoke('loops_workflow_activate', { id: state.definition.id, version: state.definition.version });
        await refreshWorkflows(state.definition.id); paintState(`v${state.definition.version} active`, 'ok'); toast('Agent Loop activated');
      } catch (error) { toast(error, true); }
    }

    async function startRun() {
      try {
        const summary = state.workflows.find((item) => item.id === state.definition?.id);
        if (!summary?.active_version) { toast('Activate the Agent Loop before starting a run', true); return; }
        const run = await invoke('loops_run_start', { request: { workflow_id: state.definition.id, workflow_version: summary.active_version, project: state.definition.project, input: {} } });
        await state.editor?.setRun(run); toast(`Run ${run.id.slice(0, 8)} started`);
      } catch (error) { toast(error, true); }
    }

    async function openRun(id) {
      const run = await invoke('loops_run_get', { id });
      await openWorkflow(run.workflow_id); await showView('workflows'); await state.editor.setRun(run);
      state.findings = (await invoke('loops_run_events', { runId: id })).slice(-20).map((event) => ({ severity: event.event.includes('failed') ? 'error' : 'warning', code: event.event, node_id: event.node_id, message: event.timestamp }));
      $('.loops-findings').innerHTML = findingsHtml();
    }

    async function load() {
      try {
        await invoke('loops_workflow_seed_delivery');
        await refreshWorkflows();
        if (state.requestedWorkflowId && state.workflows.some((item) => item.id === state.requestedWorkflowId)) {
          const requested = state.requestedWorkflowId;
          state.requestedWorkflowId = '';
          await openWorkflow(requested);
          return;
        }
        if (!state.definition && state.workflows.length) {
          await openWorkflow($('.loops-workflow-select').value);
          if (state.view !== 'workflows') await showView(state.view);
        } else if (!state.definition) await newWorkflow();
        else await showView(state.view);
      } catch (error) { $('.loops-body').innerHTML = `<div class="loops-page loops-empty">${esc(error)}</div>`; }
    }

    pane.querySelectorAll('[data-loops-view]').forEach((button) => { button.onclick = () => showView(button.dataset.loopsView); });
    $('.loops-workflow-select').onchange = (event) => openWorkflow(event.target.value);
    $('.loops-new').onclick = newWorkflow;
    $('.loops-clone').onclick = cloneWorkflow;
    $('.loops-save').onclick = saveWorkflow;
    $('.loops-validate').onclick = validateWorkflow;
    $('.loops-activate').onclick = activateWorkflow;
    $('.loops-refresh').onclick = load;
    listen('loops://run-event', async () => { if (state.view === 'runs') await renderRuns(); if (state.view === 'approvals') await renderApprovals(); }).then((unlisten) => { state.unlisten = unlisten; }).catch(() => {});
    load();
    const entry = { kind: 'loops', label, pane, refresh: load, state };
    panes.set(label, entry);
    return entry;
  }

  function destroyPanel(label) {
    const entry = panes.get(label);
    if (!entry) return;
    entry.state.editor?.destroy();
    try { entry.state.unlisten?.(); } catch (_) { /* already removed */ }
    entry.pane.remove(); panes.delete(label);
  }

  window.xnautCreateLoopsPanel = createPanel;
  window.xnautDestroyLoopsPanel = destroyPanel;
})();
