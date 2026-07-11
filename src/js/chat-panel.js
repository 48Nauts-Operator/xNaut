// Chat panes — v1.6 native LLM chat.
//
// Architecture mirrors markdown-pane.js: an IIFE that registers panes in a
// Map, exports window.xnaut* functions, and builds pure-DOM panes inside the
// parent webview. A chat pane talks to the Rust side via chat_send (with
// streaming deltas over "chat://chunk"/"chat://done"/"chat://brain" events)
// and can drive project/task scaffolding through structured JSON actions
// detected in assistant replies.
(function () {
  'use strict';

  const $ = (id) => document.getElementById(id);

  // Lazy Tauri accessors — __TAURI__ may not be injected yet at parse time.
  const invoke = (...a) => window.__TAURI__.core.invoke(...a);
  const listen = (...a) => window.__TAURI__.event.listen(...a);

  // label -> entry { kind, label, pane, ... }
  const panes = new Map();
  let labelCounter = 0;
  let activeWorkspaceContext = null;

  window.xnautSetAgentWorkspaceContext = function(context) {
    activeWorkspaceContext = context && typeof context === 'object' ? context : null;
  };
  window.xnautClearAgentWorkspaceContext = function(owner) {
    if (!owner || activeWorkspaceContext?.owner === owner) activeWorkspaceContext = null;
  };
  window.xnautGetAgentWorkspaceContext = function() {
    if (!activeWorkspaceContext) return null;
    if (typeof activeWorkspaceContext.isActive === 'function' && !activeWorkspaceContext.isActive()) return null;
    return activeWorkspaceContext;
  };
  function nextLabel() {
    labelCounter += 1;
    return `chat-${Date.now().toString(36)}-${labelCounter}`;
  }

  function newRequestId() {
    if (window.crypto && typeof window.crypto.randomUUID === 'function') {
      return window.crypto.randomUUID();
    }
    return `req-${Date.now()}-${Math.floor(Math.random() * 1e9)}`;
  }

  function escapeText(s) {
    return String(s).replace(/[&<>"']/g, (c) => ({ '&':'&amp;', '<':'&lt;', '>':'&gt;', '"':'&quot;', "'":'&#39;' }[c]));
  }

  // ----------------------------------------------- persistent history (localStorage)
  // Keyed by project path for project/plan chats, else 'default'. Survives tab
  // close and app restart. Capped to keep storage bounded.
  const HIST_PREFIX = 'xnaut-chat-history:';
  function loadChatHistory(key) {
    try { return JSON.parse(localStorage.getItem(HIST_PREFIX + key)) || []; } catch (_) { return []; }
  }
  function saveChatHistory(entry) {
    if (!entry || !entry.chatKey) return;
    try {
      const clipped = entry.history.slice(-200);
      localStorage.setItem(HIST_PREFIX + entry.chatKey, JSON.stringify(clipped));
      document.dispatchEvent(new CustomEvent('xnaut:chat-history-changed', {
        detail: { chatKey: entry.chatKey, history: clipped },
      }));
    } catch (_) { /* quota */ }
  }

  function getChatHistory(chatKey) {
    return loadChatHistory(chatKey || 'default');
  }

  function persistChatHistory(chatKey, history) {
    const key = chatKey || 'default';
    const clipped = (Array.isArray(history) ? history : []).slice(-200);
    localStorage.setItem(HIST_PREFIX + key, JSON.stringify(clipped));
    document.dispatchEvent(new CustomEvent('xnaut:chat-history-changed', {
      detail: { chatKey: key, history: clipped },
    }));
    return clipped;
  }

  function renderStoredHistory(entry) {
    if (!entry || !entry.listEl) return;
    entry.listEl.innerHTML = '';
    (entry.history || []).forEach((m) => {
      if (m.role === 'user') appendMessage(entry, 'user', m.display || m.content);
      else if (m.role === 'assistant') appendMessage(entry, 'assistant', m.content);
    });
  }

  function setChatHistory(chatKey, history) {
    const key = chatKey || 'default';
    const clipped = persistChatHistory(key, history);
    panes.forEach((entry) => {
      if (entry.chatKey !== key) return;
      entry.history = clipped.slice();
      entry.activeRequestId = null;
      entry.liveRow = null;
      entry.liveBody = null;
      entry.liveText = '';
      entry.busy = false;
      entry.expectingVaultAction = false;
      renderStoredHistory(entry);
    });
    return clipped;
  }

  function clearChatHistory(chatKey) {
    return setChatHistory(chatKey || 'default', []);
  }

  // ---------------------------------------------------------------- styles

  function injectStyles() {
    if ($('chat-panel-styles')) return;
    const style = document.createElement('style');
    style.id = 'chat-panel-styles';
    style.textContent = `
      .chatp-bar {
        display: flex; align-items: center; gap: 8px; flex: 0 0 auto;
        padding: 6px 10px;
        border-bottom: 1px solid var(--border-color, #333);
      }
      .chatp-title { font-size: 13px; font-weight: 600; color: var(--text-primary, #ddd); }
      .chatp-model { font-size: 11px; color: var(--text-muted, #888); }
      .chatp-brain {
        font-size: 11px; padding: 1px 7px; border-radius: 999px;
        border: 1px solid var(--border-color, #333);
      }
      .chatp-brain-ok { border-color: #34d399; }
      .chatp-brain-bad { border-color: #f87171; }
      .chatp-spacer { flex: 1 1 auto; }
      .chatp-list {
        flex: 1 1 0; min-height: 0; overflow-y: auto;
        display: flex; flex-direction: column; gap: 10px; padding: 12px;
      }
      .chatp-msg { display: flex; flex-direction: column; gap: 4px; max-width: 85%; position: relative; }
      .chatp-copy {
        position: absolute; top: 3px; right: 3px; opacity: 0;
        background: var(--editor-surface, #1e1e1e); border: 1px solid var(--border-color, #333);
        border-radius: 5px; color: var(--text-muted, #8a8f98); cursor: pointer; padding: 3px;
        display: flex; transition: opacity .12s, color .12s;
      }
      .chatp-msg:hover .chatp-copy { opacity: .9; }
      .chatp-copy:hover { color: var(--text-primary, #fff); }
      .chatp-copy svg { width: 12px; height: 12px; }
      .chatp-msg-user { align-self: flex-end; }
      .chatp-msg-assistant { align-self: flex-start; }
      .chatp-msg-system { align-self: center; max-width: 100%; }
      .chatp-body {
        padding: 8px 12px; border-radius: var(--radius-md, 8px);
        border: 1px solid var(--border-color, #333);
        background: var(--editor-surface, #1e1e1e);
        font-size: 13px; line-height: 1.5; color: var(--text-primary, #ddd);
        word-break: break-word; overflow-wrap: anywhere;
      }
      .chatp-body.xnaut-md { font-size:13px; line-height:1.58; }
      .chatp-body.xnaut-md h1 { font-size:18px; margin:0 0 14px; padding-bottom:8px; }
      .chatp-body.xnaut-md h2 { font-size:16px; margin:24px 0 9px; padding-bottom:5px; }
      .chatp-body.xnaut-md h3 { font-size:14px; margin:20px 0 7px; color:var(--text-primary, #e8eaf0); }
      .chatp-body.xnaut-md h4 { font-size:13px; margin:16px 0 6px; }
      .chatp-body.xnaut-md p { margin:0 0 10px; }
      .chatp-body.xnaut-md ul, .chatp-body.xnaut-md ol { margin:0 0 11px; padding-left:21px; }
      .chatp-body.xnaut-md li { margin:4px 0; }
      .chatp-body.xnaut-md table { display:block; max-width:100%; overflow-x:auto; margin:12px 0 16px; }
      .chatp-body.xnaut-md pre { max-width:100%; box-sizing:border-box; }
      .chatp-msg-user .chatp-body {
        background: var(--accent-surface, #25324a);
        border-color: var(--accent-border, #3a4a68);
      }
      .chatp-msg-system .chatp-body {
        background: transparent; border: none; font-size: 11px;
        font-style: italic; color: var(--text-muted, #888); padding: 2px 8px;
      }
      .chatp-code {
        background: var(--code-surface, #121212); border: 1px solid var(--border-color, #333);
        border-radius: 6px; padding: 8px 10px; overflow-x: auto; max-width: 100%;
        margin: 8px 0;
        font-family: var(--font-mono, ui-monospace, monospace); font-size: 12px;
        white-space: pre-wrap; overflow-wrap: anywhere; display: block;
      }
      .chatp-tool-status {
        display:flex; flex-direction:column; gap:4px; min-width:240px;
      }
      .chatp-tool-summary {
        font-size:12px; font-weight:600; color:var(--text-primary,#ddd);
      }
      .chatp-tool-results {
        display:flex; flex-direction:column; gap:3px;
      }
      .chatp-tool-status-line {
        font-size:12px; color:var(--text-secondary,#aaa);
      }
      .chatp-tool-status-line[data-error="1"] { color:#f87171; }
      .chatp-inline-code {
        background: var(--code-surface, #121212); border-radius: 4px; padding: 1px 4px;
        font-family: var(--font-mono, ui-monospace, monospace); font-size: 12px;
      }
      .chatp-brain-chip { font-size: 10px; color: var(--text-muted, #888); padding-left: 6px; }
      .chatp-error { color: #f87171; }
      .chatp-learning {
        display:flex; align-items:center; gap:7px; margin-top:5px;
        color:var(--text-muted, #888); font-size:11px;
      }
      .chatp-learning-btn {
        border:1px solid var(--border-color, #333); border-radius:6px;
        background:transparent; color:var(--text-secondary, #aaa);
        padding:4px 8px; font:inherit; cursor:pointer;
      }
      .chatp-learning-btn:hover { color:var(--text-primary, #ddd); border-color:var(--accent, #4f8cff); }
      .chatp-learning-btn[data-armed="1"] { color:var(--accent-foreground,#fff); background:var(--accent, #4f8cff); border-color:var(--accent, #4f8cff); }
      .chatp-learning-btn:disabled { opacity:.6; cursor:default; }
      .chatp-input-area {
        flex: 0 0 auto; display: flex; align-items: flex-end; gap: 8px;
        padding: 8px 10px; border-top: 1px solid var(--border-color, #333);
      }
      .chatp-input {
        flex: 1 1 auto; resize: none; outline: none;
        background: var(--input-surface, #161616); color: var(--text-primary, #ddd);
        border: 1px solid var(--border-color, #333); border-radius: var(--radius-md, 8px);
        padding: 6px 10px; font: inherit; font-size: 13px; line-height: 20px;
        max-height: 132px; overflow-y: auto;
      }
      .chatp-card {
        display: flex; flex-direction: column; gap: 6px; min-width: 240px;
      }
      .chatp-card-title { font-size: 13px; font-weight: 600; color: var(--text-primary, #ddd); }
      .chatp-card-row { display: flex; gap: 8px; font-size: 12px; }
      .chatp-card-key { color: var(--text-muted, #888); min-width: 70px; flex: 0 0 auto; }
      .chatp-card-val { color: var(--text-primary, #ddd); word-break: break-word; }
      .chatp-card-error { color: #f87171; font-size: 12px; }
      .chatp-card-actions { display: flex; gap: 8px; padding-top: 4px; }
      .chatp-btn {
        border: none; border-radius: 6px; padding: 4px 14px;
        font-size: 12px; cursor: pointer;
        background: var(--accent, #3b82f6); color: var(--accent-foreground,#fff);
      }
      .chatp-btn-dismiss {
        background: transparent; color: var(--text-muted, #aaa);
        border: 1px solid var(--border-color, #333);
      }
      .chatp-btn:disabled { opacity: 0.5; cursor: default; }
    `;
    document.head.appendChild(style);
  }

  // -------------------------------------------------------- markdown-lite

  // Escape HTML first, then: \`\`\` fences -> <pre><code>, `x` -> <code>,
  // **x** -> <strong>, \n -> <br> (fences excluded from the <br> pass).
  function renderMarkdownLite(raw) {
    const escaped = escapeText(raw);
    const stash = [];
    let html = escaped.replace(/```[a-zA-Z0-9_-]*\n?([\s\S]*?)```/g, (m, code) => {
      stash.push(`<pre class="chatp-code"><code>${code.replace(/\n$/, '')}</code></pre>`);
      return `@@@${stash.length - 1}@@@`;
    });
    html = html
      .replace(/`([^`\n]+)`/g, '<code class="chatp-inline-code">$1</code>')
      .replace(/\*\*([^*\n][^*]*?)\*\*/g, '<strong>$1</strong>')
      .replace(/\n/g, '<br>');
    return html.replace(/@@@(\d+)@@@/g, (m, i) => stash[Number(i)]);
  }

  function renderAssistantMarkdown(body, text) {
    if (window.xnautMarkdown && typeof window.xnautMarkdown.renderInto === 'function') {
      window.xnautMarkdown.renderInto(body, text).catch((e) => {
        console.error('[chat-panel] markdown render failed', e);
        body.innerHTML = renderMarkdownLite(text);
      });
      return;
    }
    body.innerHTML = renderMarkdownLite(text);
  }

  // ------------------------------------------------ scaffold action detect

  function parseLooseVaultAction(s) {
    const src = String(s || '').trim();
    const actionMatch = src.match(/["'](?:action|tool)["']\s*:\s*["']([^"']+)["']/i);
    const action = actionMatch && actionMatch[1];
    const known = ['vault_search', 'vault_read', 'vault_create', 'vault_write', 'vault_move', 'vault_tag'];
    if (!known.includes(action)) return null;
    const out = { action };
    const readStringField = (name) => {
      const re = new RegExp(`["']${name}["']\\s*:\\s*["']([^"']*)["']`, 'i');
      const m = src.match(re);
      return m ? m[1].replace(/\\n/g, '\n').replace(/\\"/g, '"') : '';
    };
    out.rel = readStringField('rel');
    out.query = readStringField('query');
    out.from = readStringField('from');
    out.to = readStringField('to');
    const contentMatch = src.match(/["']content["']\s*:\s*["']([\s\S]*)["']\s*}\s*$/i);
    if (contentMatch) out.content = contentMatch[1].replace(/\\n/g, '\n').replace(/\\"/g, '"');
    if ((action === 'vault_create' || action === 'vault_write') && (!out.rel || typeof out.content !== 'string')) return null;
    if ((action === 'vault_read' || action === 'vault_tag') && !out.rel) return null;
    if (action === 'vault_search' && !out.query) return null;
    if (action === 'vault_move' && (!out.from || !out.to)) return null;
    Object.keys(out).forEach((k) => {
      if (out[k] === '') delete out[k];
    });
    return out;
  }

  function tryParseAction(s) {
    try {
      const j = JSON.parse(s);
      const known = [
        'init_project', 'init_task', 'open_project',
        'vault_search', 'vault_read', 'vault_create', 'vault_write', 'vault_move', 'vault_tag', 'mcp_call', 'loop_create',
      ];
      if (j && typeof j === 'object' && !j.action && j.tool) j.action = j.tool;
      if (j && typeof j === 'object' && known.includes(j.action)) return j;
    } catch (_) {
      const loose = parseLooseVaultAction(s);
      if (loose) return loose;
    }
    return null;
  }

  function extractJsonObjects(text) {
    const src = String(text || '');
    const candidates = [];
    const parsedActions = parseGemmaToolCalls(src);
    const fenceRe = /```(?:json)?\s*\n?([\s\S]*?)```/gi;
    let m;
    while ((m = fenceRe.exec(src)) !== null) candidates.push(m[1].trim());

    let start = -1;
    let depth = 0;
    let inString = false;
    let escaped = false;
    for (let i = 0; i < src.length; i++) {
      const ch = src[i];
      if (inString) {
        if (escaped) escaped = false;
        else if (ch === '\\') escaped = true;
        else if (ch === '"') inString = false;
        continue;
      }
      if (ch === '"') {
        inString = true;
        continue;
      }
      if (ch === '{') {
        if (depth === 0) start = i;
        depth += 1;
      } else if (ch === '}' && depth > 0) {
        depth -= 1;
        if (depth === 0 && start >= 0) {
          candidates.push(src.slice(start, i + 1));
          start = -1;
        }
      }
    }

    const seen = new Set();
    return parsedActions.concat(candidates
      .map((c) => tryParseAction(c))
      .filter(Boolean))
      .filter((a) => {
        const key = JSON.stringify(a);
        if (seen.has(key)) return false;
        seen.add(key);
        return true;
      });
  }

  function parseGemmaToolCalls(text) {
    const out = [];
    const re = /<\|tool_call>call:([a-zA-Z_][\w]*)\{([\s\S]*?)\}<tool_call\|>/g;
    let m;
    while ((m = re.exec(String(text || ''))) !== null) {
      const action = m[1];
      if (!action.startsWith('vault_') && action !== 'mcp_call') continue;
      out.push({ action, ...parseGemmaToolArgs(m[2]) });
    }
    return out;
  }

  function parseGemmaToolArgs(raw) {
    const body = String(raw || '');
    const keys = ['query', 'rel', 'content', 'from', 'to', 'add', 'remove', 'server', 'tool', 'arguments'];
    const hits = [];
    for (const key of keys) {
      const re = new RegExp(`(?:^|,)\\s*${key}\\s*:`, 'g');
      let m;
      while ((m = re.exec(body)) !== null) {
        hits.push({ key, start: m.index, valueStart: re.lastIndex });
      }
    }
    hits.sort((a, b) => a.start - b.start);

    const args = {};
    for (let i = 0; i < hits.length; i++) {
      const hit = hits[i];
      const next = hits[i + 1];
      let value = body.slice(hit.valueStart, next ? next.start : body.length).trim();
      value = value.replace(/^,/, '').replace(/,$/, '').trim();
      args[hit.key] = parseGemmaToolValue(value);
    }
    return args;
  }

  function parseGemmaToolValue(raw) {
    let value = String(raw || '').trim();
    if ((value.startsWith('"') && value.endsWith('"')) || (value.startsWith("'") && value.endsWith("'"))) {
      value = value.slice(1, -1);
    } else if (value.startsWith('"') || value.startsWith("'")) {
      value = value.slice(1);
    }
    value = value.replace(/\\"/g, '"').replace(/\\n/g, '\n');
    if ((value.startsWith('[') && value.endsWith(']')) || (value.startsWith('{') && value.endsWith('}'))) {
      try { return JSON.parse(value); } catch (_) { /* keep string */ }
    }
    return value;
  }

  function detectScaffoldAction(text) {
    return extractJsonObjects(text)[0] || null;
  }

  function detectScaffoldActions(text) {
    return extractJsonObjects(text);
  }

  function latestUserNeedsVaultAction(history) {
    const last = [...(history || [])].reverse().find((m) => m.role === 'user');
    const text = String(last?.display || last?.content || '').toLowerCase();
    if (!text) return false;
    if (/^\s*(can|could|should|would)\s+(we|i)\b/.test(text)) return false;
    return /\b(create|make|add|write|save|update|revise|edit|move|rename|tag|read|open|search|find)\b/.test(text)
      && /\b(note|document|file|template|templates|vault|folder|tag|tags|research|concept|whitepaper|business|development|sprint|test|testing|\.md)\b/.test(text);
  }

  function latestUserWantsVaultMutation(history) {
    const last = [...(history || [])].reverse().find((m) => m.role === 'user');
    const text = String(last?.display || last?.content || '').toLowerCase();
    if (!text) return false;
    if (/^\s*(can|could|should|would)\s+(we|i)\b/.test(text)) return false;
    return /\b(create|make|add|write|save|update|revise|edit|move|rename|tag|import|copy)\b/.test(text)
      && /\b(note|document|file|template|templates|vault|folder|tag|tags|obsidian|markdown|\.md)\b/.test(text);
  }

  function latestUserRequestsVaultRead(history) {
    const last = [...(history || [])].reverse().find((m) => m.role === 'user');
    const text = String(last?.display || last?.content || '').toLowerCase();
    return /\b(read|open|inspect|review|show|look at)\b/.test(text)
      && /\b(note|document|file|vault|concept|business case|requirements|architecture|plan|review)\b/.test(text);
  }

  function vaultActionLabel(action) {
    const a = typeof action === 'string' ? action : action?.action;
    if (a === 'vault_search') return 'search notes';
    if (a === 'vault_read') return 'read note';
    if (a === 'vault_create') return 'create note';
    if (a === 'vault_write') return 'write note';
    if (a === 'vault_move') return 'move note';
    if (a === 'vault_tag') return 'update note tags';
    return 'vault action';
  }

  function normalizeVaultRel(value, vault) {
    let rel = String(value || '').trim().replace(/\\/g, '/');
    const vaultName = String(vault || '').trim();
    const prefixes = [`${vaultName}:`, `vault:${vaultName}:`].filter((prefix) => prefix !== ':');
    for (const prefix of prefixes) {
      if (rel.toLowerCase().startsWith(prefix.toLowerCase())) {
        rel = rel.slice(prefix.length).replace(/^\/+/, '');
        break;
      }
    }
    return rel.replace(/^\.\//, '');
  }

  function lastMessageIsVaultToolResult(history) {
    const last = (history || [])[history.length - 1];
    return last?.role === 'system' && String(last.content || '').startsWith('VAULT TOOL RESULTS:');
  }

  function lastVaultToolResultsHaveMutation(history) {
    const last = (history || [])[history.length - 1];
    if (!lastMessageIsVaultToolResult(history)) return false;
    return /\b(CREATED|WROTE|MOVED|TAGS UPDATED)\b/.test(String(last.content || ''));
  }

  function lastVaultToolResultsBlockedAction(history) {
    const last = (history || [])[history.length - 1];
    if (!lastMessageIsVaultToolResult(history)) return false;
    return /\b(TOOL ERROR|TOOL LIMIT)\b/.test(String(last.content || ''));
  }

  function isToolOnlyAssistantMessage(message) {
    if (!message || message.role !== 'assistant') return false;
    const actions = detectScaffoldActions(message.content || '');
    if (!actions.length) return false;
    const stripped = String(message.content || '')
      .replace(/<\|tool_call>call:[\s\S]*?<tool_call\|>/g, '')
      .replace(/```(?:json)?\s*\n?[\s\S]*?```/gi, '')
      .replace(/\{[\s\S]*?\}/g, '')
      .trim();
    return !stripped || actions.every((a) => a.action && (a.action.startsWith('vault_') || a.action === 'mcp_call' || a.action === 'loop_create'));
  }

  function compileAgentLoop(action) {
    const slug = (value, fallback) => String(value || fallback || 'node').toLowerCase()
      .replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 64) || fallback || 'node';
    const allowedKinds = new Set(['trigger', 'agent', 'action', 'decision', 'human_approval', 'transform', 'retry', 'parallel', 'subflow', 'output']);
    const sourceNodes = Array.isArray(action.nodes) ? action.nodes : [];
    if (sourceNodes.length < 2) throw new Error('Agent Loop requires at least two nodes');
    const ids = new Set();
    const normalized = sourceNodes.map((value, index) => {
      const raw = value && typeof value === 'object' ? value : {};
      let id = slug(raw.id || raw.name, `node-${index + 1}`);
      while (ids.has(id)) id = `${id}-${index + 1}`;
      ids.add(id);
      return Object.assign({}, raw, { id, kind: allowedKinds.has(raw.kind) ? raw.kind : 'action' });
    });
    const originalToId = new Map();
    sourceNodes.forEach((value, index) => {
      const raw = value && typeof value === 'object' ? value : {};
      originalToId.set(String(raw.id || ''), normalized[index].id);
      originalToId.set(String(raw.name || ''), normalized[index].id);
      originalToId.set(normalized[index].id, normalized[index].id);
    });
    const resolve = (value) => originalToId.get(String(value || '')) || slug(value, '');
    const routes = [];
    normalized.forEach((node) => {
      if (node.next) routes.push({ from: node.id, port: 'next', to: resolve(node.next) });
      Object.entries(node.branches || {}).forEach(([label, target]) => routes.push({ from: node.id, port: slug(label, 'route'), to: resolve(target) }));
    });
    for (const route of routes) {
      if (!ids.has(route.to)) throw new Error(`Unknown Agent Loop target: ${route.to}`);
    }
    const outgoing = new Map();
    routes.forEach((route) => { if (!outgoing.has(route.from)) outgoing.set(route.from, []); outgoing.get(route.from).push(route); });
    const nodes = normalized.map((node) => {
      const config = { access_preset: String(node.access || 'read_only') };
      if (node.kind === 'retry') config.max_cycles = Math.max(1, Number(node.max_cycles || 3));
      const permissions = (node.permissions || []).map((value) => {
        const [resource, actionName] = String(value).split(':');
        return resource && actionName ? { resource, action: actionName } : null;
      }).filter(Boolean);
      return {
        id: node.id,
        kind: node.kind,
        name: String(node.name || node.id),
        inputs: node.kind === 'trigger' ? [] : [{ id: 'input', data_type: 'loop', required: true }],
        outputs: node.kind === 'output' ? [] : (outgoing.get(node.id) || []).map((route) => ({ id: route.port, data_type: 'loop', required: true })),
        config,
        permissions,
        permission_layers: [],
        model_policy: node.kind === 'agent' ? { kind: ['local', 'balanced', 'frontier'].includes(node.model_policy) ? node.model_policy : 'balanced', provider: null, model: null } : null,
        timeout_seconds: ['trigger', 'output'].includes(node.kind) ? null : Math.max(1, Number(node.timeout_seconds || 1800)),
        max_retries: node.kind === 'retry' ? Math.max(1, Number(node.max_cycles || 3)) : 0,
      };
    });
    const connections = routes.map((route, index) => ({
      id: `connection-${index + 1}`,
      from_node: route.from,
      from_port: route.port,
      to_node: route.to,
      to_port: 'input',
    }));
    const adjacency = new Map();
    routes.forEach((route) => { if (!adjacency.has(route.from)) adjacency.set(route.from, []); adjacency.get(route.from).push(route.to); });
    const depth = new Map();
    const trigger = nodes.find((node) => node.kind === 'trigger') || nodes[0];
    depth.set(trigger.id, 0);
    const queue = [trigger.id];
    while (queue.length) {
      const current = queue.shift();
      for (const target of adjacency.get(current) || []) {
        if (depth.has(target)) continue;
        depth.set(target, depth.get(current) + 1);
        queue.push(target);
      }
    }
    const lanes = new Map();
    const presentationNodes = {};
    nodes.forEach((node, index) => {
      const column = depth.has(node.id) ? depth.get(node.id) : index;
      const lane = lanes.get(column) || 0;
      lanes.set(column, lane + 1);
      presentationNodes[node.id] = { x: 80 + column * 280, y: 80 + lane * 190, collapsed: false };
    });
    const name = String(action.name || 'Agent Loop').trim() || 'Agent Loop';
    return {
      schema_version: 1,
      id: `agent-loop-${slug(name, 'draft')}-${Date.now().toString(36)}`.slice(0, 80),
      version: 1,
      name,
      description: String(action.description || ''),
      project: action.project ? String(action.project) : null,
      status: 'draft',
      limits: Object.assign({ max_duration_seconds: 86400, max_node_executions: 100, max_agent_calls: 20, max_tokens: 250000, max_cost_usd: 50, on_budget_exhausted: 'pause' }, action.limits || {}),
      governance: { require_frontier_approval: true, require_independent_review: false, require_delivery_evidence: false, independent_review: null, allowed_providers: [], permission_layers: [], model_rates: [] },
      nodes,
      connections,
      presentation: { nodes: presentationNodes, viewport_x: 0, viewport_y: 0, zoom: 1 },
      created_at: '',
      updated_at: '',
    };
  }

  async function runLoopTool(entry, row, action) {
    const body = row.querySelector('.chatp-body');
    if (body) body.innerHTML = '<div class="chatp-tool-status"><div class="chatp-tool-summary">Compiling and validating Agent Loop...</div></div>';
    try {
      const definition = compileAgentLoop(action);
      const report = await invoke('loops_workflow_audit', { definition });
      if (!report.valid) {
        const findings = (report.findings || []).filter((item) => ['critical', 'error'].includes(item.severity));
        throw new Error(findings.map((item) => `${item.code}: ${item.message}`).join('; ') || 'Agent Loop validation failed');
      }
      const saved = await invoke('loops_workflow_save', { definition });
      const message = `Created draft Agent Loop **${saved.name}** with ${saved.nodes.length} nodes. It passed validation and is ready for review. It has not been activated.`;
      entry.history.push({ role: 'assistant', content: message });
      saveChatHistory(entry);
      renderAssistantMarkdown(body, message);
      window.xnautAttachLoopsTab?.({ workflowId: saved.id });
    } catch (error) {
      const message = `Agent Loop was not created: ${String(error)}`;
      entry.history.push({ role: 'assistant', content: message });
      saveChatHistory(entry);
      if (body) body.innerHTML = `<span class="chatp-error">${escapeText(message)}</span>`;
    }
  }

  async function runMcpTools(entry, row, actions) {
    const body = row.querySelector('.chatp-body');
    const results = [];
    for (const action of actions) {
      entry.toolRounds = (entry.toolRounds || 0) + 1;
      if (entry.toolRounds > 8) {
        results.push('MCP TOOL LIMIT: 8 calls used.');
        break;
      }
      const server = String(action.server || entry.mcpTools?.server || '').trim();
      const tool = String(action.tool || '').trim();
      if (!server || !tool) {
        results.push('MCP TOOL ERROR: server and tool are required.');
        continue;
      }
      try {
        if (body) body.innerHTML = `<div class="chatp-tool-status"><div class="chatp-tool-summary">Running ${escapeText(tool)}...</div></div>`;
        const result = await invoke('mcp_call_tool', { server, tool, arguments: action.arguments || {} });
        results.push(`MCP RESULT ${server}/${tool}:\n${JSON.stringify(result, null, 2).slice(0, 12000)}`);
      } catch (error) {
        results.push(`MCP TOOL ERROR ${server}/${tool}: ${String(error)}`);
      }
    }
    entry.history.push({ role: 'system', content: `MCP TOOL RESULTS:\n${results.join('\n\n')}` });
    saveChatHistory(entry);
    await complete(entry, row);
  }

  async function runVaultTools(entry, row, actions, opts) {
    opts = opts || {};
    const vault = entry.vaultTools.vault();
    const body = row.querySelector('.chatp-body');
    const lines = [];
    const renderStatus = () => {
      if (!body) return;
      const hasError = lines.some((l) => l.error);
      body.innerHTML = '<div class="chatp-tool-status">'
        + `<div class="chatp-tool-summary">${hasError ? 'Note action needs attention' : 'Updating notes...'}</div>`
        + '<div class="chatp-tool-results">'
        + lines.map((l) => `<div class="chatp-tool-status-line"${l.error ? ' data-error="1"' : ''}>${escapeText(l.text)}</div>`).join('')
        + '</div>'
        + '</div>';
      scrollToBottom(entry);
    };
    const cardLine = (text, error) => {
      lines.push({ text, error: !!error });
      renderStatus();
    };
    const results = [];
    const changedRels = [];
    try {
      for (const rawAction of actions) {
        const action = Object.assign({}, rawAction);
        for (const key of ['rel', 'from', 'to']) {
          if (action[key] != null) action[key] = normalizeVaultRel(action[key], vault);
        }
        if (entry.vaultTools.readOnly && !['vault_search', 'vault_read'].includes(action.action)) {
          const result = `TOOL ERROR (${vaultActionLabel(action)}): this Agent has read-only access to the active workspace Vault.`;
          results.push(result);
          cardLine(`${vaultActionLabel(action)} denied by read-only access`, true);
          continue;
        }
        const restrictedWriteRel = typeof entry.vaultTools.writeRel === 'function'
          ? String(entry.vaultTools.writeRel() || '')
          : String(entry.vaultTools.writeRel || '');
        if (restrictedWriteRel && action.action !== 'vault_search' && action.action !== 'vault_read'
          && (action.action !== 'vault_write' || action.rel !== restrictedWriteRel)) {
          const result = `TOOL ERROR (${vaultActionLabel(action)}): this Agent may write only ${restrictedWriteRel}.`;
          results.push(result);
          cardLine(`${vaultActionLabel(action)} denied outside the active document`, true);
          continue;
        }
        entry.toolRounds = (entry.toolRounds || 0) + 1;
        if (entry.toolRounds > 5) {
          const result = 'TOOL LIMIT: 5 calls used - answer the user with what you have.';
          results.push(result);
          cardLine('tool limit reached', true);
          break;
        }
        if (action.action === 'vault_search') {
          const hits = await invoke('vault_search', { vault, query: String(action.query || '') });
          const result = 'SEARCH RESULTS:\n' + (hits.length
            ? hits.map((h) => `- ${h.rel} - ${h.title}${h.snippet ? ' - ' + h.snippet : ''}`).join('\n')
            : '(no matches)');
          results.push(result);
          cardLine(`searched "${action.query}" - ${hits.length} hits`);
        } else if (action.action === 'vault_read') {
          const noteBody = await invoke('vault_note_read', { vault, rel: String(action.rel || '') });
          results.push(`CONTENT OF ${action.rel}:\n\n${noteBody.slice(0, 8000)}`);
          cardLine(`read ${action.rel}`);
        } else if (action.action === 'vault_create') {
          const rel = String(action.rel || '');
          await invoke('vault_note_create', { vault, rel, content: String(action.content || '') || null });
          results.push(`CREATED ${rel}.`);
          cardLine(`created ${rel}`);
          changedRels.push(rel);
        } else if (action.action === 'vault_write') {
          const rel = String(action.rel || '');
          const content = String(action.content || '');
          await invoke('vault_note_write', { vault, rel, content });
          if (typeof entry.vaultTools.onWrite === 'function') await entry.vaultTools.onWrite(rel, content);
          results.push(`WROTE ${rel}.`);
          cardLine(`updated ${rel}`);
          changedRels.push(rel);
        } else if (action.action === 'vault_move') {
          await invoke('vault_note_move', { vault, fromRel: String(action.from || ''), toRel: String(action.to || '') });
          results.push(`MOVED ${action.from} -> ${action.to}.`);
          cardLine(`moved ${action.from} -> ${action.to}`);
          changedRels.push(String(action.to || ''));
        } else if (action.action === 'vault_tag') {
          const rel = String(action.rel || '');
          let noteBody = await invoke('vault_note_read', { vault, rel });
          noteBody = applyTagEdit(noteBody, action.add || [], action.remove || []);
          await invoke('vault_note_write', { vault, rel, content: noteBody });
          results.push(`TAGS UPDATED on ${rel}.`);
          cardLine(`tagged ${rel}`);
          changedRels.push(rel);
        } else {
          results.push(`UNKNOWN TOOL ${action.action}`);
          cardLine(`unknown action ${action.action}`, true);
        }
      }
    } catch (e) {
      const action = actions[Math.max(0, (entry.toolRounds || 1) - 1)] || actions[0] || {};
      const label = vaultActionLabel(action);
      const result = `TOOL ERROR (${label}): ${String(e)}`;
      results.push(result);
      cardLine(`${label} failed: ${String(e).slice(0, 120)}`, true);
    }
    if (changedRels.length && entry.vaultTools.entry) {
      if (typeof entry.vaultTools.entry.refresh === 'function') {
        await entry.vaultTools.entry.refresh();
      }
      if (typeof entry.vaultTools.entry.openNote === 'function') {
        await entry.vaultTools.entry.openNote(changedRels[changedRels.length - 1]);
      }
    }
    entry.history.push({ role: 'system', content: 'VAULT TOOL RESULTS:\n' + results.join('\n\n') });
    saveChatHistory(entry);
    if (opts.summarize === false) {
      const finalText = opts.finalText || results.map((r) => r.replace(/\.$/, '.')).join('\n') || 'Note action finished.';
      if (body) body.innerHTML = renderMarkdownLite(finalText);
      entry.history.push({ role: 'assistant', content: finalText });
      saveChatHistory(entry);
      return;
    }
    await complete(entry, row);
  }

  async function runVaultTool(entry, row, action) {
    return runVaultTools(entry, row, [action]);
  }

  function applyTagEdit(body, add, remove) {
    const cur = new Set();
    let rest = body;
    const m = body.match(/^---\n([\s\S]*?)\n---\n?/);
    if (m) {
      rest = body.slice(m[0].length);
      const tagsLine = m[1].match(/^tags:\s*\[([^\]]*)\]/m);
      if (tagsLine) tagsLine[1].split(',').forEach((t) => { const c = t.trim(); if (c) cur.add(c.toLowerCase()); });
    }
    (add || []).forEach((t) => cur.add(String(t).toLowerCase().replace(/^#/, '')));
    (remove || []).forEach((t) => cur.delete(String(t).toLowerCase().replace(/^#/, '')));
    const fmBody = m ? m[1].replace(/^tags:.*(\n(\s+- .*))*$/m, '').trim() : '';
    const lines = ['---', `tags: [${[...cur].join(', ')}]`];
    if (fmBody) lines.push(fmBody);
    lines.push('---');
    return lines.join('\n') + '\n' + rest;
  }

  // Plan Mode: the planning agent emits the full living plan as one fenced
  // ```markdown block. Extract its content so the right-hand doc pane mirrors it.
  function extractPlanDoc(text) {
    const s = String(text);
    // Preferred: explicit sentinels — these never collide with the ```mermaid /
    // ```code fences that live INSIDE the plan (a ```markdown wrapper would be
    // truncated by the first inner closing fence).
    let m = s.match(/===PLAN DOCUMENT===\s*\n?([\s\S]*?)\n?===END PLAN DOCUMENT===/i);
    if (m) return m[1].trim();
    // Fallback for older replies: a ```markdown fence, greedy to the LAST ```
    // so nested fences inside the plan survive.
    m = s.match(/```(?:markdown|md)\s*\n([\s\S]*)```/i);
    if (m) return m[1].trim();
    return null;
  }
  // The chat bubble shows only the conversation — strip the plan block out.
  function stripPlanDoc(text) {
    return String(text)
      .replace(/===PLAN DOCUMENT===[\s\S]*?===END PLAN DOCUMENT===/i, '')
      .replace(/```(?:markdown|md)\s*\n[\s\S]*```/i, '')
      .replace(/\n{3,}/g, '\n\n')
      .trim();
  }

  // ----------------------------------------------------------- system prompt

  function buildSystemPrompt(settings, agents) {
    const labels = (settings.categories || []).map((c) => c.label).filter(Boolean);
    const hosts = (settings.forges || []).map((f) => f.base_url).filter(Boolean);
    const agentIds = (agents || []).map((a) => a.id).filter(Boolean);
    return (
      "You are xNaut's assistant. " +
      `Project categories: [${labels.join(', ')}]. ` +
      `Forge hosts by index: [${hosts.map((h, i) => `${i}: ${h}`).join(', ')}]. ` +
      `Agents: [${agentIds.join(', ')}]. ` +
      'When the user asks to create a project or task and you have gathered name ' +
      '(+ for projects: type, host, agent, baseline prompt), reply ONLY with a JSON object: ' +
      `{"action":"init_project","name":...,"type":<one of: [${labels.join(', ')}]>,` +
      `"forge_index":<index into [${hosts.join(', ')}]>,` +
      `"agent":<one of: [${agentIds.join(', ')}]>,"baseline_prompt":...} ` +
      'or {"action":"init_task","name":...,"agent":...}. ' +
      'To OPEN an EXISTING project folder already on disk (so it shows in the left projects list), reply ONLY with ' +
      '{"action":"open_project","path":<absolute folder path>}. Use this when the user says "open <project>"; ' +
      `project folders live under ${settings.project_root || '<project_root>'}/<category folder>/<name>. ` +
      'Ask one short question at a time for missing fields. Otherwise answer normally.'
    );
  }

  // -------------------------------------------------------------- messages

  function scrollToBottom(entry) {
    entry.listEl.scrollTop = entry.listEl.scrollHeight;
  }

  const ICON_COPY = '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4"><rect x="5.5" y="5.5" width="8" height="8" rx="1.5"/><path d="M3.5 10.5h-1V2.5h8v1"/></svg>';
  const ICON_CHECK = '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6"><path d="M3.5 8.5l3 3 6-6.5"/></svg>';

  function copyText(s) {
    try {
      if (navigator.clipboard && navigator.clipboard.writeText) return navigator.clipboard.writeText(s);
    } catch (_) { /* fall through */ }
    const ta = document.createElement('textarea');
    ta.value = s; ta.style.position = 'fixed'; ta.style.opacity = '0';
    document.body.appendChild(ta); ta.select();
    try { document.execCommand('copy'); } catch (_) { /* noop */ }
    document.body.removeChild(ta);
    return Promise.resolve();
  }

  function appendMessage(entry, role, text) {
    const row = document.createElement('div');
    row.className = `chatp-msg chatp-msg-${role}`;
    const body = document.createElement('div');
    body.className = 'chatp-body';
    if (text) {
      if (role === 'assistant') renderAssistantMarkdown(body, text);
      else body.innerHTML = escapeText(text).replace(/\n/g, '<br>');
    }
    row.appendChild(body);
    // Copy button on user/assistant bubbles — copies the message text.
    if (role !== 'system') {
      const copyBtn = document.createElement('button');
      copyBtn.className = 'chatp-copy';
      copyBtn.title = 'Copy message';
      copyBtn.innerHTML = ICON_COPY;
      copyBtn.onclick = (e) => {
        e.stopPropagation();
        Promise.resolve(copyText(body.innerText || body.textContent || '')).then(() => {
          copyBtn.innerHTML = ICON_CHECK;
          setTimeout(() => { copyBtn.innerHTML = ICON_COPY; }, 1200);
        });
      };
      row.appendChild(copyBtn);
    }
    entry.listEl.appendChild(row);
    scrollToBottom(entry);
    return row;
  }

  function appendLearningAction(entry, row, analysis) {
    if (!entry.learningContext || !row || !String(analysis || '').trim()) return;
    const wrap = document.createElement('div');
    wrap.className = 'chatp-learning';
    const button = document.createElement('button');
    button.className = 'chatp-learning-btn';
    button.textContent = 'Add verified learning to Engram';
    const status = document.createElement('span');
    let armed = false;
    let armTimer = null;
    button.onclick = async () => {
      if (!armed) {
        armed = true;
        button.dataset.armed = '1';
        button.textContent = 'Confirm findings are verified';
        status.textContent = 'Stores this review as durable ticket learning.';
        clearTimeout(armTimer);
        armTimer = setTimeout(() => {
          armed = false;
          button.dataset.armed = '0';
          button.textContent = 'Add verified learning to Engram';
          status.textContent = '';
        }, 8000);
        return;
      }
      clearTimeout(armTimer);
      button.disabled = true;
      button.textContent = 'Saving...';
      status.textContent = '';
      try {
        await invoke('engram_store_learning', {
          learning: Object.assign({}, entry.learningContext, { analysis: String(analysis) }),
        });
        button.dataset.armed = '0';
        button.textContent = 'Saved to Engram';
        status.textContent = 'Included in the daily learning loop.';
      } catch (e) {
        armed = false;
        button.disabled = false;
        button.dataset.armed = '0';
        button.textContent = 'Retry Engram save';
        status.textContent = String(e);
      }
    };
    wrap.append(button, status);
    row.appendChild(wrap);
  }

  // ------------------------------------------------------------ action card

  // Open an existing project folder: verify it exists, register it, switch to it.
  async function handleOpenProject(entry, row, j) {
    const body = row.querySelector('.chatp-body');
    const path = (j.path || '').trim();
    if (!path) { body.innerHTML = '<span class="chatp-error">No folder path given.</span>'; return; }
    try {
      await invoke('list_directory', { path }); // throws if it doesn't exist
    } catch (_) {
      body.innerHTML = `<span class="chatp-error">That folder doesn't exist: ${escapeText(path)}</span>`;
      return;
    }
    try {
      const name = path.replace(/\/+$/, '').split('/').pop() || path;
      const proj = await invoke('tasks_create_project', { name, path });
      if (window.xnautSidebarRefresh) window.xnautSidebarRefresh();
      if (window.xnautSetActiveProject) await window.xnautSetActiveProject(proj.id, proj);
      body.innerHTML = `📂 Opened project <strong>${escapeText(proj.name)}</strong> — it's now in your projects list.`;
    } catch (e) {
      body.innerHTML = `<span class="chatp-error">${escapeText(String(e))}</span>`;
    }
  }

  function renderActionCard(entry, row, j, rawText) {
    const body = row.querySelector('.chatp-body');
    const isProject = j.action === 'init_project';
    const name = j.name || '(unnamed)';

    const rows = [];
    const category = j.type || j.category;
    if (category) rows.push(['Category', category]);
    if (isProject) {
      const idx = (j.forge_index !== undefined && j.forge_index !== null) ? j.forge_index : 0;
      const forge = (entry.settings.forges || [])[idx];
      rows.push(['Host', forge ? forge.base_url : `forge #${idx}`]);
    }
    if (j.agent) rows.push(['Agent', j.agent]);
    const baseline = j.baseline_prompt || j.prompt;
    if (isProject && baseline) rows.push(['Prompt', baseline]);

    body.innerHTML = '';
    const card = document.createElement('div');
    card.className = 'chatp-card';
    card.innerHTML = `
      <div class="chatp-card-title">${isProject ? 'Create project' : 'Create task'} ${escapeText(name)}</div>
      ${rows.map(([k, v]) => `
        <div class="chatp-card-row">
          <span class="chatp-card-key">${escapeText(k)}</span>
          <span class="chatp-card-val">${escapeText(String(v))}</span>
        </div>`).join('')}
      <div class="chatp-card-error" hidden></div>
      <div class="chatp-card-actions">
        <button class="chatp-btn chatp-btn-create">Create</button>
        <button class="chatp-btn chatp-btn-dismiss">Dismiss</button>
      </div>
    `;
    body.appendChild(card);

    const errEl = card.querySelector('.chatp-card-error');
    const createBtn = card.querySelector('.chatp-btn-create');
    const dismissBtn = card.querySelector('.chatp-btn-dismiss');

    dismissBtn.onclick = () => {
      body.innerHTML = renderMarkdownLite(rawText);
    };

    createBtn.onclick = async () => {
      createBtn.disabled = true;
      dismissBtn.disabled = true;
      errEl.hidden = true;
      try {
        let spec;
        if (isProject) {
          spec = await invoke('scaffold_init_project', {
            name: j.name,
            categoryLabel: j.type || j.category,
            forgeIndex: (j.forge_index !== undefined && j.forge_index !== null) ? j.forge_index : 0,
            agentId: j.agent || 'claude',
            baselinePrompt: j.baseline_prompt || j.prompt || '',
            private: j.private !== false,
          });
        } else {
          spec = await invoke('scaffold_init_task', {
            name: j.name,
            agentId: j.agent || null,
          });
        }
        createBtn.textContent = 'Created';
        dismissBtn.hidden = true;
        const path = spec && spec.task && spec.task.path;
        appendMessage(entry, 'system',
          `${isProject ? 'Project' : 'Task'} "${j.name}" created${path ? ` at ${path}` : ''}`);
        if (typeof window.xnautOpenLaunchSpec === 'function') {
          window.xnautOpenLaunchSpec(spec);
        }
      } catch (e) {
        errEl.textContent = String(e);
        errEl.hidden = false;
        createBtn.disabled = false;
        dismissBtn.disabled = false;
      }
    };
  }

  // ------------------------------------------------------- attachments

  // Resolve obsidian://open?vault=X&file=Y links and absolute *.md paths in
  // a message to file contents. Local models can't fetch files themselves —
  // pasted document references are inlined before the message is sent.
  const ATTACH_CAP = 12000;
  let vaultRootsPromise = null;

  // Obsidian vault name -> root path. Vault folders live up to 3 levels under
  // ~/Knowledge/Obsidian (e.g. Business/xNauts/xNauts). Cached per app run.
  function findVaultRoots() {
    if (vaultRootsPromise) return vaultRootsPromise;
    vaultRootsPromise = (async () => {
      const roots = {};
      const home = await invoke('get_home_directory').catch(() => null);
      if (!home) return roots;
      const base = `${home}/Knowledge/Obsidian`;
      const walk = async (dir, depth) => {
        if (depth > 3) return;
        let entries = [];
        try {
          const listing = await invoke('list_directory', { path: dir });
          entries = (listing && listing.entries) || (Array.isArray(listing) ? listing : []);
        } catch (_) { return; }
        for (const e of entries) {
          if (!e.is_directory || e.name.startsWith('.')) continue;
          roots[e.name] = roots[e.name] || e.path;
          await walk(e.path, depth + 1);
        }
      };
      await walk(base, 1);
      return roots;
    })();
    return vaultRootsPromise;
  }

  async function expandAttachments(text) {
    const attachments = [];
    const readAttachment = async (path) => {
      const body = await invoke('read_file', { path });
      const t = typeof body === 'string' ? body : (body && body.content) || '';
      return {
        content: t.length > ATTACH_CAP ? t.slice(0, ATTACH_CAP) + '\n…[truncated]' : t,
        fullContent: t,
      };
    };

    // obsidian://open?vault=<vault>&file=<url-encoded relative path, no .md>
    const obsRe = /obsidian:\/\/open\?[^\s)>"]+/g;
    for (const link of text.match(obsRe) || []) {
      try {
        const params = new URLSearchParams(link.slice(link.indexOf('?') + 1));
        const vault = params.get('vault');
        const file = params.get('file');
        if (!vault || !file) continue;
        const roots = await findVaultRoots();
        const root = roots[decodeURIComponent(vault)];
        if (!root) { attachments.push({ name: link, error: `vault "${vault}" not found` }); continue; }
        const rel = decodeURIComponent(file);
        const path = `${root}/${rel}${/\.md$/i.test(rel) ? '' : '.md'}`;
        attachments.push({ name: rel, path, ...await readAttachment(path) });
      } catch (e) {
        attachments.push({ name: link, error: String(e) });
      }
    }

    // Plain absolute markdown paths pasted into the message.
    const pathRe = /(?:^|[\s"'(<])(\/(?:[^\s"'()<>]+\/)*[^\s"'()<>]+\.md)\b/g;
    let m;
    while ((m = pathRe.exec(text)) !== null) {
      try {
        attachments.push({ name: m[1].split('/').pop(), path: m[1], ...await readAttachment(m[1]) });
      } catch (_) { /* not readable — leave the path as plain text */ }
    }
    return attachments;
  }

  function shouldImportAttachmentsToVault(entry, text, attachments) {
    if (!entry.vaultTools || !attachments || !attachments.some((a) => a.fullContent)) return false;
    return latestUserWantsVaultMutation([{ role: 'user', content: text, display: text }])
      && /\b(add|save|import|copy|create|write)\b/i.test(String(text || ''))
      && /\b(vault|note|document|markdown|\.md|obsidian)\b/i.test(String(text || ''));
  }

  function importedAttachmentRel(att) {
    const raw = String(att.name || att.path || 'imported.md').split('/').filter(Boolean).pop() || 'imported.md';
    const cleaned = raw
      .replace(/\.md$/i, '')
      .replace(/[\\/:*?"<>|#\[\]]+/g, '-')
      .replace(/\s+/g, ' ')
      .trim() || 'imported';
    return `_inbox/${cleaned}.md`;
  }

  async function importAttachmentsToVault(entry, row, attachments) {
    const vault = entry.vaultTools.vault();
    const body = row.querySelector('.chatp-body');
    const imported = [];
    for (const att of attachments.filter((a) => a.fullContent)) {
      const rel = importedAttachmentRel(att);
      let mode = 'created';
      try {
        await invoke('vault_note_create', { vault, rel, content: att.fullContent });
      } catch (e) {
        if (!/note exists/i.test(String(e))) throw e;
        await invoke('vault_note_write', { vault, rel, content: att.fullContent });
        mode = 'updated';
      }
      imported.push({ rel, mode });
    }
    if (entry.vaultTools.entry && typeof entry.vaultTools.entry.refresh === 'function') {
      await entry.vaultTools.entry.refresh();
    }
    const results = imported.map((i) => `${i.mode.toUpperCase()} ${i.rel}.`).join('\n');
    entry.history.push({ role: 'system', content: 'VAULT TOOL RESULTS:\n' + results });
    const finalText = imported.map((i) => `${i.mode} ${i.rel}`).join('\n');
    entry.history.push({ role: 'assistant', content: finalText });
    saveChatHistory(entry);
    if (imported[0] && entry.vaultTools.entry && entry.vaultTools.entry.openNote) {
      await entry.vaultTools.entry.openNote(imported[0].rel);
    }
    if (body) {
      body.innerHTML = '<div class="chatp-tool-status">'
        + '<div class="chatp-tool-summary">Imported Markdown into Vault</div>'
        + '<div class="chatp-tool-results">'
        + imported.map((i) => `<div class="chatp-tool-status-line">${escapeText(i.mode)} ${escapeText(i.rel)}</div>`).join('')
        + '</div></div>';
    }
  }

  const agentFatherDocumentIntent = /\b(?:create|make|setup|set\s+up|new|write|draft|save|import|copy|add)\b[\s\S]{0,80}\b(?:note|file|document|doc|template)\b|\b(?:note|file|document|doc|template)\b[\s\S]{0,80}\b(?:called|named|about|for)\b|\b(?:create|make|setup|set\s+up|new|save|import|copy|add|write)\b[\s\S]{0,80}\bvault\b|\bvault\b[\s\S]{0,80}\b(?:called|named|about|for|note|file|document|doc|template)\b/i;

  function wantsAgentFather(text) {
    const s = String(text || '').toLowerCase();
    if (agentFatherDocumentIntent.test(s)) return false;
    if (/\bagent[-\s]+loops?\b|\bloopbuilder\b/.test(s)) return false;
    const explicitProfileIntent = /\bagentfather\b/.test(s)
      || /\bagent[-\s]+profile\b/.test(s)
      || /\bagent\s+setup\b/.test(s);
    const directAgentCreation = /\b(?:create|make|setup)\s+(?:(?:a|an|the)\s+)?(?:new\s+)?(?:custom\s+)?agents?\b/.test(s)
      || /\bset\s+up\s+(?:(?:a|an|the)\s+)?(?:new\s+)?(?:custom\s+)?agents?\b/.test(s)
      || /\bnew\s+(?:custom\s+)?agents?\b/.test(s);
    return explicitProfileIntent || directAgentCreation;
  }

  function agentFatherSeed(entry, text) {
    const seed = {
      source: entry && entry.vaultTools ? 'librarian' : 'chat',
      responsibility: String(text || '').trim(),
    };
    if (entry && entry.vaultTools) {
      try {
        seed.vault = typeof entry.vaultTools.vault === 'function' ? entry.vaultTools.vault() : undefined;
      } catch (_) { /* best effort */ }
      const vaultEntry = entry.vaultTools.entry;
      if (vaultEntry && typeof vaultEntry.currentRel === 'function') {
        seed.rel = vaultEntry.currentRel();
      }
    }
    return seed;
  }

  function maybeOpenAgentFather(entry, text) {
    if (!wantsAgentFather(text) || typeof window.xnautOpenAgentFather !== 'function') return false;
    const seed = agentFatherSeed(entry, text);
    window.xnautOpenAgentFather(seed);
    const msg = 'Opened AgentFather for that agent setup.';
    entry.history.push({ role: 'assistant', content: msg });
    saveChatHistory(entry);
    appendMessage(entry, 'assistant', msg);
    return true;
  }

  // ----------------------------------------------------------------- send

  async function complete(entry, row) {
    const requestId = entry.activeRequestId;
    const messages = [{ role: 'system', content: entry.systemPrompt }];
    const workspaceContext = window.xnautGetAgentWorkspaceContext?.();
    if (workspaceContext) {
      const content = String(workspaceContext.content || '');
      const capped = content.length > 48000 ? `${content.slice(0, 48000)}\n\n[document truncated at 48,000 characters]` : content;
      messages.push({
        role: 'system',
        content: [
          'ACTIVE XNAUT WORKSPACE DOCUMENT',
          `Project: ${workspaceContext.project || 'Unknown'}`,
          `Stage: ${workspaceContext.stage || 'Unknown'}`,
          `Path: ${workspaceContext.vault || 'work'}:${workspaceContext.rel || ''}`,
          'The user may refer to this as "the open document", "this document", or "the one I just opened". Use this exact live snapshot when answering.',
          '',
          capped || '(empty document)',
        ].join('\n'),
      });
    }
    // Plan Mode: give the agent the live side document each turn so it extends
    // the real current content (including the user's manual edits), not a stale copy.
    if (entry.planMode && entry.planMode.getDoc) {
      const cur = (entry.planMode.getDoc() || '').trim();
      messages.push({
        role: 'system',
        content: 'CURRENT PLAN DOCUMENT (the live side document you maintain — extend or revise THIS exact content; do not start over):\n\n'
          + (cur || '(empty — draft the first version)'),
      });
    }
    const hasVaultToolResults = entry.vaultTools && lastMessageIsVaultToolResult(entry.history);
    const needsVaultAction = entry.vaultTools && !hasVaultToolResults
      && (latestUserNeedsVaultAction(entry.history) || latestUserRequestsVaultRead(entry.history));
    const pendingVaultMutation = entry.vaultTools
      && hasVaultToolResults
      && latestUserWantsVaultMutation(entry.history)
      && !lastVaultToolResultsHaveMutation(entry.history)
      && !lastVaultToolResultsBlockedAction(entry.history);
    if (needsVaultAction) {
      messages.push({
        role: 'system',
        content: 'The latest user message asks you to act on vault notes. Reply with ONLY the required vault tool JSON, such as {"action":"vault_read","rel":"relative/path.md"}. Do not explain, do not ask for confirmation, and do not write prose unless a tool result has already arrived.',
      });
    }
    messages.push(...entry.history.slice(-30).map((m) => ({ role: m.role, content: m.content })));
    if (hasVaultToolResults) {
      messages.push({
        role: 'user',
        content: pendingVaultMutation
          ? 'Continue from the vault tool results above. The original user request still requires creating, writing, moving, tagging, or saving a vault note. Reply with ONLY the next required vault tool JSON such as {"action":"vault_create","rel":"folder/name.md","content":"..."} or {"action":"vault_write","rel":"folder/name.md","content":"..."}. Do not write prose.'
          : 'Continue from the vault tool results above. Reply briefly with what changed or what needs attention.',
      });
    }

    const chatCommand = entry.providerOverride ? 'chat_send_provider' : (entry.modelOverride ? 'chat_send_model' : 'chat_send');
    const chatPayload = { requestId, messages };
    if (entry.modelOverride) chatPayload.model = entry.modelOverride;
    if (entry.providerOverride) chatPayload.provider = entry.providerOverride;
    let reply = await invoke(chatCommand, chatPayload);
    const actions = detectScaffoldActions(reply);
    const vaultActions = actions.filter((a) => a.action && a.action.startsWith('vault_'));
    const mcpActions = actions.filter((a) => a.action === 'mcp_call');
    const loopActions = actions.filter((a) => a.action === 'loop_create');
    const action = actions[0] || null;
    if (!vaultActions.length && (needsVaultAction || pendingVaultMutation)) {
      const repairPayload = {
        requestId,
        messages: messages.concat([
          { role: 'assistant', content: reply },
          { role: 'user', content: 'Your previous response was prose and did not perform the requested vault action. Convert it now into ONLY one or more JSON objects using an "action" key, for example {"action":"vault_create","rel":"Templates/example.md","content":"..."}. Use vault_create for creating notes/templates/files, vault_write for writing a full note body, vault_read for reading notes, vault_search for finding notes, vault_move for moving notes, and vault_tag for tags.' },
        ]),
      };
      if (entry.modelOverride) repairPayload.model = entry.modelOverride;
      if (entry.providerOverride) repairPayload.provider = entry.providerOverride;
      const repaired = await invoke(chatCommand, repairPayload);
      reply = repaired;
      const repairedActions = detectScaffoldActions(reply).filter((a) => a.action && a.action.startsWith('vault_'));
      if (repairedActions.length) {
        reply = repaired;
        await runVaultTools(entry, row, repairedActions);
        return;
      }
    }
    if (vaultActions.length && entry.vaultTools) {
      // Tool JSON is implementation detail, not user-facing conversation.
      // Persist only tool results and the final natural-language answer.
      await runVaultTools(entry, row, vaultActions);
      return;
    }
    if (mcpActions.length && entry.mcpTools) {
      await runMcpTools(entry, row, mcpActions);
      return;
    }
    if (loopActions.length && entry.loopTools) {
      await runLoopTool(entry, row, loopActions[0]);
      return;
    }
    if (needsVaultAction || pendingVaultMutation) {
      const attemptedVaultAction = /["']?action["']?\s*:\s*["']vault_/i.test(String(reply || ''));
      const msg = attemptedVaultAction
        ? 'The model returned an incomplete or malformed Vault command, so the document was not changed. Try again or select a model with reliable structured-output support.'
        : 'No note action was produced. The Agent response did not contain a valid Vault tool command, so no note was changed.';
      entry.history.push({ role: 'assistant', content: msg });
      saveChatHistory(entry);
      entry.liveBody.innerHTML = `<span class="chatp-error">${escapeText(msg)}</span>`;
      return;
    }
    if (action) {
      entry.history.push({ role: 'assistant', content: reply });
      saveChatHistory(entry);
      if (action.action === 'open_project') handleOpenProject(entry, row, action);
      else renderActionCard(entry, row, action, reply);
    } else if (entry.planMode) {
      // Route the plan to the side pane; show ONLY the conversation in chat.
      const doc = extractPlanDoc(reply);
      if (doc && entry.planMode.onPlanDoc) {
        Promise.resolve(entry.planMode.onPlanDoc(doc)).catch((e) => console.error('[chat-panel] plan doc apply failed', e));
      }
      let chatText = stripPlanDoc(reply);
      // If a plan was produced and the remaining prose is long, the model is
      // restating the plan in chat — drop it so only the doc carries it.
      if (doc && chatText.length > 280) chatText = '';
      const shown = chatText || (doc ? '📝 Updated the plan in the document pane →' : reply);
      renderAssistantMarkdown(entry.liveBody, shown);
      // Store only the conversation (the doc lives in the pane/file, fed back each turn).
      entry.history.push({ role: 'assistant', content: chatText || '(updated the plan document)' });
      saveChatHistory(entry);
    } else {
      entry.history.push({ role: 'assistant', content: reply });
      saveChatHistory(entry);
      renderAssistantMarkdown(entry.liveBody, reply);
      appendLearningAction(entry, row, reply);
    }
  }

  async function sendMessage(entry) {
    const text = entry.inputEl.value.trim();
    if (!text || entry.busy) return;
    entry.inputEl.value = '';
    autoGrow(entry.inputEl);

    // Inline any referenced documents (Obsidian links, absolute .md paths)
    // into the model-facing message; the UI shows the text as typed + chips.
    let modelText = text;
    let attachNote = '';
    let attachments = [];
    try {
      attachments = await expandAttachments(text);
      const ok = attachments.filter((a) => a.content);
      const failed = attachments.filter((a) => a.error);
      if (ok.length) {
        modelText += '\n\n--- Attached documents (resolved from links in this message) ---\n'
          + ok.map((a) => `### ${a.name}\n${a.content}`).join('\n\n');
        attachNote = `📎 ${ok.map((a) => a.name).join(', ')}`;
      }
      if (failed.length) {
        attachNote += `${attachNote ? ' · ' : ''}⚠ could not read: ${failed.map((a) => a.name).join(', ')}`;
      }
    } catch (e) {
      console.error('[chat-panel] attachment expansion failed', e);
    }

    entry.history.push({ role: 'user', content: modelText, display: text });
    saveChatHistory(entry);
    const userRow = appendMessage(entry, 'user', text);
    if (attachNote) {
      const note = document.createElement('div');
      note.className = 'chatp-attach-note';
      note.style.cssText = 'font-size:11px; opacity:.7; margin-top:2px;';
      note.textContent = attachNote;
      userRow.appendChild(note);
    }

    const userVaultActions = entry.vaultTools
      ? detectScaffoldActions(text).filter((a) => a.action && a.action.startsWith('vault_'))
      : [];
    const shouldImportToVault = shouldImportAttachmentsToVault(entry, text, attachments);

    if (!entry.loopTools && !userVaultActions.length && !shouldImportToVault && maybeOpenAgentFather(entry, text)) {
      entry.sendBtn.disabled = false;
      entry.inputEl.focus();
      return;
    }

    const requestId = newRequestId();
    entry.activeRequestId = requestId;
    entry.busy = true;
    entry.toolRounds = 0;
    entry.expectingVaultAction = entry.vaultTools && latestUserNeedsVaultAction(entry.history);
    entry.sendBtn.disabled = true;

    const row = appendMessage(entry, 'assistant', '');
    entry.liveRow = row;
    entry.liveBody = row.querySelector('.chatp-body');
    entry.liveText = '';
    if (userVaultActions.length) {
      entry.liveBody.innerHTML = '<span class="chatp-tool-summary">Running note action...</span>';
      try {
        await runVaultTools(entry, row, userVaultActions, { summarize: false });
      } catch (e) {
        entry.liveBody.innerHTML = `<span class="chatp-error">${escapeText(String(e))}</span>`;
        console.error('[chat-panel] user vault action failed', e);
      } finally {
        entry.busy = false;
        entry.activeRequestId = null;
        entry.liveRow = null;
        entry.liveBody = null;
        entry.expectingVaultAction = false;
        entry.sendBtn.disabled = false;
        scrollToBottom(entry);
        entry.inputEl.focus();
      }
      return;
    }
    if (shouldImportToVault) {
      entry.liveBody.innerHTML = '<span class="chatp-tool-summary">Importing Markdown into Vault...</span>';
      try {
        await importAttachmentsToVault(entry, row, attachments);
      } catch (e) {
        entry.liveBody.innerHTML = `<span class="chatp-error">${escapeText(String(e))}</span>`;
        console.error('[chat-panel] attachment import failed', e);
      } finally {
        entry.busy = false;
        entry.activeRequestId = null;
        entry.liveRow = null;
        entry.liveBody = null;
        entry.expectingVaultAction = false;
        entry.sendBtn.disabled = false;
        scrollToBottom(entry);
        entry.inputEl.focus();
      }
      return;
    }
    // Thinking indicator: the neon-mint pulsing X spinner until the reply lands.
    entry.liveBody.innerHTML = entry.expectingVaultAction
      ? '<span class="chatp-tool-summary">Preparing note action...</span>'
      : '<span class="agent-dot" data-state="working" data-size="md" aria-label="thinking"></span>';

    try {
      await complete(entry, row);
    } catch (e) {
      entry.liveBody.innerHTML = `<span class="chatp-error">${escapeText(String(e))}</span>`;
      console.error('[chat-panel] chat_send failed', e);
    } finally {
      entry.busy = false;
      entry.activeRequestId = null;
      entry.liveRow = null;
      entry.liveBody = null;
      entry.expectingVaultAction = false;
      entry.sendBtn.disabled = false;
      scrollToBottom(entry);
      entry.inputEl.focus();
    }
  }

  // --------------------------------------------------------------- textarea

  function autoGrow(ta) {
    ta.style.height = 'auto';
    // line-height 20px * 6 lines + vertical padding (6+6) = 132px (matches CSS max-height).
    ta.style.height = Math.min(ta.scrollHeight, 132) + 'px';
  }

  // ------------------------------------------------------------ pane create

  /**
   * Build the DOM for a chat pane. Returns the pane entry the tab system
   * stores in tab.terminals[]: { kind: 'chat', label, pane }.
   */
  async function createChatPane(tabId, parentContainer, opts) {
    opts = opts || {};
    injectStyles();
    const label = nextLabel();

    const pane = document.createElement('div');
    pane.className = 'chatp-pane';
    pane.dataset.chatLabel = label;
    pane.style.cssText = [
      'display:flex',
      'flex-direction:column',
      'flex:1 1 0%',
      'width:100%',
      'height:100%',
      'min-width:0',
      'min-height:0',
      'overflow:hidden',
      'background:var(--editor-surface)',
      'border-radius:var(--radius-md)',
    ].join('; ');

    const bar = document.createElement('div');
    bar.className = 'chatp-bar';
    bar.innerHTML = `
      <span class="chatp-title">Chat</span>
      <span class="chatp-model"></span>
      <span class="chatp-brain" hidden>\u{1F9E0}</span>
      <span class="chatp-spacer"></span>
      <button class="btn-icon chatp-close" data-variant="destructive" title="Close pane" aria-label="Close chat pane">
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor"><line x1="4" y1="4" x2="12" y2="12"/><line x1="12" y1="4" x2="4" y2="12"/></svg>
      </button>
    `;
    pane.appendChild(bar);

    const listEl = document.createElement('div');
    listEl.className = 'chatp-list';
    pane.appendChild(listEl);

    const inputArea = document.createElement('div');
    inputArea.className = 'chatp-input-area';
    inputArea.innerHTML = `
      <textarea class="chatp-input" rows="1" placeholder="Message… (Enter to send, Shift+Enter for newline)"></textarea>
      <button class="btn-icon chatp-dictate" title="Dictate message" aria-label="Dictate message">
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor"><rect x="5" y="2" width="6" height="8" rx="3"/><path d="M3.5 8a4.5 4.5 0 0 0 9 0M8 12.5V15M5.5 15h5"/></svg>
      </button>
      <button class="btn-icon chatp-send" title="Send" aria-label="Send message">
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor"><path d="M2 8l12-6-4 12-2.5-4.5L2 8z"/></svg>
      </button>
    `;
    pane.appendChild(inputArea);

    parentContainer.appendChild(pane);

    const entry = {
      kind: 'chat',
      label,
      pane,
      tabId,
      listEl,
      inputEl: inputArea.querySelector('.chatp-input'),
      dictateBtn: inputArea.querySelector('.chatp-dictate'),
      sendBtn: inputArea.querySelector('.chatp-send'),
      history: [],          // [{role, content}] — system prompt prepended at send time
      settings: { llm: {}, categories: [], forges: [] },
      systemPrompt: '',
      activeRequestId: null,
      liveRow: null,
      liveBody: null,
      liveText: '',
      busy: false,
      expectingVaultAction: false,
      modelOverride: String(opts.modelOverride || '').trim(),
      providerOverride: String(opts.providerOverride || '').trim(),
      learningContext: opts.learningContext || null,
      subs: [],             // promises resolving to unlisten fns
    };
    panes.set(label, entry);
    if (opts && opts.vaultTools) entry.vaultTools = opts.vaultTools;
    if (opts && opts.mcpTools) entry.mcpTools = opts.mcpTools;
    if (opts && opts.loopTools) entry.loopTools = true;

    // Stable key for persisted history: project path for project/plan chats,
    // an explicit opts.chatKey, else the shared 'default' chat.
    entry.chatKey = opts.chatKey || (opts.projectContext && opts.projectContext.path) || 'default';

    // --- header data: settings + agents + engram status (best effort) ---
    try {
      if (window.xnautSyncChatSettingsFromAiSettings) {
        await window.xnautSyncChatSettingsFromAiSettings().catch(() => false);
      }
      const [settings, agents] = await Promise.all([
        invoke('settings_get'),
        invoke('agent_list').catch(() => []),
      ]);
      if (settings) entry.settings = settings;
      entry.systemPrompt = buildSystemPrompt(entry.settings, agents);
      const modelEl = bar.querySelector('.chatp-model');
      if (modelEl && entry.settings.llm && entry.settings.llm.model) {
        modelEl.textContent = entry.modelOverride || entry.settings.llm.model;
      }
    } catch (e) {
      entry.systemPrompt = buildSystemPrompt(entry.settings, []);
      console.error('[chat-panel] settings_get/agent_list failed', e);
    }

    if (opts.systemPromptAppend) {
      entry.systemPrompt += `\n\n${String(opts.systemPromptAppend).trim()}`;
    }
    const titleEl = bar.querySelector('.chatp-title');
    if (titleEl && opts.title) titleEl.textContent = String(opts.title);
    if (opts.embedded) {
      const closeBtn = bar.querySelector('.chatp-close');
      if (closeBtn) closeBtn.hidden = true;
    }

    // PM Space v1.7: project-scoped chat — opts.projectContext grounds the
    // conversation in one client project's intake data AND its documents.
    // Documents are inlined into the system prompt (capped) because local
    // models without tool-calling can't fetch files on demand.
    if (opts && opts.projectContext) {
      const pc = opts.projectContext;
      const contacts = (pc.contacts || [])
        .map((c) => `${c.name}${c.role ? ` (${c.role}` + (c.email ? `, ${c.email})` : ')') : c.email ? ` (${c.email})` : ''}`)
        .join('; ');
      entry.systemPrompt += [
        '',
        '--- Active client project ---',
        `Client: ${pc.client_company || ''}`,
        contacts ? `Contacts: ${contacts}` : '',
        pc.scope ? `Scope: ${pc.scope}` : '',
        pc.path ? `Project folder: ${pc.path} (client documents in ${pc.path}/client/)` : '',
        'Ground answers about this project in the context above and the documents below.',
      ].filter(Boolean).join('\n');
      const titleEl = bar.querySelector('.chatp-title');
      if (titleEl && pc.client_company) titleEl.textContent = `Chat — ${pc.client_company}`;

      if (pc.path) {
        const PER_FILE_CAP = 8000;   // chars per document
        const TOTAL_CAP = 28000;     // chars across all documents
        const readDoc = async (path) => {
          try {
            const body = await invoke('read_file', { path });
            const text = typeof body === 'string' ? body : (body && body.content) || '';
            return text.length > PER_FILE_CAP
              ? text.slice(0, PER_FILE_CAP) + '\n…[truncated]'
              : text;
          } catch (_) { return null; }
        };
        try {
          const docs = [];
          const readme = await readDoc(pc.path + '/CLAUDE.md');
          if (readme) docs.push({ name: 'CLAUDE.md (project brief)', text: readme });
          let entries = [];
          try {
            const listing = await invoke('list_directory', { path: pc.path + '/client' });
            entries = (listing && listing.entries) || (Array.isArray(listing) ? listing : []);
          } catch (_) { /* no client/ dir yet */ }
          for (const e of entries) {
            if (e.is_directory || !/\.md$/i.test(e.name)) continue;
            const text = await readDoc(e.path);
            if (text) docs.push({ name: 'client/' + e.name, text });
          }
          let used = 0;
          const blocks = [];
          for (const d of docs) {
            if (used + d.text.length > TOTAL_CAP) {
              blocks.push(`### ${d.name}\n[omitted — context budget reached; ask the user to paste it if needed]`);
              continue;
            }
            used += d.text.length;
            blocks.push(`### ${d.name}\n${d.text}`);
          }
          if (blocks.length) {
            entry.systemPrompt += `\n\n--- Project documents (read-only snapshot at chat start) ---\n${blocks.join('\n\n')}`;
          }
        } catch (e) {
          console.error('[chat-panel] project docs load failed', e);
        }
      }
    }

    // Plan Mode: a solution-architect persona that maintains a living plan
    // document. The doc lives in the right-hand markdown pane; the agent emits
    // the full updated plan as one ```markdown block whenever it changes.
    if (opts && opts.planMode) {
      entry.planMode = opts.planMode;
      const defaultPersona = [
        '',
        '--- PLAN MODE ---',
        'You are a senior solution architect helping scope and plan this project.',
        'A live PLAN document is shown in a side pane and given to you each turn as "CURRENT PLAN DOCUMENT".',
        'Your job is to discuss in chat AND maintain that side document. Strict rules:',
        '1. NEVER paste the plan, document, or large content into your chat reply. The chat is for short discussion and questions ONLY.',
        '2. When you create or change the plan, output the COMPLETE updated document wrapped EXACTLY between a line "===PLAN DOCUMENT===" and a line "===END PLAN DOCUMENT===". Everything between those markers is rendered in the SIDE PANE (not shown in chat), so it must always be the full current document.',
        '3. The document is normal Markdown and MAY contain ```mermaid diagrams and ```code blocks — put those INSIDE the markers (do NOT wrap the whole document in a code fence).',
        '4. Build on the CURRENT PLAN DOCUMENT you are given — extend and refine it; never start over or drop existing content the user kept.',
        '5. Recommend a concrete tech stack (prefer what is in the Engram library / project context when relevant). Use a ```mermaid graph for the Architecture section when it helps.',
        '6. The document must keep these sections: Goal, Architecture, Tech stack, Tasks (checklist of work to do), Risks.',
        'In your chat text, write at most one or two sentences summarizing what you changed, plus any question.',
      ].join('\n');
      entry.systemPrompt += opts.planMode.persona || defaultPersona;
      const titleEl2 = bar.querySelector('.chatp-title');
      if (titleEl2) titleEl2.textContent = opts.planMode.title || ('Plan' + (opts.projectContext && opts.projectContext.client_company ? ` — ${opts.projectContext.client_company}` : ''));
    }

    // Restore prior conversation for this key.
    const saved = loadChatHistory(entry.chatKey).filter((m) => !isToolOnlyAssistantMessage(m));
    if (saved.length) {
      entry.history = saved;
      saveChatHistory(entry);
      saved.forEach((m) => {
        if (m.role === 'user') appendMessage(entry, 'user', m.display || m.content);
        else if (m.role === 'assistant') appendMessage(entry, 'assistant', m.content);
      });
    }

    invoke('engram_status').then((st) => {
      if (!st || !st.enabled) return;
      const brainEl = bar.querySelector('.chatp-brain');
      if (!brainEl) return;
      brainEl.hidden = false;
      brainEl.classList.add(st.reachable ? 'chatp-brain-ok' : 'chatp-brain-bad');
      brainEl.title = `Engram ${st.reachable ? 'reachable' : 'unreachable'} (${st.url || ''})`;
    }).catch(() => { /* engram optional */ });

    invoke('chat_check_endpoint').then((ok) => {
      if (!ok) {
        const ep = entry.settings.llm && entry.settings.llm.endpoint;
        appendMessage(entry, 'system', `LLM endpoint not reachable${ep ? ` (${ep})` : ''}`);
      }
    }).catch(() => { /* non-fatal */ });

    // --- streaming event subscriptions (once per pane, filter by requestId) ---
    function sub(name, handler) {
      try {
        entry.subs.push(listen(name, handler));
      } catch (e) {
        console.error(`[chat-panel] listen(${name}) failed`, e);
      }
    }

    sub('chat://chunk', (ev) => {
      const p = ev.payload || {};
      if (!entry.activeRequestId || p.requestId !== entry.activeRequestId || !entry.liveBody) return;
      entry.liveText += p.delta || '';
      if (entry.expectingVaultAction) return;
      entry.liveBody.innerHTML = renderMarkdownLite(entry.liveText);
      scrollToBottom(entry);
    });

    sub('chat://done', (ev) => {
      const p = ev.payload || {};
      if (p.requestId !== entry.activeRequestId) return;
      // Final render happens when the chat_send promise resolves with the
      // authoritative full reply; nothing further needed here.
    });

    sub('chat://brain', (ev) => {
      const p = ev.payload || {};
      if (!entry.activeRequestId || p.requestId !== entry.activeRequestId || !entry.liveRow) return;
      let chip = entry.liveRow.querySelector('.chatp-brain-chip');
      if (!chip) {
        chip = document.createElement('span');
        chip.className = 'chatp-brain-chip';
        entry.liveRow.appendChild(chip);
      }
      chip.textContent = `\u{1F9E0} ${p.count}`;
    });

    // --- input wiring ---
    entry.inputEl.addEventListener('input', () => autoGrow(entry.inputEl));
    entry.inputEl.addEventListener('keydown', (e) => {
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        sendMessage(entry).catch((err) => console.error('[chat-panel] send failed', err));
      }
    });
    entry.sendBtn.onclick = () => sendMessage(entry).catch((err) => console.error('[chat-panel] send failed', err));
    entry.dictateBtn.onclick = () => {
      const Recognition = window.SpeechRecognition || window.webkitSpeechRecognition;
      if (!Recognition) return;
      const recognition = new Recognition();
      recognition.lang = navigator.language || 'en-US';
      recognition.interimResults = false;
      recognition.onresult = (event) => {
        const text = event.results[0][0].transcript;
        entry.inputEl.value = `${entry.inputEl.value}${entry.inputEl.value ? ' ' : ''}${text}`;
        autoGrow(entry.inputEl);
        entry.inputEl.focus();
      };
      recognition.start();
    };
    const closeBtn = bar.querySelector('.chatp-close');
    if (closeBtn) closeBtn.onclick = () => destroyChatPane(label);

    const shouldStart = !!(opts.prefill && !entry.history.length);
    if (shouldStart) {
      entry.inputEl.value = String(opts.prefill);
      autoGrow(entry.inputEl);
    }

    entry.inputEl.focus();
    if (shouldStart && opts.autoSend) {
      Promise.resolve().then(() => sendMessage(entry)).catch((e) => console.error('[chat-panel] initial send failed', e));
    }
    return entry;
  }

  async function destroyChatPane(label) {
    const entry = panes.get(label);
    if (!entry) return;
    entry.subs.forEach((p) => {
      Promise.resolve(p).then((un) => { try { un(); } catch (_) { /* already gone */ } }).catch(() => {});
    });
    if (entry.pane && entry.pane.parentNode) entry.pane.parentNode.removeChild(entry.pane);
    panes.delete(label);
  }

  // Public API hooks (mirror the markdown-pane shape).
  window.xnautCreateChatPane = createChatPane;
  window.xnautDestroyChatPane = destroyChatPane;
  window.xnautGetChatHistory = getChatHistory;
  window.xnautSetChatHistory = setChatHistory;
  window.xnautClearChatHistory = clearChatHistory;
  window.xnautCompileAgentLoop = compileAgentLoop;
  window.xnautWantsAgentFather = wantsAgentFather;

  /**
   * Top-bar "New Chat" handler — delegates to app.js's xnautAttachChatTab so
   * the tab object goes through the same renderTabs/switchTab path as
   * terminal/browser/markdown tabs.
   */
  async function newChatTab(opts) {
    if (typeof window.xnautAttachChatTab !== 'function') {
      console.warn('xnautAttachChatTab not yet defined in app.js');
      return;
    }
    return window.xnautAttachChatTab(opts || {});
  }
  window.xnautNewChatTab = newChatTab;

  function wireButton() {
    const btn = $('btn-new-chat');
    if (btn) btn.onclick = () => newChatTab().catch((e) => console.error('new chat tab failed:', e));
  }
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', wireButton);
  } else {
    wireButton();
  }
})();
