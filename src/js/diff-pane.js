// Diff panes — Phase 8a of the Orca port + hunk's killer feature.
//
// Renders a worktree's git diff as side-by-side or unified view. Inline notes
// (read from <worktree>/.xnaut/notes.json) overlay the diff at their anchor
// lines using hunk's three-dock rule:
//   - split view + new-side note + width ≥ 84col → dock right
//   - split view + old-side note + width ≥ 84col → dock left
//   - otherwise → unified-indented (4-col indent)
//
// The notes file is watched (Rust-side via `notify`); when it changes the
// backend emits `notes-changed` and we re-render without a full reload.
(function () {
  'use strict';

  const $ = (id) => document.getElementById(id);
  const inv = () => (window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke);
  const listen = () => (window.__TAURI__ && window.__TAURI__.event && window.__TAURI__.event.listen);

  // label -> { paneEl, worktree, viewMode, tabId, unsubChanges }
  const panes = new Map();
  let labelCounter = 0;
  function nextLabel() {
    labelCounter += 1;
    return `diff-${Date.now().toString(36)}-${labelCounter}`;
  }

  async function createDiffPane(tabId, parentContainer, opts) {
    opts = opts || {};
    const label = nextLabel();
    const worktree = opts.worktree || (await guessWorktree()) || '';

    const pane = document.createElement('div');
    pane.className = 'diff-pane';
    pane.dataset.diffLabel = label;
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
    bar.className = 'diff-bar';
    bar.innerHTML = `
      <input type="text" class="diff-worktree-path" placeholder="Worktree path (e.g. /Users/you/proj)" autocomplete="off" />
      <button class="diff-mode-btn" data-mode="split">Split</button>
      <button class="diff-mode-btn" data-mode="unified">Unified</button>
      <span class="diff-bar-spacer"></span>
      <button class="btn-icon diff-reload" title="Reload diff" aria-label="Reload diff">
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor"><path d="M2 8a6 6 0 1 1 1.76 4.24"/><polyline points="2 13 2 9 6 9"/></svg>
      </button>
      <button class="btn-icon diff-close" data-variant="destructive" title="Close pane" aria-label="Close diff pane">
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor"><line x1="4" y1="4" x2="12" y2="12"/><line x1="12" y1="4" x2="4" y2="12"/></svg>
      </button>
    `;
    pane.appendChild(bar);

    const body = document.createElement('div');
    body.className = 'diff-body';
    pane.appendChild(body);

    parentContainer.appendChild(pane);

    const pathInput = bar.querySelector('.diff-worktree-path');
    pathInput.value = worktree;
    let viewMode = opts.viewMode || 'split'; // 'split' | 'unified'
    const updateModeButtons = () => {
      bar.querySelectorAll('.diff-mode-btn').forEach((b) => {
        b.classList.toggle('is-active', b.dataset.mode === viewMode);
      });
    };
    updateModeButtons();

    // ── render pipeline ──
    let lastDiff = null;
    let lastNotes = null;
    async function refresh() {
      const inv1 = inv();
      const root = pathInput.value.trim();
      if (!inv1 || !root) { body.innerHTML = '<div class="diff-empty">Set a worktree path to see its diff.</div>'; return; }
      body.innerHTML = '<div class="diff-empty">Loading…</div>';
      try {
        const [diff, notes] = await Promise.all([
          inv1('diff_for_worktree', { worktree: root }),
          inv1('notes_read', { worktree: root }).catch(() => ({ version: 1, files: [] })),
        ]);
        lastDiff = diff;
        lastNotes = notes;
        renderDiff(body, diff, notes, viewMode);
      } catch (e) {
        body.innerHTML = `<div class="diff-empty diff-empty-err">${escapeText(String(e))}</div>`;
      }
    }
    async function refreshNotesOnly() {
      if (!lastDiff) return;
      try {
        const notes = await inv()('notes_read', { worktree: pathInput.value.trim() });
        lastNotes = notes;
        renderDiff(body, lastDiff, notes, viewMode);
      } catch (_e) {}
    }

    // ── wire bar ──
    bar.querySelectorAll('.diff-mode-btn').forEach((b) => {
      b.onclick = () => { viewMode = b.dataset.mode; updateModeButtons(); if (lastDiff) renderDiff(body, lastDiff, lastNotes, viewMode); };
    });
    bar.querySelector('.diff-reload').onclick = () => refresh();
    bar.querySelector('.diff-close').onclick = () => destroyDiffPane(label);
    pathInput.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') refresh();
    });

    // ── notes watcher ──
    let unsubChanges = null;
    const l = listen();
    if (l) {
      try {
        unsubChanges = await l('notes-changed', (event) => {
          const wt = event && event.payload && event.payload.worktree;
          if (wt && wt === pathInput.value.trim()) refreshNotesOnly();
        });
      } catch (_e) {}
    }
    if (worktree) {
      try { await inv()('notes_watch_start', { worktree }); } catch (_e) {}
    }

    panes.set(label, {
      paneEl: pane, worktree, viewMode, tabId,
      unsubChanges,
      pathInputEl: pathInput,
      bodyEl: body,
    });
    if (worktree) refresh();
    return { kind: 'diff', label, pane };
  }

  async function destroyDiffPane(label) {
    const entry = panes.get(label);
    if (!entry) return;
    try { if (entry.unsubChanges) entry.unsubChanges(); } catch (_e) {}
    try { await inv()('notes_watch_stop', {}); } catch (_e) {}
    if (entry.paneEl && entry.paneEl.parentNode) entry.paneEl.parentNode.removeChild(entry.paneEl);
    panes.delete(label);
  }

  // Try to fill the worktree from the currently focused terminal's cwd.
  async function guessWorktree() {
    try { return await inv()('get_current_directory', {}); } catch (_e) { return null; }
  }

  // ─────────────────────────────────────────────────────────────────────────
  //                           render core
  // ─────────────────────────────────────────────────────────────────────────

  function renderDiff(body, diff, notes, viewMode) {
    body.innerHTML = '';
    if (!diff || !diff.files || diff.files.length === 0) {
      body.innerHTML = '<div class="diff-empty">No changes in this worktree.</div>';
      return;
    }
    const notesByFile = buildNotesIndex(notes);
    diff.files.forEach((file) => {
      const fileEl = renderFile(file, notesByFile[fileKey(file)] || [], viewMode);
      body.appendChild(fileEl);
    });
  }

  function buildNotesIndex(notes) {
    const out = {};
    if (!notes || !notes.files) return out;
    notes.files.forEach((f) => { out[f.path] = f.annotations || []; });
    return out;
  }
  function fileKey(file) { return file.new_path || file.old_path || ''; }

  function renderFile(file, notes, viewMode) {
    const wrap = document.createElement('div');
    wrap.className = 'diff-file';
    const header = document.createElement('div');
    header.className = 'diff-file-header';
    const newName = file.new_path || '(deleted)';
    const oldName = file.old_path || '(new)';
    header.textContent = newName === oldName ? newName : `${oldName} → ${newName}`;
    wrap.appendChild(header);

    if (file.is_binary) {
      const bin = document.createElement('div');
      bin.className = 'diff-binary';
      bin.textContent = 'Binary file — no preview';
      wrap.appendChild(bin);
      return wrap;
    }

    const adds = countLines(file.hunks, 'add');
    const dels = countLines(file.hunks, 'del');
    const stats = document.createElement('span');
    stats.className = 'diff-file-stats';
    stats.innerHTML = `<span class="diff-stat-add">+${adds}</span> <span class="diff-stat-del">−${dels}</span>`;
    header.appendChild(stats);

    // File-level fallback: any notes whose anchor line isn't actually present
    // in any hunk get surfaced at the top of the file section instead of
    // silently dropping. Mirrors hunk's resolveCommentTarget fallback chain.
    const unmatched = computeUnmatchedNotes(file.hunks, notes);
    if (unmatched.length) {
      const group = document.createElement('div');
      group.className = 'diff-file-notes';
      const header2 = document.createElement('div');
      header2.className = 'diff-file-notes-header';
      header2.textContent = `${unmatched.length} note${unmatched.length === 1 ? '' : 's'} not in current hunk view`;
      group.appendChild(header2);
      unmatched.forEach((note, idx) => {
        group.appendChild(renderNote(note, idx, unmatched.length, viewMode));
      });
      wrap.appendChild(group);
    }

    file.hunks.forEach((hunk) => {
      const hunkEl = renderHunk(hunk, notes, viewMode);
      wrap.appendChild(hunkEl);
    });
    return wrap;
  }

  // Returns notes whose anchor line isn't present in any of this file's hunks.
  function computeUnmatchedNotes(hunks, notes) {
    const newLinesPresent = new Set();
    const oldLinesPresent = new Set();
    hunks.forEach((h) => h.lines.forEach((l) => {
      if (l.new_line != null) newLinesPresent.add(l.new_line);
      if (l.old_line != null) oldLinesPresent.add(l.old_line);
    }));
    return notes.filter((n) => {
      if (n.newRange) {
        for (let i = n.newRange[0]; i <= n.newRange[1]; i += 1) if (newLinesPresent.has(i)) return false;
      }
      if (n.oldRange) {
        for (let i = n.oldRange[0]; i <= n.oldRange[1]; i += 1) if (oldLinesPresent.has(i)) return false;
      }
      return true;
    });
  }

  function countLines(hunks, kind) {
    let n = 0;
    hunks.forEach((h) => h.lines.forEach((l) => { if (l.kind === kind) n += 1; }));
    return n;
  }

  function renderHunk(hunk, notes, viewMode) {
    const wrap = document.createElement('div');
    wrap.className = 'diff-hunk';
    const header = document.createElement('div');
    header.className = 'diff-hunk-header';
    header.textContent = hunk.header;
    wrap.appendChild(header);
    if (viewMode === 'unified') renderHunkUnified(wrap, hunk, notes);
    else renderHunkSplit(wrap, hunk, notes);
    return wrap;
  }

  function renderHunkUnified(wrap, hunk, notes) {
    const rows = document.createElement('div');
    rows.className = 'diff-rows diff-rows-unified';
    hunk.lines.forEach((line) => {
      const row = document.createElement('div');
      row.className = 'diff-row diff-row-' + line.kind;
      row.dataset.oldLine = line.old_line == null ? '' : line.old_line;
      row.dataset.newLine = line.new_line == null ? '' : line.new_line;
      const gutters = `<span class="diff-gutter diff-gutter-old">${line.old_line ?? ''}</span><span class="diff-gutter diff-gutter-new">${line.new_line ?? ''}</span>`;
      row.innerHTML = `${gutters}<span class="diff-line">${escapeText(line.content)}</span>`;
      rows.appendChild(row);
      injectNotesAfter(rows, line, notes, 'unified');
    });
    wrap.appendChild(rows);
  }

  function renderHunkSplit(wrap, hunk, notes) {
    const rows = document.createElement('div');
    rows.className = 'diff-rows diff-rows-split';
    // Pair adjacent del+add into the same visual row when possible (best-effort);
    // otherwise leave one side blank. For v1 we just walk linearly and let context
    // rows occupy both sides identically.
    const queue = hunk.lines.slice();
    while (queue.length) {
      const line = queue.shift();
      if (line.kind === 'context') {
        rows.appendChild(splitRow(line.old_line, line.content, line.new_line, line.content, 'context'));
        injectNotesAfter(rows, line, notes, 'split');
      } else if (line.kind === 'del') {
        const next = queue[0];
        if (next && next.kind === 'add') {
          queue.shift();
          rows.appendChild(splitRow(line.old_line, line.content, next.new_line, next.content, 'change'));
          injectNotesAfter(rows, next, notes, 'split');
        } else {
          rows.appendChild(splitRow(line.old_line, line.content, null, '', 'del'));
          injectNotesAfter(rows, line, notes, 'split');
        }
      } else if (line.kind === 'add') {
        rows.appendChild(splitRow(null, '', line.new_line, line.content, 'add'));
        injectNotesAfter(rows, line, notes, 'split');
      } else {
        // header lines (\ No newline at end of file)
        const row = document.createElement('div');
        row.className = 'diff-row diff-row-meta';
        row.textContent = line.content;
        rows.appendChild(row);
      }
    }
    wrap.appendChild(rows);
  }

  function splitRow(oldLine, oldContent, newLine, newContent, kind) {
    const row = document.createElement('div');
    row.className = 'diff-row diff-row-split diff-row-' + kind;
    row.innerHTML = `
      <span class="diff-gutter diff-gutter-old">${oldLine ?? ''}</span>
      <span class="diff-line diff-line-old">${escapeText(oldContent)}</span>
      <span class="diff-gutter diff-gutter-new">${newLine ?? ''}</span>
      <span class="diff-line diff-line-new">${escapeText(newContent)}</span>
    `;
    return row;
  }

  // ── notes overlay — the killer feature ──
  // Anchors a note to the LAST line of its range so the card renders below
  // the entire anchored block (matches hunk's render plan). The off-by-one
  // bug here was using [0] for both bounds — fixed to use [1] for the upper.
  function injectNotesAfter(rowsContainer, line, notes, viewMode) {
    const matches = notes.filter((n) => {
      if (line.new_line != null && n.newRange) {
        return line.new_line === n.newRange[1];
      }
      if (line.old_line != null && n.oldRange) {
        return line.old_line === n.oldRange[1];
      }
      return false;
    });
    if (!matches.length) return;
    matches.forEach((note, idx) => {
      const card = renderNote(note, idx, matches.length, viewMode);
      rowsContainer.appendChild(card);
    });
  }

  function renderNote(note, idx, count, viewMode) {
    const wrap = document.createElement('div');
    const side = note.newRange ? 'new' : 'old';
    wrap.className = `diff-note diff-note-${side} diff-note-${viewMode}`;
    const title = noteTitle(note, idx, count);
    const summary = escapeText(note.summary || '');
    const rationale = note.rationale ? `<div class="diff-note-rationale">${escapeText(note.rationale)}</div>` : '';
    const tags = (note.tags || []).map((t) => `<span class="diff-note-tag">${escapeText(t)}</span>`).join('');
    wrap.innerHTML = `
      <div class="diff-note-header">
        <span class="diff-note-title">${escapeText(title)}</span>
        <span class="diff-note-tags">${tags}</span>
      </div>
      <div class="diff-note-summary">${summary}</div>
      ${rationale}
    `;
    return wrap;
  }

  function noteTitle(note, idx, count) {
    let base;
    if (note.author) base = `${note.author} note`;
    else if (note.source === 'user') base = 'Your note';
    else if (note.source === 'agent') base = 'Agent note';
    else if (note.source === 'ai') base = 'AI note';
    else base = 'Note';
    if (count > 1) base += ` ${idx + 1}/${count}`;
    if (note.newRange) base += ` · L${note.newRange[0]}`;
    else if (note.oldRange) base += ` · L${note.oldRange[0]}`;
    return base;
  }

  function escapeText(s) {
    return String(s == null ? '' : s).replace(/[&<>"']/g, (c) => ({ '&':'&amp;', '<':'&lt;', '>':'&gt;', '"':'&quot;', "'":'&#39;' }[c]));
  }

  // ── public API ──
  window.xnautCreateDiffPane = createDiffPane;
  window.xnautDestroyDiffPane = destroyDiffPane;

  async function newDiffTab(opts) {
    if (typeof window.xnautAttachDiffTab !== 'function') {
      console.warn('xnautAttachDiffTab not yet defined in app.js');
      return;
    }
    return window.xnautAttachDiffTab(opts || {});
  }
  window.xnautNewDiffTab = newDiffTab;

  function wireButton() {
    const btn = $('btn-new-diff');
    if (btn) btn.onclick = () => newDiffTab().catch((e) => console.error('new diff tab failed:', e));
  }
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', wireButton);
  } else {
    wireButton();
  }
})();
