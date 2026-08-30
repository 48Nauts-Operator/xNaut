// Right pane · Verify history (XNAUT-250): every sandbox verify run, kept.
//
// André, 2026-08-30: "Make it history information on the right pane,
// persistent, so one can go back and see what happened." The records already
// live on disk (sandbox_verify_records), so this view is a reader: one
// collapsible row per run, newest first, updated live by the same
// sandbox-verify-changed event the corner pill uses. The pill says "now";
// this pane answers "what happened while I was elsewhere".
(function () {
  'use strict';

  const invoke = (...a) => window.__TAURI__.core.invoke(...a);
  const listen = (...a) => window.__TAURI__.event.listen(...a);

  const STATUS_COLOR = {
    running: 'var(--working, #eab308)',
    passed: 'var(--clear, #10b981)',
    failed: 'var(--alarm, #ff6568)',
    cancelled: 'var(--text-dim, #a1a1a1)',
  };

  const STYLES = `
.vh-wrap { display:flex; flex-direction:column; overflow-y:auto; height:100%; }
.vh-empty { padding:14px 12px; font-size:12px; color:var(--text-secondary, #888); }
.vh-row { border-bottom:1px solid var(--border, rgba(255,255,255,.06)); }
.vh-head { display:flex; align-items:center; gap:8px; padding:7px 10px; cursor:pointer; }
.vh-head:hover { background:var(--hover-bg, rgba(255,255,255,.05)); }
.vh-dot { flex:0 0 auto; width:8px; height:8px; border-radius:50%; }
.vh-ticket { flex:0 0 auto; font-size:12px; font-weight:600; color:var(--text-primary, #e8eaf0); }
.vh-verdict { flex:1 1 auto; min-width:0; font-size:11px; color:var(--text-secondary, #8a8f98); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
.vh-time { flex:0 0 auto; font-size:10px; color:var(--text-secondary, #8a8f98); }
.vh-body { padding:2px 10px 10px 26px; display:flex; flex-direction:column; gap:6px; }
.vh-meta { font-size:10px; color:var(--text-secondary, #8a8f98); font-family:var(--font-mono, monospace); word-break:break-all; }
.vh-step { display:flex; flex-direction:column; gap:2px; }
.vh-step-head { display:flex; gap:8px; align-items:baseline; font-size:11px; color:var(--text-primary, #ddd); }
.vh-exit { font-family:var(--font-mono, monospace); font-size:10px; }
.vh-log { margin:0; padding:6px 8px; max-height:180px; overflow:auto; font:10px/1.5 var(--font-mono, monospace); color:var(--text-secondary, #a8adb8); background:rgba(0,0,0,.25); border-radius:6px; white-space:pre-wrap; word-break:break-word; }
`;

  function ensureStyles() {
    if (document.getElementById('right-pane-verify-styles')) return;
    const el = document.createElement('style');
    el.id = 'right-pane-verify-styles';
    el.textContent = STYLES;
    document.head.appendChild(el);
  }

  function when(iso) {
    const t = new Date(iso);
    if (Number.isNaN(t.getTime())) return '';
    const sameDay = t.toDateString() === new Date().toDateString();
    return sameDay
      ? t.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
      : t.toLocaleDateString([], { month: 'short', day: 'numeric' });
  }

  function verdict(record) {
    const done = (record.steps || []).filter((s) => s.exit_code !== null && s.exit_code !== undefined);
    const red = done.find((s) => s.exit_code !== 0);
    if (record.status === 'passed') return `passed · ${record.sandbox_id || record.provider_kind}`;
    if (record.status === 'failed') return red ? `failed at ${red.name} (exit ${red.exit_code})` : 'failed';
    const next = (record.steps || [])[done.length];
    return next ? `running: ${next.name}…` : 'starting…';
  }

  let host = null;
  let listHost = null;
  let unlisten = null;
  const rows = new Map(); // record id -> { el, open }

  function renderBody(body, record) {
    body.textContent = '';
    const meta = document.createElement('div');
    meta.className = 'vh-meta';
    meta.textContent = `${record.provider_kind} · ${record.id}`;
    body.appendChild(meta);
    for (const step of record.steps || []) {
      const wrap = document.createElement('div');
      wrap.className = 'vh-step';
      const head = document.createElement('div');
      head.className = 'vh-step-head';
      const name = document.createElement('span');
      name.textContent = step.name;
      const exit = document.createElement('span');
      exit.className = 'vh-exit';
      const code = step.exit_code;
      exit.textContent = code === null || code === undefined ? '·' : `exit ${code}`;
      exit.style.color = code === 0 ? STATUS_COLOR.passed : code ? STATUS_COLOR.failed : 'inherit';
      head.append(name, exit);
      wrap.appendChild(head);
      if (step.log_tail) {
        const log = document.createElement('pre');
        log.className = 'vh-log';
        log.textContent = step.log_tail;
        wrap.appendChild(log);
      }
      body.appendChild(wrap);
    }
  }

  function upsert(record, toTop) {
    if (!listHost || !record || !record.id) return;
    let row = rows.get(record.id);
    if (!row) {
      const el = document.createElement('div');
      el.className = 'vh-row';
      const head = document.createElement('div');
      head.className = 'vh-head';
      const body = document.createElement('div');
      body.className = 'vh-body';
      body.style.display = 'none';
      head.onclick = () => {
        row.open = !row.open;
        body.style.display = row.open ? 'flex' : 'none';
        if (row.open) renderBody(body, row.record);
      };
      el.append(head, body);
      row = { el, head, body, open: false, record };
      rows.set(record.id, row);
      if (toTop && listHost.firstChild) listHost.insertBefore(el, listHost.firstChild);
      else listHost.appendChild(el);
    }
    row.record = record;
    row.head.textContent = '';
    const dot = document.createElement('span');
    dot.className = 'vh-dot';
    dot.style.background = STATUS_COLOR[record.status] || STATUS_COLOR.cancelled;
    const ticket = document.createElement('span');
    ticket.className = 'vh-ticket';
    ticket.textContent = record.ticket_id || record.project || '?';
    const line = document.createElement('span');
    line.className = 'vh-verdict';
    line.textContent = verdict(record);
    const time = document.createElement('span');
    time.className = 'vh-time';
    time.textContent = when(record.updated_at || record.created_at);
    row.head.append(dot, ticket, line, time);
    if (row.open) renderBody(row.body, record);
  }

  async function seed() {
    try {
      const records = (await invoke('sandbox_verify_records')) || [];
      if (!records.length) {
        const empty = document.createElement('div');
        empty.className = 'vh-empty';
        empty.textContent = 'No verify runs yet. Ask NautBot to verify a ticket and it lands here.';
        listHost.appendChild(empty);
        return;
      }
      for (const record of records) upsert(record, false); // already newest first
    } catch (error) {
      console.warn('[verify pane] records load failed:', error);
    }
  }

  const view = {
    mount(container) {
      ensureStyles();
      host = document.createElement('div');
      host.className = 'vh-wrap';
      listHost = host;
      container.appendChild(host);
      seed();
      listen('sandbox-verify-changed', (event) => {
        const empty = host && host.querySelector('.vh-empty');
        if (empty) empty.remove();
        upsert(event.payload, true);
      }).then((u) => { unlisten = u; }).catch(() => {});
    },
    setRoot() { /* records are machine-wide, keyed by ticket */ },
    destroy() {
      if (unlisten) { try { unlisten(); } catch (_) {} unlisten = null; }
      rows.clear();
      if (host) { host.remove(); host = null; listHost = null; }
    },
  };

  if (window.xnautRightPaneRegisterView) window.xnautRightPaneRegisterView('verify', view);
  else (window.__xnautRightPaneQueue = window.__xnautRightPaneQueue || []).push({ key: 'verify', view });
})();
