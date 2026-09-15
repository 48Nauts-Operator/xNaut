// Mesh — the human inbox (XNAUT-156), first menu entry.
//
// Designs: Paper file xNaut, page V3 (artboards 01-05).
// Backend: inbox.rs. Every mutation goes through a Tauri command and the
// backend emits `inbox-changed`, so open panes refresh without polling.
//
// Layout rule from the design: cards do not scale, rows do. One dense row
// per message; the expanded card is only the detail view. The action
// affordance is chosen by item KIND (approve / ask / todo / notify), but
// every kind keeps the free-text reply because any reply unblocks the run.
(function () {
  'use strict';

  const invoke = (...args) => window.__TAURI__.core.invoke(...args);
  const listen = (...args) => window.__TAURI__.event.listen(...args);
  const esc = (value) => String(value == null ? '' : value).replace(/[&<>"']/g, (c) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  }[c]));

  const KIND_PILL = {
    approve: { label: 'AWAITING YOU', color: '#f59e0b' },
    ask: { label: 'QUESTION', color: '#f5b840' },
    todo: { label: 'TO-DO', color: '#a0a0a0' },
    notify: { label: '', color: '' },
  };

  function ago(iso) {
    const then = Date.parse(iso || '');
    if (!Number.isFinite(then)) return '';
    const secs = Math.max(1, Math.round((Date.now() - then) / 1000));
    if (secs < 60) return `${secs}s`;
    if (secs < 3600) return `${Math.round(secs / 60)}m`;
    if (secs < 86400) return `${Math.round(secs / 3600)}h`;
    return `${Math.round(secs / 86400)}d`;
  }

  function initials(handle) {
    const clean = String(handle || '?').replace(/^@/, '');
    return clean.slice(0, 2).toUpperCase();
  }

  function greeting() {
    const hour = new Date().getHours();
    const part = hour < 12 ? 'morning' : hour < 18 ? 'afternoon' : 'evening';
    let name = '';
    try { name = String(localStorage.getItem('xnaut-user-name') || '').trim(); } catch (_) {}
    return `Good ${part}${name ? `, ${name}` : ''}.`;
  }

  function ensureStyles() {
    if (document.getElementById('mesh-panel-styles')) return;
    const style = document.createElement('style');
    style.id = 'mesh-panel-styles';
    style.textContent = `
      .mesh { --mesh-accent:#f5b840; display:flex; flex:1 1 auto; flex-direction:column; min-width:0; min-height:0;
        overflow-y:auto; color:var(--text-primary,#e0e0e0); background:var(--bg-primary,#0a0a0f);
        font-family:var(--font-ui,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif); }
      .mesh * { box-sizing:border-box; }
      .mesh-head { padding:30px 36px 0; }
      .mesh-date { font-family:var(--font-mono,monospace); font-size:10px; letter-spacing:.14em; color:#5a5a62; }
      .mesh-greeting { margin:8px 0 4px; font-size:26px; font-weight:620; letter-spacing:-.02em; color:var(--text-primary,#e0e0e0); }
      .mesh-status { font-size:13px; color:var(--text-secondary,#a0a0a0); }
      .mesh-tabs { display:flex; align-items:center; gap:8px; padding:16px 36px 0; }
      .mesh-tab { padding:6px 14px; border:1px solid var(--border,#2a2a2f); border-radius:7px; background:transparent;
        color:var(--text-secondary,#a0a0a0); font:inherit; font-size:12px; cursor:pointer; }
      .mesh-tab.active { background:var(--mesh-accent); border-color:var(--mesh-accent); color:#0a0a0f; font-weight:600; }
      .mesh-spacer { flex:1; }
      .mesh-bulk { display:flex; align-items:center; gap:9px; margin:14px 36px 0; padding:9px 13px;
        background:#241f10; border:1px solid var(--mesh-accent); border-radius:9px; }
      .mesh-bulk[hidden] { display:none; }
      .mesh-bulk-count { font-size:12px; font-weight:600; color:var(--text-primary,#e0e0e0); }
      .mesh-btn { padding:6px 14px; border:1px solid var(--border,#2a2a2f); border-radius:7px; background:transparent;
        color:var(--text-secondary,#a0a0a0); font:inherit; font-size:12px; cursor:pointer; }
      .mesh-btn:hover { color:var(--text-primary,#e0e0e0); }
      .mesh-btn.primary { background:var(--mesh-accent); border-color:var(--mesh-accent); color:#0a0a0f; font-weight:600; }
      .mesh-btn.danger { color:#ef4444; }
      .mesh-list { margin:16px 36px 36px; border:1px solid var(--border,#2a2a2f); border-radius:10px;
        background:var(--bg-secondary,#141419); overflow:hidden; }
      .mesh-row { display:flex; align-items:center; gap:11px; padding:11px 14px; border-bottom:1px solid #1c1c22; cursor:pointer; }
      .mesh-row:last-child { border-bottom:0; }
      .mesh-row:hover { background:rgba(255,255,255,.02); }
      .mesh-row.selected { background:#191713; }
      .mesh-check { width:14px; height:14px; flex:0 0 auto; border:1.3px solid #3a3a44; border-radius:3px; cursor:pointer;
        display:flex; align-items:center; justify-content:center; font-size:9px; font-weight:800; color:transparent; }
      .mesh-check.on { background:var(--mesh-accent); border-color:var(--mesh-accent); color:#0a0a0f; }
      .mesh-unread { width:6px; height:6px; flex:0 0 auto; border-radius:3px; background:transparent; }
      .mesh-unread.on { background:var(--mesh-accent); }
      .mesh-avatar { width:22px; height:22px; flex:0 0 auto; border-radius:6px; background:var(--bg-tertiary,#2a2a2f);
        color:var(--text-primary,#e0e0e0); display:flex; align-items:center; justify-content:center;
        font-family:var(--font-mono,monospace); font-size:8px; font-weight:700; }
      .mesh-copy { display:flex; flex-direction:column; gap:2px; flex:1; min-width:0; }
      .mesh-meta { display:flex; align-items:center; gap:7px; }
      .mesh-from { font-size:11px; font-weight:600; color:var(--text-secondary,#a0a0a0); }
      .mesh-pill { font-size:9px; font-weight:700; border-radius:999px; padding:1px 7px; border:1px solid currentColor; }
      .mesh-title { font-size:13px; font-weight:600; color:var(--text-primary,#e0e0e0); overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
      .mesh-preview { font-size:11px; color:#6a6a74; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
      .mesh-age { flex:0 0 auto; font-family:var(--font-mono,monospace); font-size:10px; color:#5a5a62; }
      .mesh-empty { padding:34px 16px; text-align:center; color:var(--text-secondary,#a0a0a0); font-size:13px; }
      .mesh-detail { margin:16px 36px 36px; display:flex; flex-direction:column; gap:14px; }
      .mesh-back { align-self:flex-start; background:transparent; border:0; color:var(--text-secondary,#a0a0a0);
        font:inherit; font-size:12px; cursor:pointer; padding:0; }
      .mesh-detail-head { display:flex; align-items:center; gap:11px; }
      .mesh-detail-title { font-size:20px; font-weight:620; color:var(--text-primary,#e0e0e0); }
      .mesh-detail-sub { font-size:11px; color:var(--text-secondary,#a0a0a0); }
      .mesh-card { background:var(--bg-secondary,#141419); border:1px solid var(--border,#2a2a2f); border-radius:10px;
        padding:16px 18px; display:flex; flex-direction:column; gap:12px; }
      .mesh-body { font-size:13px; line-height:1.6; color:var(--text-primary,#e0e0e0); white-space:pre-wrap; }
      .mesh-ctx { display:flex; flex-direction:column; border-top:1px solid #24242b; border-bottom:1px solid #24242b; }
      .mesh-ctx-row { display:flex; gap:12px; padding:8px 0; border-top:1px solid #1c1c22; }
      .mesh-ctx-row:first-child { border-top:0; }
      .mesh-ctx-key { width:210px; flex:0 0 auto; font-family:var(--font-mono,monospace); font-size:11px; color:var(--text-secondary,#a0a0a0); }
      /* pre-line so a multi-line value keeps its lines: a completed run
         reports its changed files as one entry (XNAUT-190), and HTML
         would otherwise run them together into one unreadable string. */
      .mesh-ctx-val { font-family:var(--font-mono,monospace); font-size:11px; color:var(--text-primary,#e0e0e0); white-space:pre-line; }
      .mesh-opt { display:flex; align-items:flex-start; gap:11px; border:1px solid var(--border,#2a2a2f); border-radius:9px;
        padding:12px 14px; cursor:pointer; }
      .mesh-opt.on { border-color:var(--mesh-accent); background:#191713; }
      .mesh-radio { width:15px; height:15px; flex:0 0 auto; margin-top:1px; border:1.5px solid #3a3a44; border-radius:8px;
        display:flex; align-items:center; justify-content:center; }
      .mesh-opt.on .mesh-radio { border-color:var(--mesh-accent); }
      .mesh-radio span { width:8px; height:8px; border-radius:4px; background:transparent; }
      .mesh-opt.on .mesh-radio span { background:var(--mesh-accent); }
      .mesh-opt-label { font-size:13px; font-weight:600; color:var(--text-primary,#e0e0e0); }
      .mesh-opt-detail { font-size:11px; color:#8a8a94; line-height:1.5; }
      .mesh-rec { font-size:9px; font-weight:700; color:#0a0a0f; background:var(--mesh-accent); border-radius:999px; padding:2px 8px; }
      .mesh-actions { display:flex; align-items:center; gap:8px; flex-wrap:wrap; }
      .mesh-reply { width:100%; min-height:70px; padding:10px 12px; resize:vertical; border:1px solid var(--border,#2a2a2f);
        border-radius:9px; background:var(--bg-primary,#0a0a0f); color:var(--text-primary,#e0e0e0); font:inherit; font-size:13px; }
      .mesh-waiting { font-size:12px; color:#f59e0b; }
      .mesh-links { display:flex; gap:8px; flex-wrap:wrap; }
      .mesh-link { font-size:11px; color:var(--mesh-accent); cursor:pointer; }
    `;
    document.head.appendChild(style);
  }

  function createMeshPanel(tabId, parent) {
    ensureStyles();
    const pane = document.createElement('section');
    pane.className = 'mesh';
    parent.appendChild(pane);

    let items = [];
    let tab = 'active';            // active (All) | you | jury | archived
    let selection = new Set();
    let openId = null;             // detail view when set
    let draftChoice = null;

    const isActive = (item) => item.status === 'open' || (item.context?.revocable === 'true' && item.status === 'done');
    // "You": what waits on the owner's hand. "Jury": what the jury raised or
    // decided, reviews attached, so its notices stop crowding the asks
    // (André, 2026-09-08: "a separate tab for jury approved tickets").
    const isJury = (item) => !!(item.context && item.context.jury_id);
    const needsYou = (item) => item.status === 'open' && (item.kind === 'ask' || item.kind === 'approve');
    const visible = () => items.filter((item) => {
      if (tab === 'you') return needsYou(item);
      if (tab === 'jury') return isJury(item) && item.status !== 'archived';
      if (tab === 'archived') return !isActive(item);
      return isActive(item); // All: everything live, jury included
    });

    async function load() {
      try {
        items = (await invoke('inbox_list', { project: null, status: null })) || [];
      } catch (error) {
        console.error('[mesh] list failed:', error);
        items = [];
      }
      render();
    }

    function statusLine() {
      const open = items.filter(isActive);
      const blocking = open.filter((item) => item.kind === 'ask' || item.kind === 'approve');
      if (blocking.length) return `<span style="color:#f59e0b">●</span>&nbsp; ${blocking.length} item${blocking.length === 1 ? '' : 's'} need you`;
      if (open.length) return `<span style="color:#a0a0a0">●</span>&nbsp; ${open.length} open item${open.length === 1 ? '' : 's'}, nothing blocking`;
      return '<span style="color:#10b981">●</span>&nbsp; Inbox clear.';
    }

    function rowMarkup(item) {
      const pill = KIND_PILL[item.kind] || KIND_PILL.notify;
      const pillHtml = pill.label
        ? `<span class="mesh-pill" style="color:${pill.color}">${pill.label}</span>` : '';
      const preview = (item.body || '').split('\n')[0].slice(0, 120);
      return `<div class="mesh-row ${selection.has(item.id) ? 'selected' : ''}" data-id="${esc(item.id)}">
        <span class="mesh-check ${selection.has(item.id) ? 'on' : ''}" data-check="${esc(item.id)}">✓</span>
        <span class="mesh-unread ${isActive(item) ? 'on' : ''}"></span>
        <span class="mesh-avatar">${esc(initials(item.from))}</span>
        <span class="mesh-copy">
          <span class="mesh-meta"><span class="mesh-from">${esc(item.from || 'system')}</span>${pillHtml}</span>
          <span class="mesh-title">${esc(item.title)}</span>
          <span class="mesh-preview">${esc(preview)}</span>
        </span>
        <span class="mesh-age">${esc(ago(item.at))}</span>
      </div>`;
    }

    function listMarkup() {
      const rows = visible();
      const unread = items.filter(isActive).length;
      const yours = items.filter(needsYou).length;
      const juryCount = items.filter((item) => isJury(item) && item.status !== 'archived').length;
      return `<div class="mesh-head">
          <div class="mesh-date">${esc(new Intl.DateTimeFormat(undefined, { weekday: 'long', day: 'numeric', month: 'long' }).format(new Date()))}</div>
          <h1 class="mesh-greeting">${esc(greeting())}</h1>
          <div class="mesh-status">${statusLine()}</div>
        </div>
        <div class="mesh-tabs">
          <button class="mesh-tab ${tab === 'active' ? 'active' : ''}" data-tab="active">All${unread ? ` · ${unread}` : ''}</button>
          <button class="mesh-tab ${tab === 'you' ? 'active' : ''}" data-tab="you">You${yours ? ` · ${yours}` : ''}</button>
          <button class="mesh-tab ${tab === 'jury' ? 'active' : ''}" data-tab="jury">Jury${juryCount ? ` · ${juryCount}` : ''}</button>
          <button class="mesh-tab ${tab === 'archived' ? 'active' : ''}" data-tab="archived">Archived</button>
          <span class="mesh-spacer"></span>
        </div>
        <div class="mesh-bulk" ${selection.size ? '' : 'hidden'}>
          <span class="mesh-check on">✓</span>
          <span class="mesh-bulk-count">${selection.size} selected</span>
          <span class="mesh-spacer"></span>
          <button class="mesh-btn primary" data-bulk="approve">Approve ${selection.size}</button>
          <button class="mesh-btn danger" data-bulk="deny">Deny ${selection.size}</button>
          <button class="mesh-btn" data-bulk="archive">Archive</button>
          <button class="mesh-btn" data-bulk="clear">Clear</button>
        </div>
        <div class="mesh-list">${rows.length ? rows.map(rowMarkup).join('') : '<div class="mesh-empty">Nothing here. Agents will reach you when they need a decision.</div>'}</div>`;
    }

    function optionsMarkup(item) {
      if (!item.options || !item.options.length) return '';
      return item.options.map((option) => `<div class="mesh-opt ${draftChoice === option.key ? 'on' : ''}" data-option="${esc(option.key)}">
          <span class="mesh-radio"><span></span></span>
          <span class="mesh-copy">
            <span class="mesh-meta"><span class="mesh-opt-label">${esc(option.key)} — ${esc(option.label)}</span>${option.recommended ? '<span class="mesh-rec">RECOMMENDED</span>' : ''}</span>
            ${option.detail ? `<span class="mesh-opt-detail">${esc(option.detail)}</span>` : ''}
          </span>
        </div>`).join('');
    }

    function contextMarkup(item) {
      const keys = Object.keys(item.context || {});
      if (!keys.length) return '';
      return `<div class="mesh-ctx">${keys.map((key) => `<div class="mesh-ctx-row">
          <span class="mesh-ctx-key">${esc(key)}</span><span class="mesh-ctx-val">${esc(item.context[key])}</span></div>`).join('')}</div>`;
    }

    // The action row is chosen by KIND — an approval is not a question and a
    // to-do is not either. Every kind keeps the reply box below it.
    function actionsMarkup(item) {
      if (item.context?.revocable === "true" && item.status !== "revoked") {
        return `<div class="mesh-actions"><button class="mesh-btn danger" data-decide="revoke">Revoke jury approval</button><button class="mesh-btn" data-status="archived">Archive</button></div>`;
      }
      if (!isActive(item)) {
        return `<div class="mesh-actions"><span class="mesh-waiting" style="color:#a0a0a0">Settled: ${esc(item.status)}${item.answer ? ` · “${esc(item.answer)}”` : ''}</span>
          <span class="mesh-spacer"></span><button class="mesh-btn" data-status="archived">Archive</button></div>`;
      }
      if (item.kind === 'approve') {
        // A refused Approve stays on the card with its reason, and a jury card
        // offers Re-review: retire the parked job and review the same green
        // record against the ticket as it is now (XNAUT-399).
        const refused = item.context && item.context.decide_error
          ? `<div class="mesh-refused" role="alert">Refused: ${esc(item.context.decide_error)}</div>` : '';
        const rereview = isJury(item)
          ? `<button class="mesh-btn" data-rereview="${esc(item.context.jury_id)}" title="Retire this review and start a fresh one on the current ticket">Re-review</button>` : '';
        return `${refused}<div class="mesh-actions">
          <button class="mesh-btn primary" data-decide="approved">Approve</button>
          <button class="mesh-btn danger" data-decide="denied">Deny</button>
          ${rereview}
          <span class="mesh-spacer"></span><button class="mesh-btn" data-status="archived">Archive</button></div>`;
      }
      if (item.kind === 'todo') {
        return `<div class="mesh-actions">
          <button class="mesh-btn primary" data-status="done">Mark done</button>
          <span class="mesh-spacer"></span><button class="mesh-btn" data-status="archived">Archive</button></div>`;
      }
      return `<div class="mesh-actions">
        <button class="mesh-btn primary" data-send>Send reply</button>
        <span class="mesh-waiting">The agent is waiting for your reply before continuing.</span>
        <span class="mesh-spacer"></span><button class="mesh-btn" data-status="archived">Archive</button></div>`;
    }

    function detailMarkup(item) {
      const pill = KIND_PILL[item.kind] || KIND_PILL.notify;
      return `<div class="mesh-detail">
        <button class="mesh-back" data-back>← Mesh</button>
        <div class="mesh-detail-head">
          <span class="mesh-avatar" style="width:30px;height:30px;border-radius:8px;font-size:10px">${esc(initials(item.from))}</span>
          <span class="mesh-copy">
            <span class="mesh-detail-title">${esc(item.title)}</span>
            <span class="mesh-detail-sub">${esc(item.from || 'system')} · ${esc(item.project || 'no project')} · ${esc(ago(item.at))} ago</span>
          </span>
          ${pill.label ? `<span class="mesh-pill" style="color:${pill.color}">${pill.label}</span>` : ''}
        </div>
        <div class="mesh-card">
          ${item.body ? `<div class="mesh-body">${esc(item.body)}</div>` : ''}
          ${contextMarkup(item)}
          ${optionsMarkup(item)}
          ${(item.links || []).length ? `<div class="mesh-links">${item.links.map((link) => `<span class="mesh-link" data-link="${esc(link.href)}">${esc(link.label || link.href)}</span>`).join('')}</div>` : ''}
          ${actionsMarkup(item)}
          ${isActive(item) && item.kind !== 'notify' ? '<textarea class="mesh-reply" data-reply placeholder="Type your answer… any reply unblocks the run"></textarea>' : ''}
        </div>
      </div>`;
    }

    function render() {
      const item = openId ? items.find((entry) => entry.id === openId) : null;
      pane.innerHTML = item ? detailMarkup(item) : listMarkup();
      wire(item);
    }

    async function decide(id, decision) {
      try { await invoke('inbox_decide', { id, decision }); } catch (error) {
        // A refused decision has a reason (stale review inputs, a policy the
        // jury will not accept, a ticket that moved on). Saying nothing made
        // the button look dead (André, 2026-09-08: "I can't approve").
        console.error('[mesh] decide failed:', error);
        refusal(String(error));
      }
      await load();
    }

    function refusal(message) {
      const node = document.createElement('div');
      node.className = 'mesh-refusal';
      node.setAttribute('role', 'alert');
      node.textContent = message;
      pane.prepend(node);
      setTimeout(() => node.remove(), 8000);
    }

    async function answer(id, text) {
      if (!text || !text.trim()) return;
      try { await invoke('inbox_answer', { id, answer: text.trim() }); } catch (error) { console.error('[mesh] answer failed:', error); }
      openId = null;
      await load();
    }

    function wire(item) {
      if (item) {
        pane.querySelector('[data-back]').onclick = () => { openId = null; draftChoice = null; render(); };
        pane.querySelectorAll('[data-option]').forEach((el) => {
          el.onclick = () => { draftChoice = el.dataset.option; render(); };
        });
        const reply = pane.querySelector('[data-reply]');
        if (reply && draftChoice) {
          const chosen = (item.options || []).find((option) => option.key === draftChoice);
          reply.value = chosen ? `${chosen.key} — ${chosen.label}` : draftChoice;
        }
        pane.querySelectorAll('[data-decide]').forEach((el) => {
          el.onclick = () => decide(item.id, el.dataset.decide);
        });
        pane.querySelectorAll('[data-rereview]').forEach((el) => {
          el.onclick = async () => {
            el.disabled = true;
            try { await invoke('jury_rereview', { juryId: el.dataset.rereview }); }
            catch (error) { console.error('[mesh] re-review failed:', error); refusal(String(error)); }
            openId = null;
            await load();
          };
        });
        pane.querySelectorAll('[data-status]').forEach((el) => {
          el.onclick = async () => {
            try { await invoke('inbox_set_status', { id: item.id, status: el.dataset.status }); }
            catch (error) { console.error('[mesh] status failed:', error); }
            openId = null;
            await load();
          };
        });
        const send = pane.querySelector('[data-send]');
        if (send) send.onclick = () => answer(item.id, reply ? reply.value : '');
        if (reply) reply.onkeydown = (event) => {
          if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) { event.preventDefault(); answer(item.id, reply.value); }
        };
        pane.querySelectorAll('[data-link]').forEach((el) => {
          el.onclick = () => {
            const href = el.dataset.link;
            if (window.xnautNewBrowserTab) window.xnautNewBrowserTab(href);
          };
        });
        return;
      }
      pane.querySelectorAll('[data-tab]').forEach((el) => {
        el.onclick = () => { tab = el.dataset.tab; selection.clear(); render(); };
      });
      pane.querySelectorAll('[data-check]').forEach((el) => {
        el.onclick = (event) => {
          event.stopPropagation();
          const id = el.dataset.check;
          if (selection.has(id)) selection.delete(id); else selection.add(id);
          render();
        };
      });
      pane.querySelectorAll('.mesh-row').forEach((el) => {
        el.onclick = () => { openId = el.dataset.id; draftChoice = null; render(); };
      });
      pane.querySelectorAll('[data-bulk]').forEach((el) => {
        el.onclick = async () => {
          const action = el.dataset.bulk;
          if (action === 'clear') { selection.clear(); render(); return; }
          const ids = Array.from(selection);
          try { await invoke('inbox_bulk', { ids, action }); } catch (error) { console.error('[mesh] bulk failed:', error); }
          selection.clear();
          await load();
        };
      });
    }

    const unlisten = listen('inbox-changed', () => load());
    load();

    return {
      kind: 'mesh',
      label: `mesh-${tabId}`,
      pane,
      dispose() { Promise.resolve(unlisten).then((off) => { try { off(); } catch (_) {} }).catch(() => {}); },
    };
  }

  window.xnautCreateMeshPanel = createMeshPanel;
  window.xnautOpenMesh = () => {
    if (window.xnautHomeContext) window.xnautHomeContext();
    return window.xnautAttachSingletonPanelTab
      ? window.xnautAttachSingletonPanelTab('Mesh', 'xnautCreateMeshPanel', {})
      : window.xnautAttachPanelTab && window.xnautAttachPanelTab('Mesh', 'xnautCreateMeshPanel', {});
  };
})();
