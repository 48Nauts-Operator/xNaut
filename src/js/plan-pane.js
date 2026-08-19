// Plan Mode pane — a two-pane planning workspace: chat (left) + a living
// markdown plan document (right). The chat runs in "plan mode" (solution-
// architect persona, Engram-grounded); whenever the agent emits the full plan
// as a ```markdown block, it is written to <project>/PLAN.md and shown in the
// right-hand doc pane.
//
// The doc pane is dependency-free (no CDN TipTap, which fails under this
// WebKit). It has three modes: Review (rendered markdown you can annotate),
// Preview (rendered, no annotation chrome) and Edit (raw textarea). Edits
// autosave to PLAN.md; the agent overwrites it on revision.
//
// Plan Canvas (XNAUT-192): in Review mode a click on any block of the plan
// attaches a numbered note to the lines that block came from, and the bar at
// the bottom answers a waiting agent with Approve or Request changes.
//
// Idea from ECC (github.com/affaan-m/ecc, MIT), `docs/design/plan-canvas.md`,
// which credits lavish-axi by @kunchenguid. Where we depart: ECC stands up a
// loopback web server with a DNS-rebinding guard, ships its own markdown
// renderer and pins a Mermaid CDN, all so a CLI can borrow a browser. xNAUT is
// the browser, so none of that is here. The notes are notes.rs annotations
// (the same range anchors the diff pane uses) and the verdict rides the inbox
// the veto's ask tier already blocks on.
//
// Registered as a panel factory: xnautAttachPanelTab → window.xnautCreatePlanPane.
(function () {
  'use strict';

  const invoke = (...a) => window.__TAURI__.core.invoke(...a);
  const MODE_TOGGLE_HTML = `
    <button data-mode="review" data-active="1" title="Review" aria-label="Review">
      <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M2.5 3.5h11v7h-6l-3 2.5v-2.5h-2z"/></svg>
    </button>
    <button data-mode="preview" title="Preview" aria-label="Preview">
      <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M1.8 8s2.3-4 6.2-4 6.2 4 6.2 4-2.3 4-6.2 4-6.2-4-6.2-4z"/><circle cx="8" cy="8" r="2"/></svg>
    </button>
    <button data-mode="edit" title="Edit" aria-label="Edit">
      <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M3 13l1-3 6.8-6.8a1.4 1.4 0 0 1 2 2L6 12z"/><path d="M9.8 4.2l2 2"/></svg>
    </button>`;

  // ── Anchors ────────────────────────────────────────────────────────────
  // Split the plan into the blocks a reviewer actually points at: a heading, a
  // paragraph, a list, a table, a fenced code block. Blank lines separate. A
  // fence swallows everything up to its closer, because a note stuck to half a
  // mermaid diagram anchors to lines that mean nothing on their own.
  //
  // Line numbers are 1-indexed and inclusive, which is what notes.rs
  // oldRange/newRange already are, so a plan note and a diff note are the same
  // record and the diff pane's anchor model needed no change at all.
  function splitBlocks(src) {
    const lines = String(src == null ? '' : src).replace(/\r\n/g, '\n').split('\n');
    const isFence = (l) => /^\s*```/.test(l);
    const blocks = [];
    let i = 0;
    while (i < lines.length) {
      if (lines[i].trim() === '') { i++; continue; }
      const start = i;
      if (isFence(lines[i])) {
        i++;
        while (i < lines.length && !isFence(lines[i])) i++;
        if (i < lines.length) i++;
      } else {
        while (i < lines.length && lines[i].trim() !== '' && !isFence(lines[i])) i++;
      }
      blocks.push({ start: start + 1, end: i, text: lines.slice(start, i).join('\n') });
    }
    return blocks;
  }

  // Number the notes down the plan, not by the order they were clicked.
  // plan_review.rs::collect_notes sorts the same way on the way out: "see note
  // 2" has to mean the same note on both sides of the handover.
  function numberNotes(annotations) {
    return (annotations || [])
      .filter((a) => a && String(a.summary || '').trim())
      .map((a) => Object.assign({}, a, { range: a.newRange || a.oldRange || [0, 0] }))
      .sort((a, b) => (a.range[0] - b.range[0]) || (a.range[1] - b.range[1]))
      .map((a, idx) => Object.assign(a, { n: idx + 1 }));
  }

  function notesForBlock(notes, block) {
    return (notes || []).filter((nt) => nt.range[0] <= block.end && nt.range[1] >= block.start);
  }

  // A note whose anchor no longer lands on any block (the plan was edited under
  // it) must still be visible: it is still going to the agent.
  function driftedNotes(notes, blocks) {
    return (notes || []).filter((nt) => !blocks.some((b) => nt.range[0] <= b.end && nt.range[1] >= b.start));
  }

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
.plan-block { position:relative; border-radius:6px; padding:2px 8px; margin:0 -8px; }
.plan-doc-view[data-review="1"] .plan-block { cursor:text; }
.plan-doc-view[data-review="1"] .plan-block:hover { background:rgba(79,140,255,.07); box-shadow:inset 2px 0 0 var(--accent, #4f8cff); }
.plan-block[data-noted="1"] { box-shadow:inset 2px 0 0 var(--agent-thinking, #4dffd0); }
.plan-notes { display:flex; flex-direction:column; gap:4px; margin:0 0 12px; }
.plan-note { display:flex; align-items:flex-start; gap:8px; padding:6px 9px; border-radius:6px; border:1px solid var(--border, rgba(255,255,255,.12)); background:rgba(77,255,208,.06); font-size:12.5px; line-height:1.5; }
.plan-note-n { flex:0 0 auto; min-width:18px; height:18px; display:inline-flex; align-items:center; justify-content:center; border-radius:999px; background:var(--agent-thinking, #4dffd0); color:#10131a; font-size:11px; font-weight:700; }
.plan-note-text { flex:1 1 auto; white-space:pre-wrap; }
.plan-note-del { flex:0 0 auto; background:transparent; border:none; color:var(--text-muted, #8a8f98); cursor:pointer; padding:0 2px; font-size:12px; line-height:1; }
.plan-note-del:hover { color:var(--danger, #ef4444); }
.plan-composer { display:flex; flex-direction:column; gap:6px; margin:6px 0 12px; }
.plan-composer-input { width:100%; box-sizing:border-box; resize:vertical; padding:7px 9px; border-radius:6px; border:1px solid var(--accent, #4f8cff); background:rgba(255,255,255,.04); color:var(--text, #d7dae0); font-family:inherit; font-size:13px; }
.plan-composer-actions { display:flex; gap:6px; justify-content:flex-end; }
.plan-drift { margin-top:24px; padding-top:12px; border-top:1px dashed var(--border, rgba(255,255,255,.16)); }
.plan-drift-label { font-size:11px; text-transform:uppercase; letter-spacing:.06em; color:var(--text-muted, #8a8f98); margin-bottom:6px; }
/* display:flex beats [hidden] unless it is said out loud (the modal gotcha). */
.plan-review-bar[hidden] { display:none; }
.plan-review-bar { display:flex; align-items:center; gap:10px; padding:9px 14px; border-top:1px solid var(--border, rgba(255,255,255,.12)); background:rgba(255,255,255,.03); font-size:12.5px; color:var(--text-muted, #9aa0aa); flex-shrink:0; }
.plan-review-msg { flex:1 1 auto; }
`;
    document.head.appendChild(st);
  }

  // Live panes, so a `plan-review` event lands in the pane already showing that
  // plan instead of stacking a second tab on top of it.
  const livePanes = new Set();

  // opts: { projectContext: { client_company, scope, contacts, path }, planPath?, reviewId? }
  async function createPlanPane(tabId, container, opts) {
    opts = opts || {};
    injectStyles();
    const pc = opts.projectContext || {};
    const root = pc.path ? pc.path.replace(/\/+$/, '') : null;
    const planPath = opts.planPath || (root ? root + '/PLAN.md' : null);
    // notes.rs keys annotations by a path relative to the worktree, the same
    // way the diff pane keys them by the file in the diff.
    const noteFile = (planPath && root && planPath.indexOf(root + '/') === 0)
      ? planPath.slice(root.length + 1)
      : 'PLAN.md';

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

    const reviewBar = document.createElement('div');
    reviewBar.className = 'plan-review-bar';
    reviewBar.hidden = true;
    reviewBar.innerHTML = `
      <span class="plan-review-msg"></span>
      <button class="btn btn-sm plan-review-changes">Request changes</button>
      <button class="btn btn-sm btn-primary plan-review-approve">Approve plan</button>`;
    const reviewMsg = reviewBar.querySelector('.plan-review-msg');
    const btnChanges = reviewBar.querySelector('.plan-review-changes');
    const btnApprove = reviewBar.querySelector('.plan-review-approve');

    right.appendChild(bar);
    right.appendChild(view);
    right.appendChild(ta);
    right.appendChild(reviewBar);
    row.appendChild(left);
    row.appendChild(divider);
    row.appendChild(right);
    container.appendChild(row);

    let mode = 'review';
    let notes = [];

    async function loadNotes() {
      if (!root) { notes = []; return; }
      try {
        const doc = await invoke('notes_read', { worktree: root });
        const entry = ((doc && doc.files) || []).filter((f) => f.path === noteFile)[0];
        notes = numberNotes(entry ? entry.annotations : []);
      } catch (_) { notes = []; }
    }

    function noteRow(nt) {
      const el = document.createElement('div');
      el.className = 'plan-note';
      el.dataset.noteId = nt.id || '';
      const num = document.createElement('span');
      num.className = 'plan-note-n';
      num.textContent = String(nt.n);
      const text = document.createElement('span');
      text.className = 'plan-note-text';
      text.textContent = nt.summary;
      const del = document.createElement('button');
      del.className = 'plan-note-del';
      del.type = 'button';
      del.textContent = '✕';
      del.setAttribute('aria-label', `Remove note ${nt.n}`);
      del.onclick = async (e) => {
        e.stopPropagation();
        if (!nt.id || !root) return;
        try { await invoke('notes_remove', { worktree: root, noteId: nt.id }); } catch (err) { console.error('[plan-pane] remove note', err); }
        await loadNotes();
        renderView();
      };
      el.appendChild(num);
      el.appendChild(text);
      el.appendChild(del);
      return el;
    }

    function openComposer(blockEl, block) {
      if (blockEl.querySelector('.plan-composer')) return;
      const box = document.createElement('div');
      box.className = 'plan-composer';
      const input = document.createElement('textarea');
      input.className = 'plan-composer-input';
      input.rows = 2;
      input.placeholder = 'What should change here?';
      input.setAttribute('aria-label', `Note on lines ${block.start} to ${block.end}`);
      const actions = document.createElement('div');
      actions.className = 'plan-composer-actions';
      const cancel = document.createElement('button');
      cancel.className = 'btn btn-sm plan-composer-cancel';
      cancel.type = 'button';
      cancel.textContent = 'Cancel';
      const save = document.createElement('button');
      save.className = 'btn btn-sm btn-primary plan-composer-save';
      save.type = 'button';
      save.textContent = 'Add note';
      actions.appendChild(cancel);
      actions.appendChild(save);
      box.appendChild(input);
      box.appendChild(actions);
      blockEl.appendChild(box);
      input.focus();

      cancel.onclick = (e) => { e.stopPropagation(); box.remove(); };
      save.onclick = async (e) => {
        e.stopPropagation();
        const text = input.value.trim();
        if (!text || !root) { box.remove(); return; }
        save.disabled = true;
        try {
          await invoke('notes_add', {
            worktree: root,
            filePath: noteFile,
            note: {
              newRange: [block.start, block.end],
              summary: text,
              source: 'user',
              createdAt: new Date().toISOString(),
              editable: true,
            },
          });
        } catch (err) {
          console.error('[plan-pane] add note', err);
          status.textContent = 'note failed';
          save.disabled = false;
          return;
        }
        await loadNotes();
        renderView();
      };
      input.addEventListener('keydown', (ev) => {
        if (ev.key === 'Escape') { ev.stopPropagation(); box.remove(); }
        if (ev.key === 'Enter' && (ev.metaKey || ev.ctrlKey)) { ev.preventDefault(); save.onclick(ev); }
      });
    }

    async function renderView() {
      const src = ta.value || '';
      const reviewing = mode === 'review';
      view.dataset.review = reviewing ? '1' : '0';
      view.innerHTML = '';
      if (!src.trim()) {
        await window.xnautMarkdown.renderInto(view, '_No plan yet — ask the agent to draft one._');
        return;
      }
      const blocks = splitBlocks(src);
      const els = blocks.map((block) => {
        const el = document.createElement('div');
        el.className = 'plan-block';
        el.dataset.start = String(block.start);
        el.dataset.end = String(block.end);
        const body = document.createElement('div');
        el.appendChild(body);
        const mine = reviewing ? notesForBlock(notes, block) : [];
        if (mine.length) {
          el.dataset.noted = '1';
          const strip = document.createElement('div');
          strip.className = 'plan-notes';
          mine.forEach((nt) => strip.appendChild(noteRow(nt)));
          el.appendChild(strip);
        }
        if (reviewing) {
          el.addEventListener('click', (ev) => {
            if (ev.target.closest('.plan-note, .plan-composer')) return;
            openComposer(el, block);
          });
        }
        view.appendChild(el);
        return { body, block };
      });
      if (reviewing) {
        const drifted = driftedNotes(notes, blocks);
        if (drifted.length) {
          const box = document.createElement('div');
          box.className = 'plan-drift';
          const label = document.createElement('div');
          label.className = 'plan-drift-label';
          label.textContent = 'Notes whose lines the plan no longer has';
          const strip = document.createElement('div');
          strip.className = 'plan-notes';
          drifted.forEach((nt) => strip.appendChild(noteRow(nt)));
          box.appendChild(label);
          box.appendChild(strip);
          view.appendChild(box);
        }
      }
      // A re-render replaces the whole view, so a stale renderInto lands in a
      // detached node and nothing else has to guard against it.
      await Promise.all(els.map((e) => window.xnautMarkdown.renderInto(e.body, e.block.text)));
    }

    const setMode = (m) => {
      mode = m;
      if (m === 'edit') { view.style.display = 'none'; ta.style.display = 'block'; ta.focus(); }
      else { renderView(); view.style.display = 'block'; ta.style.display = 'none'; }
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

    // ── The verdict ────────────────────────────────────────────────────────
    let reviewId = null;

    function showReview(msg) {
      reviewBar.hidden = false;
      reviewMsg.textContent = msg;
      const waiting = !!reviewId;
      btnApprove.hidden = !waiting;
      btnChanges.hidden = !waiting;
    }

    async function decide(decision) {
      if (!reviewId) return;
      const id = reviewId;
      btnApprove.disabled = true;
      btnChanges.disabled = true;
      try {
        // "denied" is the inbox's only word for a no; plan_review.rs turns it
        // into changes_requested, because a plan sent back with notes is not
        // rejected work.
        await invoke('inbox_decide', { id, decision });
      } catch (e) {
        console.error('[plan-pane] decide', e);
        btnApprove.disabled = false;
        btnChanges.disabled = false;
        showReview('Could not answer: ' + String(e));
        return;
      }
      reviewId = null;
      btnApprove.disabled = false;
      btnChanges.disabled = false;
      const count = notes.length;
      showReview(decision === 'approved'
        ? 'Approved. The agent is unblocked.'
        : `Sent back with ${count} note${count === 1 ? '' : 's'}.`);
    }

    btnApprove.onclick = () => decide('approved');
    btnChanges.onclick = () => decide('denied');

    async function adopt(id) {
      reviewId = id || null;
      if (planPath) {
        try {
          const body = await invoke('read_file', { path: planPath });
          ta.value = typeof body === 'string' ? body : (body && body.content) || '';
        } catch (_) { /* keep what is on screen */ }
      }
      await loadNotes();
      if (mode !== 'edit') renderView();
      if (reviewId) showReview('An agent is waiting on this plan.');
    }

    // An agent may already be blocked when the tab is opened by hand, so the
    // buttons cannot depend on catching the event.
    async function findPendingReview() {
      if (!root) return null;
      try {
        const items = await invoke('inbox_list', { project: null, status: 'open' });
        const hit = (items || []).filter((it) => it.context
          && it.context.plan_project === root
          && it.context.plan_file === noteFile)[0];
        return hit ? hit.id : null;
      } catch (_) { return null; }
    }

    if (planPath) {
      try {
        const body = await invoke('read_file', { path: planPath });
        ta.value = typeof body === 'string' ? body : (body && body.content) || '';
      } catch (_) { /* no plan yet */ }
    }
    await loadNotes();

    let saveTimer = null;
    ta.addEventListener('input', () => {
      status.textContent = 'editing…';
      clearTimeout(saveTimer);
      saveTimer = setTimeout(() => persist(ta.value), 600);
    });

    // Agent revision: replace doc, persist, and refresh the preview if showing.
    const onPlanDoc = async (markdown) => {
      ta.value = markdown;
      if (mode !== 'edit') renderView();
      await persist(markdown);
    };

    setMode('review');

    const entry = { planPath, adopt };
    livePanes.add(entry);

    if (opts.reviewId) adopt(opts.reviewId);
    else findPendingReview().then((id) => { if (id) adopt(id); });

    const chat = await window.xnautCreateChatPane(tabId, left, {
      projectContext: pc,
      planMode: { onPlanDoc, getDoc: () => ta.value },
    });

    return {
      kind: 'plan',
      pane: row,
      tabId,
      chat,
      planPath,
      getDoc: () => ta.value,
      adopt,
      destroy: () => { livePanes.delete(entry); },
    };
  }

  // The pane, not the Mesh list, is where a plan gets read: plan_review.rs
  // emits this the moment an agent hands one over.
  if (typeof window !== 'undefined' && window.__TAURI__ && window.__TAURI__.event) {
    window.__TAURI__.event.listen('plan-review', (event) => {
      const p = (event && event.payload) || {};
      const open = Array.from(livePanes).filter((pane) => pane.planPath === p.planPath)[0];
      if (open) { open.adopt(p.id); return; }
      if (window.xnautAttachPlanTab) {
        window.xnautAttachPlanTab({
          projectContext: { path: p.project },
          planPath: p.planPath,
          reviewId: p.id,
        });
      }
    });
  }

  window.xnautCreatePlanPane = createPlanPane;
  // Lifted by scripts/plan-canvas-smoke.cjs so the anchor rules are tested as
  // shipped rather than as a copy that can drift.
  window.xnautPlanCanvas = { splitBlocks, numberNotes, notesForBlock, driftedNotes };
})();
