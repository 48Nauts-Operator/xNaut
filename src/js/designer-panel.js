// Designer (XNAUT-61) — the Paper replacement inside a project workspace.
// Two views: a list of the project's designs, and the canvas (live sandbox
// build in the middle, chat on the right). A design is always a REAL project:
// the agent scaffolds and edits actual source, a GitVM sandbox builds and
// serves it, and the canvas is an iframe on that running site.
(function () {
  'use strict';

  const invoke = (...a) => window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke(...a);
  const esc = (s) => String(s == null ? '' : s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));

  const KINDS = [
    ['website', 'Website', '#F5B840', '#4A3D22'],
    ['deck', 'Deck', '#5BD1C9', '#234A48'],
    ['document', 'Document', '#7EC98F', '#2A4A2E'],
    ['appui', 'App UI', '#8B7BE8', '#332C55'],
  ];
  const kindOf = (k) => KINDS.find((x) => x[0] === k) || KINDS[0];

  function ago(ms) {
    if (!ms) return '';
    const m = Math.max(0, Math.floor((Date.now() - ms) / 60000));
    if (m < 1) return 'just now';
    if (m < 60) return m + 'm ago';
    const h = Math.floor(m / 60);
    if (h < 24) return h + 'h ago';
    return Math.floor(h / 24) + 'd ago';
  }
  const minsLeft = (ms) => Math.max(0, Math.round((ms - Date.now()) / 60000));

  let styled = false;
  function injectStyles() {
    if (styled) return; styled = true;
    const st = document.createElement('style');
    st.textContent = `
.dsg { display:flex; flex-direction:column; gap:18px; }
.dsg-head { display:flex; align-items:flex-start; gap:14px; }
.dsg-head .t { display:flex; flex-direction:column; gap:4px; flex:1 1 auto; }
.dsg-head h3 { margin:0; font-size:15px; font-weight:700; color:var(--foreground,#fafafa); }
.dsg-head p { margin:0; font-size:12px; color:var(--muted-foreground,#a1a1a1); }
.dsg-btn { display:flex; align-items:center; gap:7px; height:34px; padding:0 16px; border:0; border-radius:9px; background:var(--xnaut-yellow,#f5b840); color:#171717; font:inherit; font-weight:700; font-size:12.5px; cursor:pointer; }
.dsg-btn.ghost { background:transparent; border:1px solid var(--border,#262626); color:var(--foreground); font-weight:600; }
.dsg-filters { display:flex; align-items:center; gap:8px; flex-wrap:wrap; }
.dsg-chip { display:inline-flex; align-items:center; height:28px; padding:0 13px; border-radius:999px; border:1px solid var(--border,#262626); background:transparent; color:var(--muted-foreground,#a1a1a1); font:inherit; font-size:11.5px; cursor:pointer; }
.dsg-chip.on { background:#1c1f26; border-color:#2a2e37; color:var(--foreground,#fafafa); font-weight:600; }
.dsg-grid { display:flex; flex-wrap:wrap; gap:20px; }
.dsg-card { width:358px; background:#12141a; border:1px solid #2a2e37; border-radius:12px; overflow:hidden; display:flex; flex-direction:column; cursor:pointer; }
.dsg-card.archived { opacity:.55; background:#0f1116; border-color:#1c1f26; }
.dsg-card.ghost { align-items:center; justify-content:center; gap:9px; height:232px; border:1px dashed #2a2e37; background:transparent; }
.dsg-thumb { height:168px; background:#0a0b0e; display:flex; align-items:center; justify-content:center; }
.dsg-thumb .mark { font-size:26px; }
.dsg-meta { display:flex; align-items:center; gap:10px; padding:13px 14px; }
.dsg-meta .c { display:flex; flex-direction:column; gap:3px; flex:1 1 auto; min-width:0; }
.dsg-meta .n { font-size:13.5px; font-weight:600; color:var(--foreground,#fafafa); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
.dsg-meta .s { font-family:ui-monospace,Menlo,monospace; font-size:10.5px; color:var(--muted-foreground,#a1a1a1); }
.dsg-kind { display:inline-flex; align-items:center; height:20px; padding:0 8px; border-radius:5px; font-family:ui-monospace,Menlo,monospace; font-size:9px; flex-shrink:0; }
.dsg-live { display:inline-flex; align-items:center; gap:5px; height:20px; padding:0 8px; border-radius:999px; border:1px solid #24402c; background:#0f1a14; font-family:ui-monospace,Menlo,monospace; font-size:9px; color:#7ec98f; flex-shrink:0; }
.dsg-live i { width:5px; height:5px; border-radius:50%; background:#7ec98f; display:block; }
.dsg-menu { border:0; background:transparent; color:#5d6268; font:inherit; font-size:14px; cursor:pointer; padding:0 2px; flex-shrink:0; }
.dsg-empty { padding:26px 0; font-size:12px; color:var(--muted-foreground,#a1a1a1); }
/* Inline create form — native prompt() is a no-op in Tauri's WKWebView. */
.dsg-new { display:flex; flex-direction:column; gap:10px; padding:16px; background:#12141a; border:1px solid #2a2e37; border-radius:12px; }
.dsg-new label { font-size:10px; letter-spacing:.09em; font-weight:650; color:var(--muted-foreground); text-transform:uppercase; }
.dsg-new input, .dsg-new select { height:36px; background:#0b0c10; border:1px solid #262626; border-radius:9px; color:var(--foreground); font:inherit; font-size:13px; padding:0 11px; outline:none; }
.dsg-new input:focus, .dsg-new select:focus { border-color:var(--xnaut-yellow,#f5b840); }
.dsg-new .row { display:flex; gap:10px; }
.dsg-new .row > div { display:flex; flex-direction:column; gap:5px; flex:1 1 0; }
.dsg-new .acts { display:flex; gap:8px; justify-content:flex-end; }

/* ---- canvas view ---- */
.dsgc { position:fixed; inset:0; z-index:60; display:flex; flex-direction:column; background:#0d0e12; }
.dsgc-bar { display:flex; align-items:center; gap:12px; height:56px; padding:0 16px; border-bottom:1px solid #1c1f26; flex-shrink:0; }
.dsgc-back { border:0; background:transparent; color:var(--muted-foreground); font:inherit; font-size:16px; cursor:pointer; }
.dsgc-title { display:flex; flex-direction:column; gap:1px; min-width:0; }
.dsgc-title b { font-size:13.5px; font-weight:600; color:var(--foreground); }
.dsgc-title span { font-family:ui-monospace,Menlo,monospace; font-size:9.5px; color:#5d6268; }
.dsgc-body { flex:1 1 auto; min-height:0; display:flex; }
.dsgc-canvas { flex:1 1 auto; min-width:0; background:#08090c; display:flex; align-items:center; justify-content:center; }
.dsgc-canvas iframe { width:100%; height:100%; border:0; background:#fff; }
.dsgc-state { display:flex; flex-direction:column; align-items:center; gap:10px; color:var(--muted-foreground); font-size:12.5px; }
.dsgc-chat { width:380px; flex-shrink:0; display:flex; flex-direction:column; background:#0b0c10; border-left:1px solid #1c1f26; }
.dsgc-thread { flex:1 1 auto; min-height:0; overflow-y:auto; display:flex; flex-direction:column; gap:14px; padding:16px; }
.dsgc-u { display:flex; justify-content:flex-end; }
.dsgc-u > div { max-width:80%; background:#1c1f26; border-radius:11px; padding:10px 12px; font-size:12px; line-height:18px; color:#edeff2; white-space:pre-wrap; }
.dsgc-a { display:flex; gap:9px; }
.dsgc-a .av { width:24px; height:24px; flex-shrink:0; border-radius:7px; background:#3a2c12; color:#f5b840; display:flex; align-items:center; justify-content:center; font-size:11px; }
.dsgc-a .bub { flex:1 1 auto; background:#12141a; border:1px solid #1c1f26; border-radius:11px; padding:10px 12px; font-size:12px; line-height:18px; color:#c9cdd4; }
.dsgc-files { display:flex; flex-direction:column; gap:3px; border-top:1px solid #1c1f26; margin-top:7px; padding-top:7px; font-family:ui-monospace,Menlo,monospace; font-size:10px; color:#7ec98f; }
.dsgc-comp { display:flex; flex-direction:column; gap:9px; padding:12px 14px 16px; border-top:1px solid #1c1f26; flex-shrink:0; }
.dsgc-quick { display:flex; gap:6px; flex-wrap:wrap; }
.dsgc-quick button { height:24px; padding:0 10px; border-radius:999px; border:1px solid #1c1f26; background:transparent; color:var(--muted-foreground); font:inherit; font-size:10.5px; cursor:pointer; }
.dsgc-row { display:flex; align-items:flex-end; gap:8px; }
.dsgc-row textarea { flex:1 1 auto; min-height:44px; max-height:140px; resize:vertical; background:#12141a; border:1px solid #1c1f26; border-radius:11px; color:var(--foreground); font:inherit; font-size:12px; padding:12px; outline:none; }
.dsgc-row textarea:focus { border-color:var(--xnaut-yellow,#f5b840); }
.dsgc-send { width:40px; height:40px; flex-shrink:0; border:0; border-radius:10px; background:var(--xnaut-yellow,#f5b840); color:#171717; font:inherit; font-weight:700; font-size:14px; cursor:pointer; }`;
    document.head.appendChild(st);
  }

  // ---- list view --------------------------------------------------------------
  function createDesigner() {
    let host = null, project = null, designs = [], filter = 'all', busy = false, creating = false;

    async function load() {
      try { designs = (await invoke('designer_list', { project: project.name })) || []; }
      catch (e) { designs = []; console.warn('[designer] list failed:', e); }
      render();
    }

    function visible() {
      if (filter === 'archived') return designs.filter((d) => d.archived);
      const live = designs.filter((d) => !d.archived);
      return filter === 'all' ? live : live.filter((d) => d.kind === filter);
    }

    function cardHtml(d, i) {
      const k = kindOf(d.kind);
      const live = d.sandbox_id && d.sandbox_expires_ms > Date.now();
      const sub = d.archived ? 'archived' : ago(d.updated_at_ms);
      return `<div class="dsg-card${d.archived ? ' archived' : ''}" data-open="${i}">
        <div class="dsg-thumb"><span class="mark" style="color:${k[2]}">${d.kind === 'deck' ? '▭' : d.kind === 'document' ? '▤' : d.kind === 'appui' ? '▣' : '◫'}</span></div>
        <div class="dsg-meta">
          <div class="c"><span class="n">${esc(d.name)}</span><span class="s">${esc(sub)}</span></div>
          ${live ? `<span class="dsg-live"><i></i>live</span>` : ''}
          <span class="dsg-kind" style="color:${k[2]};border:1px solid ${k[3]}">${k[1].toUpperCase()}</span>
          <button class="dsg-menu" data-menu="${i}" title="${d.archived ? 'Restore' : 'Archive'}">${d.archived ? '↩' : '⌸'}</button>
        </div>
      </div>`;
    }

    function render() {
      if (!host) return;
      const list = visible();
      const counts = (kind) => designs.filter((d) => !d.archived && d.kind === kind).length;
      host.innerHTML = `<div class="dsg">
        <div class="dsg-head">
          <div class="t"><h3>Designer</h3><p>Websites, decks, documents and app UI for ${esc(project.name)} — each one a real build on its own domain.</p></div>
          <button class="dsg-btn" data-new>✦ New design</button>
        </div>
        <div class="dsg-filters">
          <button class="dsg-chip${filter === 'all' ? ' on' : ''}" data-f="all">All ${designs.filter((d) => !d.archived).length}</button>
          ${KINDS.map(([k, label]) => `<button class="dsg-chip${filter === k ? ' on' : ''}" data-f="${k}">${label} ${counts(k)}</button>`).join('')}
          <span style="flex:1 1 auto"></span>
          <button class="dsg-chip${filter === 'archived' ? ' on' : ''}" data-f="archived">Archived ${designs.filter((d) => d.archived).length}</button>
        </div>
        ${creating ? `<div class="dsg-new">
          <div class="row">
            <div><label>Name</label><input data-name placeholder="Marketing site — v1" value=""></div>
            <div><label>Kind</label><select data-kind>${KINDS.map(([k, l]) => `<option value="${k}">${l}</option>`).join('')}</select></div>
          </div>
          <div class="acts">
            <button class="dsg-btn ghost" data-cancel>Cancel</button>
            <button class="dsg-btn" data-create>Create &amp; open</button>
          </div>
        </div>` : ''}
        <div class="dsg-grid">
          ${list.map(cardHtml).join('')}
          ${filter === 'archived' || creating ? '' : `<div class="dsg-card ghost" data-new>
            <span style="font-size:22px;color:var(--xnaut-yellow,#f5b840)">✦</span>
            <span style="font-size:13px;font-weight:600;color:#c9cdd4">New design</span>
            <span style="font-size:11px;color:#5d6268">Website · Deck · Document · App UI</span>
          </div>`}
        </div>
        ${list.length ? '' : `<div class="dsg-empty">${filter === 'archived' ? 'Nothing archived.' : 'No designs yet — describe what you want and it gets built for real.'}</div>`}
      </div>`;

      host.querySelectorAll('[data-f]').forEach((b) => { b.onclick = () => { filter = b.dataset.f; render(); }; });
      host.querySelectorAll('[data-new]').forEach((b) => {
        b.onclick = () => { creating = true; render(); const n = host.querySelector('[data-name]'); if (n) n.focus(); };
      });
      const cancel = host.querySelector('[data-cancel]');
      if (cancel) cancel.onclick = () => { creating = false; render(); };
      const create = host.querySelector('[data-create]');
      if (create) create.onclick = submitNew;
      const nameInput = host.querySelector('[data-name]');
      if (nameInput) nameInput.onkeydown = (ev) => { if (ev.key === 'Enter') { ev.preventDefault(); submitNew(); } };
      host.querySelectorAll('[data-open]').forEach((b) => {
        b.onclick = (ev) => {
          if (ev.target.closest('[data-menu]')) return;
          openCanvas(project, visible()[+b.dataset.open], load);
        };
      });
      // Archive / restore toggle. Rename lives in the canvas bar (click the
      // name) — no prompt() anywhere, it is a no-op in Tauri's WKWebView.
      host.querySelectorAll('[data-menu]').forEach((b) => {
        b.onclick = async (ev) => {
          ev.stopPropagation();
          const d = visible()[+b.dataset.menu];
          try {
            await invoke('designer_set_archived', { project: project.name, slug: d.slug, archived: !d.archived });
          } catch (e) { console.error('[designer] archive failed:', e); }
          load();
        };
      });
    }

    async function submitNew() {
      if (busy) return;
      const name = (host.querySelector('[data-name]').value || '').trim() || 'Untitled design';
      const kind = host.querySelector('[data-kind]').value || 'website';
      busy = true;
      try {
        const d = await invoke('designer_create', { project: project.name, name, kind });
        creating = false;
        await load();
        openCanvas(project, d, load);
      } catch (e) {
        console.error('[designer] create failed:', e);
        const box = host.querySelector('.dsg-new');
        if (box) box.insertAdjacentHTML('beforeend', `<span style="font-size:11px;color:#e98b83">${esc(String(e))}</span>`);
      }
      busy = false;
    }

    return {
      mount(el, proj) { injectStyles(); host = el; project = proj; load(); },
    };
  }

  // ---- canvas view ------------------------------------------------------------
  // Full-screen overlay: live sandbox build in the middle, chat on the right.
  function openCanvas(project, design, onClose) {
    injectStyles();
    let d = design, renewTimer = null, sending = false;
    const root = document.createElement('div');
    root.className = 'dsgc';
    document.body.appendChild(root);

    const close = async () => {
      clearInterval(renewTimer);
      root.remove();
      if (onClose) onClose();
    };

    function canvasHtml() {
      const live = d.sandbox_id && d.sandbox_expires_ms > Date.now();
      if (live && d.public_url) return `<iframe src="${esc(d.public_url)}" sandbox="allow-scripts allow-same-origin allow-forms"></iframe>`;
      return `<div class="dsgc-state"><span>${d.messages && d.messages.length ? 'Sandbox stopped — reopening rebuilds it.' : 'Describe what you want on the right. It gets scaffolded, built and served for real.'}</span>
        <button class="dsg-btn" data-spin>▸ ${d.messages && d.messages.length ? 'Restart build' : 'Start sandbox'}</button></div>`;
    }

    function threadHtml() {
      const msgs = d.messages || [];
      if (!msgs.length) return `<div class="dsgc-a"><div class="av">✦</div><div class="bub">Tell me what to build — “a marketing site for ${esc(project.name)}, warm neutrals, amber accent”. I scaffold a real project, build it in a sandbox and serve it on its own domain.</div></div>`;
      return msgs.map((m) => m.role === 'user'
        ? `<div class="dsgc-u"><div>${esc(m.text)}</div></div>`
        : `<div class="dsgc-a"><div class="av">✦</div><div class="bub">${esc(m.text)}${(m.files || []).length ? `<div class="dsgc-files">${m.files.map(esc).join('<br>')}</div>` : ''}</div></div>`).join('');
    }

    function render() {
      const k = kindOf(d.kind);
      const live = d.sandbox_id && d.sandbox_expires_ms > Date.now();
      root.innerHTML = `
        <div class="dsgc-bar">
          <button class="dsgc-back" data-back>‹</button>
          <div class="dsgc-title"><b data-rename title="Click to rename">${esc(d.name)}</b><span>${esc(project.name)} · work/${esc(project.name)}/Design/${esc(d.slug)}</span></div>
          <span class="dsg-kind" style="color:${k[2]};border:1px solid ${k[3]}">${k[1].toUpperCase()}</span>
          ${live ? `<span class="dsg-live"><i></i>${esc(String(d.public_url).replace(/^https?:\/\//, ''))} · ${minsLeft(d.sandbox_expires_ms)}m left</span>` : ''}
          <span style="flex:1 1 auto"></span>
          ${live ? `<button class="dsg-btn ghost" data-open-ext>Open ↗</button><button class="dsg-btn ghost" data-stop>■ Stop</button>` : ''}
        </div>
        <div class="dsgc-body">
          <div class="dsgc-canvas">${canvasHtml()}</div>
          <div class="dsgc-chat">
            <div class="dsgc-thread">${threadHtml()}</div>
            <div class="dsgc-comp">
              <div class="dsgc-quick">
                <button data-q="Add a page">Add a page</button>
                <button data-q="Change the palette">Change palette</button>
                <button data-q="Make it responsive">Make responsive</button>
              </div>
              <div class="dsgc-row">
                <textarea data-input placeholder="Describe what to build or change…"></textarea>
                <button class="dsgc-send" data-send>↑</button>
              </div>
            </div>
          </div>
        </div>`;

      root.querySelector('[data-back]').onclick = close;
      // Rename inline: the title becomes an input on click (no prompt()).
      const titleEl = root.querySelector('[data-rename]');
      if (titleEl) titleEl.onclick = () => {
        const input = document.createElement('input');
        input.value = d.name;
        input.style.cssText = 'background:#0b0c10;border:1px solid #f5b840;border-radius:6px;color:#fafafa;font:inherit;font-size:13.5px;font-weight:600;padding:2px 6px;width:220px;outline:none';
        titleEl.replaceWith(input);
        input.focus();
        input.select();
        const commit = async () => {
          const name = (input.value || '').trim();
          if (name && name !== d.name) {
            try { d = await invoke('designer_rename', { project: project.name, slug: d.slug, name }); }
            catch (e) { console.error('[designer] rename failed:', e); }
          }
          render();
        };
        input.onblur = commit;
        input.onkeydown = (ev) => { if (ev.key === 'Enter') { ev.preventDefault(); commit(); } if (ev.key === 'Escape') render(); };
      };
      const bind = (sel, fn) => { const el = root.querySelector(sel); if (el) el.onclick = fn; };
      bind('[data-spin]', spinUp);
      bind('[data-stop]', stop);
      bind('[data-open-ext]', () => { if (window.xnautOpenBrowserTab) window.xnautOpenBrowserTab(d.public_url); });
      bind('[data-send]', send);
      root.querySelectorAll('[data-q]').forEach((b) => {
        b.onclick = () => { const t = root.querySelector('[data-input]'); t.value = b.dataset.q; t.focus(); };
      });
      const input = root.querySelector('[data-input]');
      if (input) input.onkeydown = (ev) => { if (ev.key === 'Enter' && !ev.shiftKey) { ev.preventDefault(); send(); } };
    }

    async function refresh() {
      try { d = await invoke('designer_get', { project: project.name, slug: d.slug }); } catch (_) {}
      render();
    }

    async function spinUp() {
      const canvas = root.querySelector('.dsgc-canvas');
      if (canvas) canvas.innerHTML = '<div class="dsgc-state"><span>Spinning up the sandbox and building…</span></div>';
      try {
        d = await invoke('designer_spin_up', { project: project.name, slug: d.slug });
        startRenew();
      } catch (e) {
        console.error('[designer] spin up failed:', e);
        const c = root.querySelector('.dsgc-canvas');
        if (c) c.innerHTML = `<div class="dsgc-state"><span style="color:#e98b83">Sandbox failed to start: ${esc(String(e))}</span><button class="dsg-btn" data-spin>Retry</button></div>`;
      }
      render();
    }

    async function stop() {
      clearInterval(renewTimer);
      try { d = await invoke('designer_stop', { project: project.name, slug: d.slug }); }
      catch (e) { console.error('[designer] stop failed:', e); }
      render();
    }

    // Renew well before the lease expires; each renew also checkpoints the
    // source back to the vault, so a surprise teardown cannot lose work.
    function startRenew() {
      clearInterval(renewTimer);
      renewTimer = setInterval(async () => {
        if (!d.sandbox_id) return;
        try { d = await invoke('designer_renew', { project: project.name, slug: d.slug }); render(); }
        catch (e) { console.warn('[designer] renew failed:', e); }
      }, 10 * 60 * 1000);
    }

    async function send() {
      if (sending) return;
      const input = root.querySelector('[data-input]');
      const text = (input.value || '').trim();
      if (!text) { input.focus(); return; }
      sending = true;
      input.value = '';
      await invoke('designer_append_message', { project: project.name, slug: d.slug,
        message: { role: 'user', text, files: [], at_ms: Date.now() } }).catch(() => {});
      await refresh();
      // The build happens in the sandbox — make sure one is running first.
      if (!d.sandbox_id || d.sandbox_expires_ms <= Date.now()) await spinUp();
      try {
        const reply = await window.xnautDesignerAgent.run(project, d, text);
        await invoke('designer_append_message', { project: project.name, slug: d.slug,
          message: { role: 'agent', text: reply.text, files: reply.files || [], at_ms: Date.now() } }).catch(() => {});
      } catch (e) {
        await invoke('designer_append_message', { project: project.name, slug: d.slug,
          message: { role: 'agent', text: 'Build failed: ' + e, files: [], at_ms: Date.now() } }).catch(() => {});
      }
      sending = false;
      await refresh();
      const frame = root.querySelector('.dsgc-canvas iframe');
      if (frame) frame.src = frame.src; // hot reload already rebuilt — force the view
    }

    render();
    if (d.sandbox_id && d.sandbox_expires_ms > Date.now()) startRenew();
  }

  window.xnautDesigner = createDesigner();
})();
