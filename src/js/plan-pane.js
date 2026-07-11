// Plan Mode pane — a two-pane planning workspace: chat (left) + a living
// markdown plan document (right). The chat runs in "plan mode" (solution-
// architect persona, Engram-grounded); whenever the agent emits the full plan
// as a ```markdown block, it is written to <project>/PLAN.md and shown in the
// right-hand doc pane.
//
// The doc pane is dependency-free (no CDN TipTap, which fails under this
// WebKit). It has two modes: Preview (rendered markdown) and Edit (raw
// textarea). Edits autosave to PLAN.md; the agent overwrites it on revision.
//
// Registered as a panel factory: xnautAttachPanelTab → window.xnautCreatePlanPane.
(function () {
  'use strict';

  const invoke = (...a) => window.__TAURI__.core.invoke(...a);
  const MODE_TOGGLE_HTML = `
    <button data-mode="preview" data-active="1" title="Preview" aria-label="Preview">
      <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M1.8 8s2.3-4 6.2-4 6.2 4 6.2 4-2.3 4-6.2 4-6.2-4-6.2-4z"/><circle cx="8" cy="8" r="2"/></svg>
    </button>
    <button data-mode="edit" title="Edit" aria-label="Edit">
      <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M3 13l1-3 6.8-6.8a1.4 1.4 0 0 1 2 2L6 12z"/><path d="M9.8 4.2l2 2"/></svg>
    </button>`;

  function injectStyles() {
    if (document.getElementById('plan-doc-styles')) return;
    const st = document.createElement('style');
    st.id = 'plan-doc-styles';
    st.textContent = `
.plan-doc-view { flex:1 1 0%; min-height:0; overflow:auto; padding:24px 32px; color:var(--text, #d7dae0); font-family:-apple-system,"SF Pro Text",Segoe UI,Roboto,sans-serif; font-size:14px; line-height:1.65; }
.plan-doc-view h1 { font-size:24px; font-weight:700; margin:0 0 20px; padding-bottom:10px; border-bottom:1px solid var(--border, rgba(255,255,255,.1)); }
.plan-doc-view h2 { font-size:19px; font-weight:650; margin:34px 0 12px; padding-bottom:6px; border-bottom:1px solid var(--border, rgba(255,255,255,.08)); }
.plan-doc-view h3 { font-size:16px; font-weight:600; margin:28px 0 10px; color:var(--agent-thinking, #4dffd0); }
.plan-doc-view h4 { font-size:14px; font-weight:600; margin:22px 0 8px; }
.plan-doc-view h1:first-child, .plan-doc-view h2:first-child, .plan-doc-view h3:first-child, .plan-doc-view h4:first-child { margin-top:0; }
.plan-doc-view p { margin:0 0 12px; }
.plan-doc-view ul, .plan-doc-view ol { margin:0 0 12px; padding-left:24px; }
.plan-doc-view li { margin:3px 0; }
.plan-doc-view li.task { list-style:none; margin-left:-20px; }
.plan-doc-view li.task input { margin-right:8px; vertical-align:middle; accent-color:var(--agent-thinking, #4dffd0); }
.plan-doc-view a { color:var(--accent, #4f8cff); text-decoration:none; }
.plan-doc-view a:hover { text-decoration:underline; }
.plan-doc-view code { font-family:"SF Mono",Menlo,monospace; font-size:.88em; background:var(--input-bg, rgba(255,255,255,.06)); padding:1px 5px; border-radius:4px; }
.plan-doc-view pre { background:var(--input-bg, rgba(255,255,255,.05)); border:1px solid var(--border, rgba(255,255,255,.1)); border-radius:8px; padding:12px 14px; overflow:auto; margin:0 0 14px; }
.plan-doc-view pre code { background:none; padding:0; font-size:12.5px; line-height:1.5; }
.plan-doc-view .mermaid { margin:0 0 14px; text-align:center; background:var(--input-bg, rgba(255,255,255,.04)); border:1px solid var(--border, rgba(255,255,255,.08)); border-radius:8px; padding:14px; overflow:auto; }
.plan-doc-view .mermaid svg { max-width:100%; height:auto; }
.plan-doc-view blockquote { margin:0 0 12px; padding:4px 14px; border-left:3px solid var(--agent-thinking, #4dffd0); color:var(--text-muted, #9aa0aa); }
.plan-doc-view hr { border:none; border-top:1px solid var(--border, rgba(255,255,255,.12)); margin:20px 0; }
.plan-doc-view table { border-collapse:separate; border-spacing:0; width:100%; margin:16px 0 22px; font-size:13px; line-height:1.5; border:1px solid var(--border, rgba(255,255,255,.14)); border-radius:8px; overflow:hidden; background:rgba(255,255,255,.025); }
.plan-doc-view th, .plan-doc-view td { border-right:1px solid var(--border, rgba(255,255,255,.1)); border-bottom:1px solid var(--border, rgba(255,255,255,.08)); padding:8px 11px; text-align:left; vertical-align:top; }
.plan-doc-view th:last-child, .plan-doc-view td:last-child { border-right:none; }
.plan-doc-view tbody tr:last-child td { border-bottom:none; }
.plan-doc-view th { background:rgba(255,255,255,.08); color:var(--text-primary, #f0f2f5); font-weight:650; }
.plan-doc-view tbody tr:nth-child(odd) { background:rgba(255,255,255,.025); }
.plan-doc-view tbody tr:hover { background:rgba(79,140,255,.09); }
.plan-doc-view td code { white-space:nowrap; }
.plan-doc-toggle { margin-left:auto; display:flex; align-items:center; gap:2px; padding:2px; border:1px solid var(--border, rgba(255,255,255,.16)); border-radius:999px; background:rgba(255,255,255,.035); }
.plan-doc-toggle button { width:28px; height:24px; display:flex; align-items:center; justify-content:center; background:transparent; border:none; border-radius:999px; color:var(--text-muted, #8a8f98); cursor:pointer; padding:0; }
.plan-doc-toggle button:hover { color:var(--text, #fff); background:rgba(255,255,255,.07); }
.plan-doc-toggle button[data-active="1"] { background:var(--accent, #4f8cff); color:var(--accent-foreground,#fff); }
.plan-doc-toggle svg { width:14px; height:14px; }
`;
    document.head.appendChild(st);
  }

  // opts: { projectContext: { client_company, scope, contacts, path }, planPath? }
  async function createPlanPane(tabId, container, opts) {
    opts = opts || {};
    injectStyles();
    const pc = opts.projectContext || {};
    const planPath = opts.planPath || (pc.path ? pc.path.replace(/\/+$/, '') + '/PLAN.md' : null);

    const row = document.createElement('div');
    row.style.cssText = 'display:flex; flex:1 1 0%; width:100%; height:100%; min-width:0; min-height:0; overflow:hidden;';
    const left = document.createElement('div');
    left.style.cssText = 'display:flex; flex:1 1 0%; min-width:0; min-height:0; overflow:hidden;';
    const divider = document.createElement('div');
    divider.style.cssText = 'width:4px; cursor:col-resize; background:var(--border); flex-shrink:0;';
    const right = document.createElement('div');
    right.style.cssText = 'display:flex; flex-direction:column; flex:1 1 0%; min-width:0; min-height:0; overflow:hidden; background:var(--editor-surface, #1b1d23);';

    const bar = document.createElement('div');
    bar.style.cssText = 'display:flex; align-items:center; gap:8px; padding:8px 12px; border-bottom:1px solid var(--border); font-size:12px; color:var(--text-muted, #8a8f98); flex-shrink:0;';
    const title = document.createElement('span');
    title.textContent = planPath ? planPath.split('/').pop() : 'PLAN.md';
    const toggle = document.createElement('div');
    toggle.className = 'plan-doc-toggle';
    toggle.innerHTML = MODE_TOGGLE_HTML;
    const status = document.createElement('span');
    status.style.cssText = 'font-size:11px; opacity:.7; min-width:48px; text-align:right;';
    bar.appendChild(title);
    bar.appendChild(toggle);
    bar.appendChild(status);

    const view = document.createElement('div');
    view.className = 'plan-doc-view';
    const ta = document.createElement('textarea');
    ta.className = 'plan-doc-edit';
    ta.spellcheck = false;
    ta.placeholder = 'The plan the agent writes will appear here…';
    ta.style.cssText = 'flex:1 1 0%; width:100%; min-height:0; box-sizing:border-box; resize:none; border:none; outline:none; padding:14px 16px; background:transparent; color:var(--text, #d7dae0); font-family:"SF Mono",Menlo,"JetBrains Mono",monospace; font-size:13px; line-height:1.55; display:none;';

    right.appendChild(bar);
    right.appendChild(view);
    right.appendChild(ta);
    row.appendChild(left);
    row.appendChild(divider);
    row.appendChild(right);
    container.appendChild(row);

    let mode = 'preview';
    const renderView = () => {
      window.xnautMarkdown.renderInto(view, ta.value || '_No plan yet — ask the agent to draft one._');
    };
    const setMode = (m) => {
      mode = m;
      if (m === 'preview') { renderView(); view.style.display = 'block'; ta.style.display = 'none'; }
      else { view.style.display = 'none'; ta.style.display = 'block'; ta.focus(); }
      toggle.querySelectorAll('button').forEach((b) => { b.dataset.active = b.dataset.mode === m ? '1' : '0'; });
    };
    toggle.querySelectorAll('button').forEach((b) => { b.onclick = () => setMode(b.dataset.mode); });

    divider.addEventListener('mousedown', (e) => {
      e.preventDefault();
      const startX = e.clientX;
      const leftW = left.getBoundingClientRect().width;
      const total = row.getBoundingClientRect().width;
      const onMove = (ev) => {
        const w = Math.max(240, Math.min(total - 240, leftW + (ev.clientX - startX)));
        left.style.flex = `0 0 ${w}px`;
        right.style.flex = '1 1 0%';
      };
      const onUp = () => { document.removeEventListener('mousemove', onMove); document.removeEventListener('mouseup', onUp); };
      document.addEventListener('mousemove', onMove);
      document.addEventListener('mouseup', onUp);
    });

    const persist = async (text) => {
      if (!planPath) return;
      try { await invoke('write_file', { path: planPath, content: text }); status.textContent = 'saved'; }
      catch (e) { status.textContent = 'save failed'; console.error('[plan-pane] save failed', e); }
    };

    if (planPath) {
      try {
        const body = await invoke('read_file', { path: planPath });
        ta.value = typeof body === 'string' ? body : (body && body.content) || '';
      } catch (_) { /* no plan yet */ }
    }

    let saveTimer = null;
    ta.addEventListener('input', () => {
      status.textContent = 'editing…';
      clearTimeout(saveTimer);
      saveTimer = setTimeout(() => persist(ta.value), 600);
    });

    // Agent revision: replace doc, persist, and refresh the preview if showing.
    const onPlanDoc = async (markdown) => {
      ta.value = markdown;
      if (mode === 'preview') renderView();
      await persist(markdown);
    };

    setMode('preview');

    const chat = await window.xnautCreateChatPane(tabId, left, {
      projectContext: pc,
      planMode: { onPlanDoc, getDoc: () => ta.value },
    });

    return { kind: 'plan', pane: row, tabId, chat, planPath, getDoc: () => ta.value };
  }

  window.xnautCreatePlanPane = createPlanPane;
})();
