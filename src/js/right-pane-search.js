// Right pane — Search view. Project-wide text search via the search_text
// backend command, results grouped by file.
//
// Registers itself with the right-pane host shell; queue-safe if this file
// loads before right-pane.js.
(function () {
  'use strict';

  const invoke = (...a) => window.__TAURI__.core.invoke(...a);

  function escapeText(s) {
    return String(s).replace(/[&<>"']/g, (c) => ({ '&':'&amp;', '<':'&lt;', '>':'&gt;', '"':'&quot;', "'":'&#39;' }[c]));
  }

  const ICON_CHEVRON = '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" width="12" height="12"><path d="M6 4l4 4-4 4"/></svg>';

  const STYLES = `
.rps-view { display:flex; flex-direction:column; min-height:0; flex:1 1 0%; }
.rps-controls { display:flex; flex-direction:column; gap:6px; flex:0 0 auto; padding:8px 10px; border-bottom:1px solid var(--border); }
.rps-input-row { display:flex; align-items:center; gap:6px; }
.rps-query, .rps-glob { flex:1 1 auto; min-width:0; height:26px; padding:0 8px; font-size:12px; color:var(--text-primary); background:var(--bg-primary); border:1px solid var(--border); border-radius:var(--radius-md, 6px); outline:none; }
.rps-query:focus, .rps-glob:focus { border-color:var(--accent); }
.rps-glob { font-family:var(--font-mono, monospace); }
.rps-case { flex:0 0 auto; width:26px; height:26px; border:1px solid var(--border); border-radius:var(--radius-md, 6px); background:transparent; color:var(--text-secondary); font-size:11px; cursor:pointer; padding:0; }
.rps-case:hover { background:var(--bg-tertiary); }
.rps-case.rps-on { background:var(--bg-tertiary); color:var(--accent); border-color:var(--accent); }
.rps-status { display:flex; align-items:center; gap:6px; font-size:11px; color:var(--text-secondary); min-height:16px; flex-wrap:wrap; }
.rps-badge { font-size:10px; padding:1px 6px; border-radius:999px; background:var(--bg-tertiary); border:1px solid var(--border); }
.rps-badge-warn { color:var(--warning, orange); border-color:var(--warning, orange); }
.rps-results { flex:1 1 0%; min-height:0; overflow-y:auto; padding:4px 0; }
.rps-file { display:flex; align-items:center; gap:4px; height:24px; padding:0 8px; font-size:12px; color:var(--text-primary); cursor:pointer; white-space:nowrap; overflow:hidden; user-select:none; background:var(--bg-tertiary); }
.rps-file-chevron { flex:0 0 14px; display:flex; align-items:center; justify-content:center; color:var(--text-secondary); transition:transform var(--transition-fast, 0.1s); }
.rps-file-chevron.rps-open { transform:rotate(90deg); }
.rps-file-path { flex:1 1 auto; min-width:0; overflow:hidden; text-overflow:ellipsis; font-family:var(--font-mono, monospace); font-size:11px; direction:rtl; text-align:left; }
.rps-file-count { flex:0 0 auto; font-size:10px; color:var(--text-secondary); }
.rps-match { display:flex; align-items:baseline; gap:8px; padding:2px 8px 2px 26px; font-size:11px; font-family:var(--font-mono, monospace); cursor:pointer; white-space:nowrap; overflow:hidden; }
.rps-match:hover { background:var(--bg-tertiary); }
.rps-line-no { flex:0 0 auto; color:var(--text-secondary); min-width:28px; text-align:right; }
.rps-line-text { flex:1 1 auto; min-width:0; overflow:hidden; text-overflow:ellipsis; color:var(--text-primary); }
.rps-line-text mark { background:transparent; color:var(--accent); font-weight:600; }
.rps-msg { padding:14px 12px; font-size:12px; color:var(--text-secondary); text-align:center; }
.rps-msg-error { color:var(--danger, #e5534b); }
`;

  function ensureStyles() {
    if (document.getElementById('right-pane-search-styles')) return;
    const el = document.createElement('style');
    el.id = 'right-pane-search-styles';
    el.textContent = STYLES;
    document.head.appendChild(el);
  }

  // Escape `text`, wrapping every occurrence of `query` in <mark>.
  // Case-aware: respects the caseSensitive flag.
  function highlightLine(text, query, caseSensitive) {
    const t = String(text);
    if (!query) return escapeText(t);
    const hay = caseSensitive ? t : t.toLowerCase();
    const needle = caseSensitive ? query : query.toLowerCase();
    let out = '';
    let i = 0;
    while (true) {
      const idx = hay.indexOf(needle, i);
      if (idx === -1) { out += escapeText(t.slice(i)); break; }
      out += escapeText(t.slice(i, idx));
      out += `<mark>${escapeText(t.slice(idx, idx + needle.length))}</mark>`;
      i = idx + needle.length;
    }
    return out;
  }

  function openMatch(root, relPath) {
    const abs = relPath.startsWith('/')
      ? relPath
      : `${String(root || '').replace(/\/+$/, '')}/${relPath}`;
    if (/\.md$/i.test(abs)) {
      if (window.xnautOpenMarkdownFile) window.xnautOpenMarkdownFile(abs);
    } else {
      if (window.xnautOpenInEditor) window.xnautOpenInEditor(abs);
    }
  }

  function createSearchView() {
    let container = null;
    let root = null;
    let caseSensitive = false;
    let debounceTimer = null;
    let searchSeq = 0;
    let queryEl = null, globEl = null, caseBtn = null, statusEl = null, resultsEl = null;

    function setStatus(html) { if (statusEl) statusEl.innerHTML = html; }

    function renderResults(matches, query, meta) {
      resultsEl.innerHTML = '';
      if (!matches.length) {
        resultsEl.innerHTML = '<div class="rps-msg">No matches</div>';
        return;
      }
      // Group by file path, preserving backend order.
      const groups = new Map();
      for (const m of matches) {
        const key = String(m.path || '');
        if (!groups.has(key)) groups.set(key, []);
        groups.get(key).push(m);
      }
      let fileIndex = 0;
      for (const [path, items] of groups) {
        const expanded = fileIndex < 10; // first 10 files open by default
        fileIndex += 1;

        const header = document.createElement('div');
        header.className = 'rps-file';
        header.title = path;
        header.innerHTML = `
          <span class="rps-file-chevron${expanded ? ' rps-open' : ''}">${ICON_CHEVRON}</span>
          <span class="rps-file-path">${escapeText(path)}</span>
          <span class="rps-file-count">${items.length}</span>
        `;
        const body = document.createElement('div');
        body.style.display = expanded ? '' : 'none';
        for (const m of items) {
          const row = document.createElement('div');
          row.className = 'rps-match';
          row.title = `${path}:${m.line}`;
          row.innerHTML = `
            <span class="rps-line-no">${escapeText(String(m.line ?? ''))}</span>
            <span class="rps-line-text">${highlightLine(m.text || '', query, caseSensitive)}</span>
          `;
          row.onclick = () => openMatch(root, path);
          body.appendChild(row);
        }
        header.onclick = () => {
          const open = body.style.display === 'none';
          body.style.display = open ? '' : 'none';
          header.querySelector('.rps-file-chevron').classList.toggle('rps-open', open);
        };
        resultsEl.appendChild(header);
        resultsEl.appendChild(body);
      }
      const badges = [
        `<span>${matches.length} match${matches.length === 1 ? '' : 'es'} in ${groups.size} file${groups.size === 1 ? '' : 's'}</span>`,
        meta.backend ? `<span class="rps-badge">${escapeText(String(meta.backend))}</span>` : '',
        meta.truncated ? '<span class="rps-badge rps-badge-warn">truncated</span>' : '',
      ];
      setStatus(badges.join(''));
    }

    async function runSearch() {
      const query = queryEl ? queryEl.value : '';
      if (!query || query.trim().length < 2) {
        setStatus('');
        resultsEl.innerHTML = '<div class="rps-msg">Type at least 2 characters to search</div>';
        return;
      }
      if (!root) {
        resultsEl.innerHTML = '<div class="rps-msg">No project root set</div>';
        return;
      }
      const seq = ++searchSeq;
      setStatus('Searching…');
      const glob = globEl && globEl.value.trim() ? globEl.value.trim() : null;
      try {
        const result = await invoke('search_text', {
          root,
          query,
          opts: { case_sensitive: caseSensitive, glob, max_results: 500 },
        });
        if (seq !== searchSeq || !resultsEl) return; // stale response
        const matches = (result && Array.isArray(result.matches)) ? result.matches : [];
        renderResults(matches, query, { backend: result && result.backend, truncated: !!(result && result.truncated) });
      } catch (e) {
        if (seq !== searchSeq || !resultsEl) return;
        setStatus('');
        resultsEl.innerHTML = `<div class="rps-msg rps-msg-error">Search failed: ${escapeText(String(e))}</div>`;
      }
    }

    function scheduleSearch() {
      clearTimeout(debounceTimer);
      debounceTimer = setTimeout(runSearch, 300);
    }

    return {
      mount(el, initialRoot) {
        ensureStyles();
        container = el;
        root = initialRoot || null;
        container.innerHTML = `
          <div class="rps-view">
            <div class="rps-controls">
              <div class="rps-input-row">
                <input class="rps-query" type="text" placeholder="Search project…" spellcheck="false">
                <button class="rps-case" title="Match case" aria-label="Toggle case sensitivity">Aa</button>
              </div>
              <div class="rps-input-row">
                <input class="rps-glob" type="text" placeholder="*.rs" spellcheck="false" title="Glob filter">
              </div>
              <div class="rps-status"></div>
            </div>
            <div class="rps-results"><div class="rps-msg">Type at least 2 characters to search</div></div>
          </div>
        `;
        queryEl = container.querySelector('.rps-query');
        globEl = container.querySelector('.rps-glob');
        caseBtn = container.querySelector('.rps-case');
        statusEl = container.querySelector('.rps-status');
        resultsEl = container.querySelector('.rps-results');

        queryEl.addEventListener('input', scheduleSearch);
        globEl.addEventListener('input', scheduleSearch);
        caseBtn.onclick = () => {
          caseSensitive = !caseSensitive;
          caseBtn.classList.toggle('rps-on', caseSensitive);
          scheduleSearch();
        };
      },
      setRoot(newRoot) {
        root = newRoot || null;
        // Re-run the current query against the new root if there is one.
        if (queryEl && queryEl.value.trim().length >= 2) scheduleSearch();
      },
      destroy() {
        clearTimeout(debounceTimer);
        searchSeq += 1; // invalidate in-flight responses
        container = null;
        queryEl = globEl = caseBtn = statusEl = resultsEl = null;
      },
    };
  }

  // Queue-safe registration (host shell may not be loaded yet).
  const registration = { key: 'search', view: createSearchView() };
  if (typeof window.xnautRightPaneRegisterView === 'function') {
    window.xnautRightPaneRegisterView(registration.key, registration.view);
  } else {
    (window.__xnautRightPaneQueue = window.__xnautRightPaneQueue || []).push(registration);
  }
})();
