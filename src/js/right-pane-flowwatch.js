// Right pane · Flow Watch (XNAUT-242): what the agents are doing, live.
//
// André, 2026-08-28: "If I execute a new flow with NautBot I want to see
// whats happening, lets use the right pane for that, we could add a
// collapsable part that can be clicked open the session output and shows
// what it is doing."
//
// One collapsible row per agent session from the status tracker. A collapsed
// row costs nothing; expanding fetches the PTY tail once
// (terminal_output_snapshot) and then streams terminal-output:{id} events
// into a read-only view. Collapse unsubscribes. This is a WINDOW, never a
// terminal: no input path exists here on purpose, so watching a flow can
// never type into it.
(function () {
  'use strict';

  const invoke = (...a) => window.__TAURI__.core.invoke(...a);
  const listen = (...a) => window.__TAURI__.event.listen(...a);

  const TAIL_CAP = 60000; // chars kept per expanded row
  const STATUS_COLOR = {
    working: 'var(--working, #eab308)',
    blocked: 'var(--alarm, #ff6568)',
    permission: 'var(--alarm, #ff6568)',
    waiting: 'var(--accent, #f5b840)',
    done: 'var(--clear, #10b981)',
    idle: 'var(--text-dim, #a1a1a1)',
    interrupted: 'var(--alarm, #ff6568)',
  };

  // Raw PTY bytes -> readable text: decode UTF-8, drop ANSI escapes and
  // control chars that are not newlines.
  const decoder = new TextDecoder('utf-8', { fatal: false });
  function cleanChunk(bytes) {
    return decoder
      .decode(bytes, { stream: true })
      .replace(/\x1b\[[0-9;?]*[ -\/]*[@-~]/g, '')
      .replace(/\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)/g, '')
      .replace(/\x1b[@-_]/g, '')
      .replace(/\r(?!\n)/g, '\n')
      .replace(/[\x00-\x08\x0b\x0c\x0e-\x1f]/g, '');
  }
  function b64Bytes(b64) {
    const bin = atob(b64 || '');
    const bytes = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
    return bytes;
  }

  const sessions = new Map(); // session_id -> meta
  const rows = new Map();     // session_id -> { el, out, open, unlisten, autoscroll }
  let host = null;
  let unsubscribers = [];

  function statusDot(meta) {
    const color = STATUS_COLOR[meta.status] || STATUS_COLOR.idle;
    return '<span class="fw-dot" style="background:' + color + '"></span>';
  }

  function escapeText(s) {
    return String(s).replace(/[&<>"']/g, (c) => ({ '&':'&amp;', '<':'&lt;', '>':'&gt;', '"':'&quot;', "'":'&#39;' }[c]));
  }

  function ensureStyles() {
    if (document.getElementById('fw-styles')) return;
    const style = document.createElement('style');
    style.id = 'fw-styles';
    style.textContent = [
      '.fw-wrap { display:flex; flex-direction:column; gap:6px; padding:10px; overflow-y:auto; height:100%; }',
      '.fw-empty { color: var(--text-dim, #a1a1a1); font-size:12.5px; padding:14px 6px; }',
      '.fw-row { border:1px solid var(--border, rgba(255,255,255,.07)); border-radius:8px; overflow:hidden; }',
      '.fw-head { display:flex; align-items:center; gap:8px; padding:8px 10px; cursor:pointer; user-select:none; background:transparent; border:none; width:100%; text-align:left; color:inherit; font:inherit; }',
      '.fw-head:hover { background: rgba(255,255,255,.04); }',
      '.fw-dot { width:8px; height:8px; border-radius:50%; flex:0 0 8px; }',
      '.fw-label { font-size:12.5px; font-weight:600; flex:1 1 auto; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }',
      '.fw-status { font-family: var(--mono, monospace); font-size:10.5px; color: var(--text-dim, #a1a1a1); flex:0 0 auto; }',
      '.fw-caret { flex:0 0 auto; transition: transform .12s; color: var(--text-dim, #a1a1a1); }',
      '.fw-row.open .fw-caret { transform: rotate(90deg); }',
      '.fw-out { display:none; margin:0; padding:8px 10px; max-height:320px; overflow-y:auto; overflow-x:hidden; font-family: var(--mono, monospace); font-size:11px; line-height:1.45; white-space:pre-wrap; word-break:break-word; background: var(--terminal-bg, #1e1e1e); border-top:1px solid var(--border, rgba(255,255,255,.07)); }',
      '.fw-row.open .fw-out { display:block; }',
    ].join('\n');
    document.head.appendChild(style);
  }

  function appendOut(row, text) {
    if (!text) return;
    row.buffer = (row.buffer + text).slice(-TAIL_CAP);
    row.out.textContent = row.buffer;
    if (row.autoscroll) row.out.scrollTop = row.out.scrollHeight;
  }

  async function expand(sid) {
    const row = rows.get(sid);
    if (!row || row.open) return;
    row.open = true;
    row.el.classList.add('open');
    row.buffer = '';
    try {
      const b64 = await invoke('terminal_output_snapshot', { sessionId: sid });
      appendOut(row, cleanChunk(b64Bytes(b64)));
    } catch (_) {
      appendOut(row, '(no output captured yet)\n');
    }
    // Live stream from here on. Payload matches the terminal listeners
    // elsewhere: base64 in event.payload (string) or payload.data.
    try {
      row.unlisten = await listen('terminal-output:' + sid, (event) => {
        const payload = event && event.payload;
        const b64 = typeof payload === 'string' ? payload : payload && payload.data;
        if (!b64) return;
        try { appendOut(row, cleanChunk(b64Bytes(b64))); } catch (_) {}
      });
    } catch (_) {}
    row.out.addEventListener('scroll', () => {
      row.autoscroll = row.out.scrollTop + row.out.clientHeight >= row.out.scrollHeight - 24;
    });
  }

  function collapse(sid) {
    const row = rows.get(sid);
    if (!row || !row.open) return;
    row.open = false;
    row.el.classList.remove('open');
    if (row.unlisten) { try { row.unlisten(); } catch (_) {} row.unlisten = null; }
  }

  function upsertRow(meta) {
    const sid = meta.session_id;
    let row = rows.get(sid);
    if (!row) {
      const el = document.createElement('div');
      el.className = 'fw-row';
      el.innerHTML =
        '<button class="fw-head" aria-expanded="false">' +
        statusDot(meta) +
        '<span class="fw-label">' + escapeText(meta.label || meta.agent_id || sid) + '</span>' +
        '<span class="fw-status">' + escapeText(meta.status || '') + '</span>' +
        '<span class="fw-caret">▸</span>' +
        '</button>' +
        '<pre class="fw-out"></pre>';
      const out = el.querySelector('.fw-out');
      row = { el, out, open: false, unlisten: null, autoscroll: true, buffer: '' };
      el.querySelector('.fw-head').addEventListener('click', () => {
        const willOpen = !row.open;
        el.querySelector('.fw-head').setAttribute('aria-expanded', String(willOpen));
        if (willOpen) expand(sid); else collapse(sid);
      });
      rows.set(sid, row);
    } else {
      row.el.querySelector('.fw-dot').style.background = STATUS_COLOR[meta.status] || STATUS_COLOR.idle;
      row.el.querySelector('.fw-status').textContent = meta.status || '';
      row.el.querySelector('.fw-label').textContent = meta.label || meta.agent_id || sid;
    }
    return row;
  }

  function dropRow(sid) {
    const row = rows.get(sid);
    if (!row) return;
    collapse(sid);
    row.el.remove();
    rows.delete(sid);
  }

  function render() {
    if (!host) return;
    const list = [...sessions.values()].sort((a, b) => (b.started_at_ms || 0) - (a.started_at_ms || 0));
    const empty = host.querySelector('.fw-empty');
    if (empty) empty.style.display = list.length ? 'none' : 'block';
    for (const meta of list) {
      const row = upsertRow(meta);
      if (row.el.parentNode !== host) host.appendChild(row.el);
    }
    for (const sid of [...rows.keys()]) {
      if (!sessions.has(sid)) dropRow(sid);
    }
  }

  async function seed() {
    try {
      const list = await invoke('agent_sessions_list');
      (list || []).forEach((meta) => { if (meta && meta.session_id) sessions.set(meta.session_id, meta); });
    } catch (_) {}
    render();
  }

  async function subscribe() {
    try {
      unsubscribers.push(await listen('agent-status-changed', (event) => {
        const meta = event && event.payload;
        if (!meta || !meta.session_id) return;
        sessions.set(meta.session_id, meta);
        render();
      }));
      unsubscribers.push(await listen('agent-status-dropped', (event) => {
        const sid = event && event.payload && event.payload.sessionId;
        if (!sid) return;
        sessions.delete(sid);
        render();
      }));
    } catch (_) {}
  }

  const view = {
    mount(container) {
      ensureStyles();
      host = document.createElement('div');
      host.className = 'fw-wrap';
      host.innerHTML = '<div class="fw-empty">No agent sessions running. Wake one and it appears here.</div>';
      container.appendChild(host);
      seed();
      subscribe();
    },
    setRoot() { /* project-independent: agents are machine-wide */ },
    destroy() {
      for (const sid of [...rows.keys()]) dropRow(sid);
      unsubscribers.forEach((u) => { try { u(); } catch (_) {} });
      unsubscribers = [];
      sessions.clear();
      if (host) { host.remove(); host = null; }
    },
  };

  if (window.xnautRightPaneRegisterView) window.xnautRightPaneRegisterView('flowwatch', view);
  else (window.__xnautRightPaneQueue = window.__xnautRightPaneQueue || []).push(['flowwatch', view]);
})();
