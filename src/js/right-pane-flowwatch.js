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

  const MAX_LINES = 400; // rendered lines kept per expanded row
  const STATUS_COLOR = {
    working: 'var(--working, #eab308)',
    blocked: 'var(--alarm, #ff6568)',
    permission: 'var(--alarm, #ff6568)',
    waiting: 'var(--accent, #f5b840)',
    done: 'var(--clear, #10b981)',
    idle: 'var(--text-dim, #a1a1a1)',
    interrupted: 'var(--alarm, #ff6568)',
  };

  // Raw PTY bytes to readable text.
  //
  // Stripping escapes is NOT enough: a TUI positions the cursor instead of
  // emitting spaces, so a naive strip glues words together ("##Yourskills")
  // and loses every column. This is the smallest model that renders one
  // honestly: a line buffer with a column, honouring the handful of
  // sequences that actually move the cursor or erase, and dropping the rest.
  // Not a terminal emulator, and it does not need to be; it needs to be
  // readable.
  const decoder = new TextDecoder('utf-8', { fatal: false });

  function makeScreen() {
    return { lines: [''], row: 0, col: 0 };
  }

  function put(screen, text) {
    let line = screen.lines[screen.row] || '';
    if (line.length < screen.col) line = line.padEnd(screen.col, ' ');
    screen.lines[screen.row] = line.slice(0, screen.col) + text + line.slice(screen.col + text.length);
    screen.col += text.length;
  }

  function newline(screen) {
    screen.row += 1;
    if (!screen.lines[screen.row]) screen.lines[screen.row] = '';
  }

  /// Feeds a chunk into the screen model. Returns nothing; read screen.lines.
  function feed(screen, bytes) {
    const text = decoder.decode(bytes, { stream: true });
    let i = 0;
    while (i < text.length) {
      const ch = text[i];
      if (ch === '\x1b') {
        // OSC: ESC ] ... BEL or ST. Window titles; never content.
        if (text[i + 1] === ']') {
          const end = text.indexOf('\x07', i);
          const st = text.indexOf('\x1b\\', i);
          const stop = end === -1 ? st : (st === -1 ? end : Math.min(end, st));
          i = stop === -1 ? text.length : stop + (text[stop] === '\x07' ? 1 : 2);
          continue;
        }
        // DCS / APC / PM: ESC P|_|^ ... ST. Terminal replies, not content.
        if ('P_^'.includes(text[i + 1])) {
          const st = text.indexOf('\x1b\\', i);
          i = st === -1 ? text.length : st + 2;
          continue;
        }
        // CSI: ESC [ params letter
        if (text[i + 1] === '[') {
          let j = i + 2;
          while (j < text.length && !/[@-~]/.test(text[j])) j++;
          const params = text.slice(i + 2, j).replace(/[?<>!]/g, '');
          const final = text[j];
          const n = parseInt(params.split(';')[0], 10);
          const count = Number.isFinite(n) ? n : 1;
          switch (final) {
            case 'C': screen.col += count; break;                    // cursor forward
            case 'D': screen.col = Math.max(0, screen.col - count); break;
            case 'G': screen.col = Math.max(0, count - 1); break;    // column
            case 'K':                                                 // erase in line
              if (params === '' || params === '0') {
                screen.lines[screen.row] = (screen.lines[screen.row] || '').slice(0, screen.col);
              }
              break;
            case 'J':                                                 // erase screen
              if (params === '2' || params === '3') { screen.lines = ['']; screen.row = 0; screen.col = 0; }
              break;
            case 'H': case 'f': {                                     // absolute position
              const parts = params.split(';');
              const r = parseInt(parts[0], 10);
              const c = parseInt(parts[1], 10);
              screen.row = Math.max(0, (Number.isFinite(r) ? r : 1) - 1);
              screen.col = Math.max(0, (Number.isFinite(c) ? c : 1) - 1);
              while (screen.lines.length <= screen.row) screen.lines.push('');
              break;
            }
            default: break;                                           // colours, modes: ignore
          }
          i = j === text.length ? j : j + 1;
          continue;
        }
        i += 2; // ESC + one byte
        continue;
      }
      if (ch === '\n') { newline(screen); screen.col = 0; i++; continue; }
      if (ch === '\r') { screen.col = 0; i++; continue; }
      if (ch === '\b') { screen.col = Math.max(0, screen.col - 1); i++; continue; }
      if (ch === '\t') { screen.col += 8 - (screen.col % 8); i++; continue; }
      if (ch < ' ' && ch !== ' ') { i++; continue; }                  // other control bytes
      // Run of printable text up to the next control character.
      let j = i;
      while (j < text.length && text[j] >= ' ' && text[j] !== '\x1b') j++;
      put(screen, text.slice(i, j));
      i = j;
    }
  }

  /// The screen as text: trailing blank lines dropped, capped for the DOM.
  function screenText(screen, maxLines) {
    let lines = screen.lines;
    let end = lines.length;
    while (end > 0 && !lines[end - 1].trim()) end--;
    lines = lines.slice(Math.max(0, end - maxLines), end);
    return lines.map((l) => l.replace(/\s+$/, '')).join('\n');
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
  let asksHost = null;
  let flowHost = null;
  let flowTimer = null;
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
      '.fw-out { display:none; margin:0; padding:8px 10px; max-height:420px; overflow-y:auto; overflow-x:hidden; font-family: var(--mono, monospace); font-size:11.5px; line-height:1.5; white-space:pre-wrap; word-break:break-word; background: var(--terminal-bg, #1e1e1e); border-top:1px solid var(--border, rgba(255,255,255,.07)); }',
      '.fw-row.open .fw-out { display:block; }',
      '.fw-asks { display:flex; flex-direction:column; gap:6px; margin-bottom:4px; }',
      '.fw-ask { border:1px solid var(--amber, #f5b840); border-radius:8px; padding:10px 12px; background: rgba(245,184,64,.06); }',
      '.fw-ask-top { display:flex; align-items:baseline; gap:8px; margin-bottom:4px; }',
      '.fw-ask-from { font-family: var(--mono, monospace); font-size:10.5px; color: var(--amber, #f5b840); flex:0 0 auto; }',
      '.fw-ask-title { font-size:12.5px; font-weight:600; flex:1 1 auto; }',
      '.fw-ask-body { font-size:11.5px; color: var(--text-dim, #a1a1a1); white-space:pre-wrap; margin-bottom:8px; max-height:120px; overflow-y:auto; }',
      '.fw-ask-acts { display:flex; gap:6px; flex-wrap:wrap; }',
      '.fw-ask-acts button { font-size:11.5px; padding:4px 10px; border-radius:6px; border:1px solid var(--border, rgba(255,255,255,.12)); background:transparent; color:inherit; cursor:pointer; }',
      '.fw-ask-acts button:hover { border-color: var(--amber, #f5b840); }',
      '.fw-ask-acts button:focus-visible { outline:2px solid var(--amber, #f5b840); outline-offset:2px; }',
      '.fw-ask-acts button.approve { border-color: var(--clear, #10b981); color: var(--clear, #10b981); }',
      '.fw-ask-acts button.deny { border-color: var(--alarm, #ff6568); color: var(--alarm, #ff6568); }',
      '.fw-ask-reply { display:flex; gap:6px; margin-top:6px; }',
      '.fw-flow { margin-top:10px; border-top:1px solid var(--border, rgba(255,255,255,.07)); padding-top:8px; }',
      '.fw-flow-title { font-family: var(--mono, monospace); font-size:10px; letter-spacing:.12em; text-transform:uppercase; color: var(--text-dim, #a1a1a1); margin-bottom:6px; }',
      '.fw-flow-row { display:grid; grid-template-columns:auto 1fr; gap:8px; padding:4px 0; border-bottom:1px solid rgba(255,255,255,.04); }',
      '.fw-flow-row:last-child { border-bottom:0; }',
      '.fw-flow-when { font-family: var(--mono, monospace); font-size:10px; color: var(--text-dim, #a1a1a1); white-space:nowrap; }',
      '.fw-flow-what { font-size:11.5px; line-height:1.4; }',
      '.fw-flow-id { font-family: var(--mono, monospace); color: var(--amber, #f5b840); }',
      '.fw-flow-who { font-family: var(--mono, monospace); color:#8ab4ff; }',
      '.fw-ask.todo { border-color: var(--border, rgba(255,255,255,.14)); background: transparent; }',
      '.fw-ask.todo .fw-ask-from { color: var(--text-dim, #a1a1a1); }',
      '.fw-ask-reply input { flex:1 1 auto; min-width:0; font-size:11.5px; padding:4px 8px; border-radius:6px; border:1px solid var(--border, rgba(255,255,255,.12)); background: var(--terminal-bg, #1e1e1e); color:inherit; }',
    ].join('\n');
    document.head.appendChild(style);
  }

  function paint(row, bytes) {
    if (!bytes || !bytes.length) return;
    feed(row.screen, bytes);
    row.out.textContent = screenText(row.screen, MAX_LINES);
    if (row.autoscroll) row.out.scrollTop = row.out.scrollHeight;
  }

  async function expand(sid) {
    const row = rows.get(sid);
    if (!row || row.open) return;
    row.open = true;
    row.el.classList.add('open');
    row.screen = makeScreen();
    // A zellij-backed run carries output_path: the pane's REAL tty stream,
    // captured by script(1). The PTY only shows the zellij client's repaint
    // protocol, which renders as bare frame lines here (André, 2026-08-31:
    // "here are just lines") — so the file is the readable source and the
    // PTY stream is only the fallback for plain sessions.
    if (row.meta && row.meta.output_path) {
      row.fileOffset = 0;
      const pull = async () => {
        if (!row.open) return;
        try {
          const chunk = await invoke('agent_run_output', { path: row.meta.output_path, offset: row.fileOffset });
          if (chunk && chunk.text) {
            row.fileOffset = chunk.next_offset;
            paint(row, new TextEncoder().encode(chunk.text));
          }
        } catch (_) {}
      };
      await pull();
      row.filePoll = setInterval(pull, 1000);
    } else {
      try {
        const b64 = await invoke('terminal_output_snapshot', { sessionId: sid });
        paint(row, b64Bytes(b64));
      } catch (_) {
        row.out.textContent = '(no output captured yet)';
      }
      // Live stream from here on. Payload matches the terminal listeners
      // elsewhere: base64 in event.payload (string) or payload.data.
      try {
        row.unlisten = await listen('terminal-output:' + sid, (event) => {
          const payload = event && event.payload;
          const b64 = typeof payload === 'string' ? payload : payload && payload.data;
          if (!b64) return;
          try { paint(row, b64Bytes(b64)); } catch (_) {}
        });
      } catch (_) {}
    }
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
    if (row.filePoll) { clearInterval(row.filePoll); row.filePoll = null; }
  }

  function upsertRow(meta) {
    const sid = meta.session_id;
    let row = rows.get(sid);
    if (row) row.meta = meta;
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
      row = { el, out, open: false, unlisten: null, autoscroll: true, screen: makeScreen(), meta };
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

  // An open ask or approval blocks an agent until a human answers. It lives
  // in the Mesh, and it still does: this is the same store, answered through
  // the same commands. What it adds is answering it WHERE THE WORK IS, so a
  // blocked run and its question are one glance apart instead of a pane
  // away (Andre, 2026-08-28).
  async function loadAsks() {
    if (!asksHost) return;
    let items = [];
    try {
      items = await invoke('inbox_list', { project: null, status: 'open' }) || [];
    } catch (_) { return; }
    // Todos belong here too. The handback of a finished ticket arrives as
    // one, and filtering it out meant the only message the loop produces was
    // invisible in the pane built for watching the loop.
    const blocking = items.filter((i) => i && ['ask', 'approve', 'todo'].includes(i.kind));
    asksHost.innerHTML = '';
    for (const item of blocking) {
      const el = document.createElement('div');
      el.className = 'fw-ask';
      const approve = item.kind === 'approve';
      const todo = item.kind === 'todo';
      if (todo) el.classList.add('todo');
      el.innerHTML =
        '<div class="fw-ask-top">' +
        '<span class="fw-ask-from">' + escapeText(item.from ? '@' + item.from : 'agent') + '</span>' +
        '<span class="fw-ask-title">' + escapeText(item.title || '(no title)') + '</span>' +
        '</div>' +
        (item.body ? '<div class="fw-ask-body">' + escapeText(item.body) + '</div>' : '') +
        '<div class="fw-ask-acts"></div>';
      const acts = el.querySelector('.fw-ask-acts');
      if (approve) {
        const yes = document.createElement('button');
        yes.className = 'approve';
        yes.textContent = 'Approve';
        yes.onclick = () => decide(item.id, 'approved');
        const no = document.createElement('button');
        no.className = 'deny';
        no.textContent = 'Deny';
        no.onclick = () => decide(item.id, 'denied');
        acts.append(yes, no);
      }
      // Options come from the agent; each is one click.
      for (const option of (item.options || [])) {
        const b = document.createElement('button');
        b.textContent = option.label || option.key;
        if (option.recommended) b.style.borderColor = 'var(--amber, #f5b840)';
        b.onclick = () => answer(item.id, option.key || option.label);
        acts.appendChild(b);
      }
      if (todo) {
        const done = document.createElement('button');
        done.textContent = 'Mark seen';
        done.onclick = () => answer(item.id, 'seen');
        acts.appendChild(done);
      } else if (!approve) {
        const row = document.createElement('div');
        row.className = 'fw-ask-reply';
        const input = document.createElement('input');
        input.type = 'text';
        input.placeholder = 'Type an answer and press Enter';
        input.onkeydown = (event) => {
          if (event.key !== 'Enter') return;
          const value = input.value.trim();
          if (value) answer(item.id, value);
        };
        row.appendChild(input);
        el.appendChild(row);
      }
      asksHost.appendChild(el);
    }
  }

  async function decide(id, decision) {
    try { await invoke('inbox_decide', { id, decision }); } catch (_) {}
    loadAsks();
  }

  async function answer(id, text) {
    try { await invoke('inbox_answer', { id, answer: text }); } catch (_) {}
    loadAsks();
  }

  const STATUS_WORDS = {
    inbox: 'filed', ready: 'ready to work', in_progress: 'picked up',
    review: 'handed back for review', blocked: 'blocked',
    done: 'finished', complete: 'approved and closed',
  };

  /// The loop, as sentences. A status change and an owner change are the two
  /// things that actually happen to a ticket, and every write records both,
  /// so the feed can say "XNAUT-233 handed back for review, now with
  /// @nautbot" instead of "ticket updated".
  async function loadFlow() {
    if (!flowHost) return;
    let events = [];
    try {
      events = await invoke('pm_event_list', { subject: null, limit: 60 }) || [];
    } catch (_) { return; }
    // Oldest first so "changed" means changed, then newest first to read.
    const seen = new Map();
    const lines = [];
    for (const event of [...events].reverse()) {
      const d = event.details || {};
      const id = event.subject || '';
      if (!id) continue;
      const prev = seen.get(id) || {};
      const parts = [];
      if (event.event === 'ticket.created') parts.push('filed');
      if (d.status && d.status !== prev.status) {
        parts.push(STATUS_WORDS[d.status] || String(d.status));
      }
      const hasOwner = Object.prototype.hasOwnProperty.call(d, 'owner');
      const owner = hasOwner ? (d.owner || null) : undefined;
      if (owner !== undefined && owner !== prev.owner) {
        parts.push(owner
          ? `now with <span class="fw-flow-who">@${escapeText(String(owner).replace(/^@/, ''))}</span>`
          : 'unassigned');
      }
      seen.set(id, { status: d.status || prev.status, owner: hasOwner ? owner : prev.owner });
      if (!parts.length) continue;
      lines.push({ id, what: parts.join(', '), at: event.timestamp });
    }
    lines.reverse();
    const rows = lines.slice(0, 25).map((line) => {
      const d = new Date(line.at);
      const when = Number.isNaN(d.getTime()) ? '' : d.toLocaleString(undefined, {
        day: '2-digit', month: 'short', hour: '2-digit', minute: '2-digit', hour12: false,
      });
      return '<div class="fw-flow-row"><span class="fw-flow-when">' + escapeText(when) + '</span>'
        + '<span class="fw-flow-what"><span class="fw-flow-id">' + escapeText(line.id) + '</span> ' + line.what + '</span></div>';
    }).join('');
    flowHost.innerHTML = '<div class="fw-flow-title">Ticket flow</div>'
      + (rows || '<div class="fw-empty">Nothing has moved yet.</div>');
  }

  function render() {
    if (!host) return;
    const list = [...sessions.values()].sort((a, b) => (b.started_at_ms || 0) - (a.started_at_ms || 0));
    const empty = host.querySelector('.fw-empty');
    if (empty) empty.style.display = list.length ? 'none' : 'block';
    for (const meta of list) {
      const row = upsertRow(meta);
      if (row.el.parentNode !== host) host.insertBefore(row.el, flowHost);
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
      unsubscribers.push(await listen('inbox-changed', () => { loadAsks(); loadFlow(); }));
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
      asksHost = document.createElement('div');
      asksHost.className = 'fw-asks';
      host.appendChild(asksHost);
      const empty = document.createElement('div');
      empty.className = 'fw-empty';
      empty.textContent = 'No agent sessions running. Wake one and it appears here.';
      host.appendChild(empty);
      flowHost = document.createElement('div');
      flowHost.className = 'fw-flow';
      host.appendChild(flowHost);
      container.appendChild(host);
      seed();
      subscribe();
      loadAsks();
      loadFlow();
      // Ticket writes come from agents and other processes, so there is no
      // event to listen for; a slow poll is honest and costs a directory read.
      flowTimer = setInterval(loadFlow, 15000);
    },
    setRoot() { /* project-independent: agents are machine-wide */ },
    destroy() {
      if (flowTimer) { clearInterval(flowTimer); flowTimer = null; }
      asksHost = null;
      flowHost = null;
      for (const sid of [...rows.keys()]) dropRow(sid);
      unsubscribers.forEach((u) => { try { u(); } catch (_) {} });
      unsubscribers = [];
      sessions.clear();
      if (host) { host.remove(); host = null; }
    },
  };

  // The Flow Watch TAB is gone (André 2026-08-30: "too many tabs"); the view
  // itself lives on, mounted at the top of the Agent pane's today group.
  window.xnautFlowWatchView = view;
  // Still registered with the right pane, in the shape the drain accepts
  // (XNAUT-282), so anything that addresses it by key keeps working.
  if (window.xnautRightPaneRegisterView) window.xnautRightPaneRegisterView('flowwatch', view);
  else (window.__xnautRightPaneQueue = window.__xnautRightPaneQueue || []).push({ key: 'flowwatch', view });
})();
