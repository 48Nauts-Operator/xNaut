// Right pane — Files view. Lazy directory tree scoped to the project root.
//
// Registers itself with the right-pane host shell. If the host script hasn't
// loaded yet, the registration is queued on window.__xnautRightPaneQueue and
// drained by right-pane.js on load.
(function () {
  'use strict';

  const invoke = (...a) => window.__TAURI__.core.invoke(...a);

  function escapeText(s) {
    return String(s).replace(/[&<>"']/g, (c) => ({ '&':'&amp;', '<':'&lt;', '>':'&gt;', '"':'&quot;', "'":'&#39;' }[c]));
  }

  const ICON_CHEVRON = '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" width="12" height="12"><path d="M6 4l4 4-4 4"/></svg>';
  const ICON_FOLDER = '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" width="14" height="14"><path d="M1.5 3.5h4l1.5 2h7.5v7a.5.5 0 0 1-.5.5H2a.5.5 0 0 1-.5-.5v-9z"/></svg>';
  const ICON_FILE = '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" width="14" height="14"><path d="M4 1.5h5l3 3V14a.5.5 0 0 1-.5.5h-7.5A.5.5 0 0 1 3.5 14V2a.5.5 0 0 1 .5-.5z"/><path d="M9 1.5v3h3"/></svg>';
  const ICON_REFRESH = '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" width="14" height="14"><path d="M13.5 8a5.5 5.5 0 1 1-1.6-3.9"/><path d="M13.5 1.5v3h-3"/></svg>';

  const ROOT_SKIP = new Set(['.git', 'node_modules', 'target']);

  const STYLES = `
.rpf-view { display:flex; flex-direction:column; min-height:0; flex:1 1 0%; }
.rpf-toolbar { display:flex; align-items:center; gap:6px; flex:0 0 auto; padding:6px 10px; border-bottom:1px solid var(--border); }
.rpf-root { flex:1 1 auto; min-width:0; font-size:11px; font-family:var(--font-mono, monospace); color:var(--text-secondary); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; direction:rtl; text-align:left; }
.rpf-refresh { flex:0 0 auto; display:flex; align-items:center; justify-content:center; width:24px; height:24px; border:none; border-radius:var(--radius-md, 6px); background:transparent; color:var(--text-secondary); cursor:pointer; padding:0; }
.rpf-refresh:hover { background:var(--bg-tertiary); color:var(--text-primary); }
.rpf-tree { flex:1 1 0%; min-height:0; overflow-y:auto; padding:4px 0; }
.rpf-row { display:flex; align-items:center; gap:4px; height:24px; padding-right:8px; font-size:12px; color:var(--text-primary); cursor:pointer; white-space:nowrap; overflow:hidden; user-select:none; }
.rpf-row:hover { background:var(--bg-tertiary); }
.rpf-chevron { flex:0 0 14px; display:flex; align-items:center; justify-content:center; color:var(--text-secondary); transition:transform var(--transition-fast, 0.1s); }
.rpf-chevron.rpf-open { transform:rotate(90deg); }
.rpf-icon { flex:0 0 16px; display:flex; align-items:center; justify-content:center; color:var(--text-secondary); }
.rpf-name { flex:1 1 auto; min-width:0; overflow:hidden; text-overflow:ellipsis; }
.rpf-children { display:flex; flex-direction:column; }
.rpf-msg { padding:12px 10px; font-size:12px; color:var(--text-secondary); }
.rpf-menu { position:fixed; z-index:10000; min-width:170px; padding:4px; background:var(--bg-primary); border:1px solid var(--border); border-radius:var(--radius-md, 6px); box-shadow:var(--elev-floating, 0 4px 16px rgba(0,0,0,0.4)); }
.rpf-menu-item { display:block; width:100%; text-align:left; border:none; background:transparent; color:var(--text-primary); font-size:12px; padding:5px 10px; border-radius:4px; cursor:pointer; }
.rpf-menu-item:hover { background:var(--bg-tertiary); }
`;

  function ensureStyles() {
    if (document.getElementById('right-pane-files-styles')) return;
    const el = document.createElement('style');
    el.id = 'right-pane-files-styles';
    el.textContent = STYLES;
    document.head.appendChild(el);
  }

  async function listDir(path) {
    const result = await invoke('list_directory', { path });
    // Backend may return {entries: [...]} or a bare array — handle both.
    if (result && Array.isArray(result.entries)) return result.entries;
    if (Array.isArray(result)) return result;
    return [];
  }

  function sortEntries(entries) {
    return entries.slice().sort((a, b) => {
      if (!!a.is_directory !== !!b.is_directory) return a.is_directory ? -1 : 1;
      const aDot = a.name.startsWith('.'), bDot = b.name.startsWith('.');
      if (aDot !== bDot) return aDot ? 1 : -1; // dotfiles last
      return a.name.localeCompare(b.name, undefined, { sensitivity: 'base' });
    });
  }

  function openFile(path) {
    if (/\.md$/i.test(path)) {
      if (window.xnautOpenMarkdownFile) window.xnautOpenMarkdownFile(path);
    } else {
      if (window.xnautOpenInEditor) window.xnautOpenInEditor(path);
    }
  }

  // ---- Context menu ----------------------------------------------------
  let menuEl = null;
  function closeMenu() {
    if (menuEl) { menuEl.remove(); menuEl = null; }
    document.removeEventListener('mousedown', onMenuDismiss, true);
    document.removeEventListener('keydown', onMenuKey, true);
  }
  function onMenuDismiss(e) { if (menuEl && !menuEl.contains(e.target)) closeMenu(); }
  function onMenuKey(e) { if (e.key === 'Escape') closeMenu(); }

  // Register a folder as a project and switch to its workspace.
  async function openAsProject(path) {
    const name = String(path).replace(/\/+$/, '').split('/').pop() || path;
    try {
      const proj = await invoke('tasks_create_project', { name, path });
      if (window.xnautSidebarRefresh) window.xnautSidebarRefresh();
      if (window.xnautSetActiveProject) await window.xnautSetActiveProject(proj.id, proj);
    } catch (e) {
      console.error('[right-pane-files] open as project failed', e);
    }
  }

  function showMenu(x, y, path, root, isDir) {
    closeMenu();
    const rootClean = String(root || '').replace(/\/+$/, '');
    let rel = path;
    if (rootClean && path.startsWith(rootClean + '/')) rel = path.slice(rootClean.length + 1);
    else if (path === rootClean) rel = '.';

    menuEl = document.createElement('div');
    menuEl.className = 'rpf-menu';
    const items = [];
    if (isDir) items.push(['Open as project', () => openAsProject(path)]);
    items.push(
      ['Copy path', () => navigator.clipboard.writeText(path)],
      ['Copy relative path', () => navigator.clipboard.writeText(rel)],
      ['Insert into terminal', () => { if (window.xnautInjectPathIntoTerminal) window.xnautInjectPathIntoTerminal(path); }],
    );
    for (const [label, fn] of items) {
      const btn = document.createElement('button');
      btn.className = 'rpf-menu-item';
      btn.textContent = label;
      btn.onclick = () => { try { fn(); } catch (e) { console.error('[right-pane-files] menu action failed', e); } closeMenu(); };
      menuEl.appendChild(btn);
    }
    menuEl.style.left = `${Math.min(x, window.innerWidth - 190)}px`;
    menuEl.style.top = `${Math.min(y, window.innerHeight - 110)}px`;
    document.body.appendChild(menuEl);
    document.addEventListener('mousedown', onMenuDismiss, true);
    document.addEventListener('keydown', onMenuKey, true);
  }

  // ---- View ------------------------------------------------------------
  function createFilesView() {
    let container = null;
    let root = null;
    let treeEl = null;
    let rootLabelEl = null;
    let generation = 0; // invalidates in-flight loads after setRoot/refresh

    function makeRow(entry, depth) {
      const row = document.createElement('div');
      row.className = 'rpf-row';
      row.style.paddingLeft = `${8 + depth * 14}px`;
      row.title = entry.path;
      row.innerHTML = `
        <span class="rpf-chevron">${entry.is_directory ? ICON_CHEVRON : ''}</span>
        <span class="rpf-icon">${entry.is_directory ? ICON_FOLDER : ICON_FILE}</span>
        <span class="rpf-name">${escapeText(entry.name)}</span>
      `;
      // Drag the row onto a terminal to insert its path.
      row.draggable = true;
      row.addEventListener('dragstart', (e) => {
        e.dataTransfer.setData('application/x-xnaut-path', entry.path);
        e.dataTransfer.setData('text/plain', entry.path);
        e.dataTransfer.effectAllowed = 'copy';
      });

      const wrap = document.createElement('div');
      wrap.appendChild(row);

      if (entry.is_directory) {
        let childrenEl = null;
        let expanded = false;
        let loading = false;
        row.onclick = async () => {
          // Folder single-click only toggles expand. To put the path in the
          // terminal, use right-click → "Insert into terminal" / "Copy path",
          // or drag the row onto the terminal.
          const chev = row.querySelector('.rpf-chevron');
          if (expanded) {
            expanded = false;
            chev.classList.remove('rpf-open');
            if (childrenEl) childrenEl.style.display = 'none';
            return;
          }
          expanded = true;
          chev.classList.add('rpf-open');
          if (childrenEl) { childrenEl.style.display = ''; return; }
          if (loading) return;
          loading = true;
          childrenEl = document.createElement('div');
          childrenEl.className = 'rpf-children';
          wrap.appendChild(childrenEl);
          const gen = generation;
          try {
            const entries = sortEntries(await listDir(entry.path));
            if (gen !== generation) return;
            if (!entries.length) {
              childrenEl.innerHTML = `<div class="rpf-msg" style="padding-left:${22 + (depth + 1) * 14}px">empty</div>`;
            } else {
              for (const child of entries) childrenEl.appendChild(makeRow(child, depth + 1));
            }
          } catch (e) {
            if (gen === generation) childrenEl.innerHTML = `<div class="rpf-msg">Error: ${escapeText(String(e))}</div>`;
          } finally {
            loading = false;
          }
        };
      } else {
        row.onclick = () => openFile(entry.path);
      }

      row.oncontextmenu = (e) => {
        e.preventDefault();
        showMenu(e.clientX, e.clientY, entry.path, root, entry.is_directory);
      };
      return wrap;
    }

    async function loadRoot() {
      if (!treeEl) return;
      generation += 1;
      const gen = generation;
      if (!root) {
        treeEl.innerHTML = '<div class="rpf-msg">No project root set</div>';
        return;
      }
      treeEl.innerHTML = '<div class="rpf-msg">Loading…</div>';
      try {
        let entries = await listDir(root);
        if (gen !== generation) return;
        entries = sortEntries(entries.filter((e) => !ROOT_SKIP.has(e.name)));
        treeEl.innerHTML = '';
        if (!entries.length) {
          treeEl.innerHTML = '<div class="rpf-msg">Empty directory</div>';
          return;
        }
        for (const entry of entries) treeEl.appendChild(makeRow(entry, 0));
      } catch (e) {
        if (gen === generation) treeEl.innerHTML = `<div class="rpf-msg">Failed to list directory: ${escapeText(String(e))}</div>`;
      }
    }

    return {
      mount(el, initialRoot) {
        ensureStyles();
        container = el;
        root = initialRoot || null;
        container.innerHTML = `
          <div class="rpf-view">
            <div class="rpf-toolbar">
              <span class="rpf-root"></span>
              <button class="rpf-refresh" title="Refresh" aria-label="Refresh file tree">${ICON_REFRESH}</button>
            </div>
            <div class="rpf-tree"></div>
          </div>
        `;
        treeEl = container.querySelector('.rpf-tree');
        rootLabelEl = container.querySelector('.rpf-root');
        rootLabelEl.textContent = root || '';
        rootLabelEl.title = root || '';
        container.querySelector('.rpf-refresh').onclick = () => loadRoot();
        loadRoot();
      },
      setRoot(newRoot) {
        root = newRoot || null;
        if (rootLabelEl) { rootLabelEl.textContent = root || ''; rootLabelEl.title = root || ''; }
        loadRoot(); // collapses everything: full re-render of the root listing
      },
      destroy() {
        closeMenu();
        container = null;
        treeEl = null;
        rootLabelEl = null;
      },
    };
  }

  // Queue-safe registration (host shell may not be loaded yet).
  const registration = { key: 'files', view: createFilesView() };
  if (typeof window.xnautRightPaneRegisterView === 'function') {
    window.xnautRightPaneRegisterView(registration.key, registration.view);
  } else {
    (window.__xnautRightPaneQueue = window.__xnautRightPaneQueue || []).push(registration);
  }
})();
