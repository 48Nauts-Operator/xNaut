// Ticket ids in rendered text become links with a hover card.
//
// Andre, 2026-09-14, reading Otto's release summary in Agent Space: "is it
// possible to add links to the tickets in the chat, so I could hover over
// them, and an overlay shows me the ticket?"
//
// One module for every surface that renders prose (Agent Space, the chat
// pane): window.xnautLinkTickets(root) walks the text nodes under root and
// wraps every `KEY-123` in an anchor. Hover shows the ticket (title, status,
// priority, owner, release, the first lines); click opens it in the Workspace
// Work tab. Tickets are read once per project through pm_ticket_list and
// cached for a minute, so a reply naming twenty ids costs one call.
(function () {
  'use strict';

  const ID = /\b([A-Z][A-Z0-9]{1,15})-(\d{1,6})\b/g;
  const SKIP = new Set(['A', 'CODE', 'PRE', 'SCRIPT', 'STYLE', 'TEXTAREA', 'INPUT', 'BUTTON']);
  const invoke = (cmd, args) => window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke(cmd, args);
  const esc = (s) => String(s == null ? '' : s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
  const LABELS = { inbox: 'Inbox', ready: 'Ready', in_progress: 'In progress', review: 'Review', blocked: 'Blocked', done: 'Done', complete: 'Complete' };

  // ---- lookup, cached per project ----
  const cache = new Map(); // project -> { at, byId }
  const TTL = 60_000;
  async function ticketsFor(project) {
    const hit = cache.get(project);
    if (hit && Date.now() - hit.at < TTL) return hit.byId;
    let list = [];
    try { list = (await invoke('pm_ticket_list', { project })) || []; } catch (_) { list = []; }
    const byId = new Map(list.map((t) => [t.id, t]));
    cache.set(project, { at: Date.now(), byId });
    return byId;
  }

  // ---- linkify ----
  function linkTickets(root) {
    if (!root) return 0;
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
      acceptNode(node) {
        if (!ID.test(node.nodeValue)) { ID.lastIndex = 0; return NodeFilter.FILTER_REJECT; }
        ID.lastIndex = 0;
        for (let el = node.parentElement; el && el !== root; el = el.parentElement) {
          if (SKIP.has(el.tagName) || el.classList.contains('xtl')) return NodeFilter.FILTER_REJECT;
        }
        return NodeFilter.FILTER_ACCEPT;
      },
    });
    const nodes = [];
    for (let n = walker.nextNode(); n; n = walker.nextNode()) nodes.push(n);
    let made = 0;
    for (const node of nodes) {
      const frag = document.createDocumentFragment();
      let last = 0;
      const text = node.nodeValue;
      text.replace(ID, (m, key, num, at) => {
        frag.appendChild(document.createTextNode(text.slice(last, at)));
        const a = document.createElement('a');
        a.className = 'xtl';
        a.href = '#';
        a.dataset.ticket = m;
        a.dataset.project = key;
        a.textContent = m;
        frag.appendChild(a);
        last = at + m.length;
        made += 1;
        return m;
      });
      frag.appendChild(document.createTextNode(text.slice(last)));
      node.parentNode.replaceChild(frag, node);
    }
    return made;
  }

  // ---- the card ----
  let card = null;
  let hideTimer = 0;
  function ensureCard() {
    if (card) return card;
    card = document.createElement('div');
    card.className = 'xtl-card';
    card.hidden = true;
    card.addEventListener('mouseenter', () => clearTimeout(hideTimer));
    card.addEventListener('mouseleave', hideSoon);
    document.body.appendChild(card);
    return card;
  }
  function hideSoon() { clearTimeout(hideTimer); hideTimer = setTimeout(() => { if (card) card.hidden = true; }, 180); }
  function place(anchor) {
    const r = anchor.getBoundingClientRect();
    const c = ensureCard();
    c.hidden = false;
    const w = Math.min(380, window.innerWidth - 24);
    c.style.width = `${w}px`;
    let left = Math.min(r.left, window.innerWidth - w - 12);
    let top = r.bottom + 6;
    if (top + c.offsetHeight > window.innerHeight - 8) top = Math.max(8, r.top - c.offsetHeight - 6);
    c.style.left = `${Math.max(8, left)}px`;
    c.style.top = `${top}px`;
  }
  function firstLines(body) {
    const lines = String(body || '').split('\n').map((l) => l.trim()).filter(Boolean);
    return lines.slice(0, 3).join(' ').slice(0, 240);
  }
  async function show(anchor) {
    clearTimeout(hideTimer);
    const id = anchor.dataset.ticket;
    const c = ensureCard();
    c.innerHTML = `<div class="xtl-head"><span class="xtl-id">${esc(id)}</span><span class="xtl-dim">looking up…</span></div>`;
    place(anchor);
    const t = (await ticketsFor(anchor.dataset.project)).get(id);
    if (c.hidden) return;
    if (!t) {
      c.innerHTML = `<div class="xtl-head"><span class="xtl-id">${esc(id)}</span><span class="xtl-dim">no such ticket</span></div>`;
      place(anchor);
      return;
    }
    const owner = t.owner ? '@' + String(t.owner).replace(/^@/, '') : 'unassigned';
    c.innerHTML = `
      <div class="xtl-head"><span class="xtl-id">${esc(t.id)}</span><span class="xtl-status" data-status="${esc(t.status)}">${esc(LABELS[t.status] || t.status)}</span></div>
      <div class="xtl-title">${esc(t.title)}</div>
      <div class="xtl-meta">${esc(t.type || '')} · ${esc(t.priority || '')} · ${esc(owner)}${t.release ? ` · release ${esc(t.release)}` : ''}</div>
      ${t.body ? `<div class="xtl-body">${esc(firstLines(t.body))}</div>` : ''}
      <div class="xtl-dim">click to open</div>`;
    place(anchor);
  }
  function open(anchor) {
    const id = anchor.dataset.ticket;
    if (typeof window.xnautOpenWorkspace === 'function') {
      window.xnautOpenWorkspace({ project: anchor.dataset.project, tab: 'work', ticket: id });
    }
    if (card) card.hidden = true;
  }

  document.addEventListener('mouseover', (e) => {
    const a = e.target && e.target.closest && e.target.closest('a.xtl');
    if (a) show(a);
  });
  document.addEventListener('mouseout', (e) => {
    const a = e.target && e.target.closest && e.target.closest('a.xtl');
    if (a) hideSoon();
  });
  document.addEventListener('click', (e) => {
    const a = e.target && e.target.closest && e.target.closest('a.xtl');
    if (!a) return;
    e.preventDefault();
    open(a);
  });

  const st = document.createElement('style');
  st.textContent = `
a.xtl { color:var(--accent,#f5b840); text-decoration:none; border-bottom:1px dotted currentColor; cursor:pointer; }
a.xtl:hover { border-bottom-style:solid; }
.xtl-card { position:fixed; z-index:10050; padding:10px 12px; border-radius:8px; font-size:12px; line-height:1.45;
  background:var(--bg-elevated,#1f2127); color:var(--text-primary,#e8e8ec); border:1px solid var(--border-color,#33363e);
  box-shadow:0 12px 32px rgba(0,0,0,.45); }
.xtl-head { display:flex; align-items:center; justify-content:space-between; gap:8px; margin-bottom:4px; }
.xtl-id { font-family:ui-monospace,Menlo,monospace; font-size:11px; color:var(--text-secondary,#a0a5af); }
.xtl-status { font-size:10px; padding:1px 7px; border-radius:999px; border:1px solid var(--border-color,#3a3d45); color:var(--text-secondary,#a0a5af); }
.xtl-status[data-status="in_progress"] { color:#f5b840; border-color:#f5b840; }
.xtl-status[data-status="review"] { color:#8ab4f8; border-color:#8ab4f8; }
.xtl-status[data-status="blocked"] { color:#eaa39c; border-color:#eaa39c; }
.xtl-status[data-status="done"],.xtl-status[data-status="complete"] { color:#7fd394; border-color:#7fd394; }
.xtl-title { font-weight:600; margin-bottom:3px; }
.xtl-meta { color:var(--text-secondary,#a0a5af); font-size:11px; margin-bottom:5px; }
.xtl-body { color:#c9ccd4; margin-bottom:5px; }
.xtl-dim { color:var(--text-muted,#777); font-size:10.5px; }
`;
  document.head.appendChild(st);

  window.xnautLinkTickets = linkTickets;
})();
