// The project workspace (XNAUT-336). Picking a project opens its CODE, and
// every other surface about that project hangs off it as a tab.
//
// BORROWED, per CLAUDE.md. The arrangement is Orca's: the centre of the app is
// the repository's code, the surfaces about the project are tabs across the top
// of it, and the things you configure once rather than read daily sit behind a
// three-dot menu beside the project name. No Orca code was read or copied; the
// shape was described from its UI and rebuilt here. Where we depart: Orca keeps
// the file tree in the window rail and the tabs in a side pane, so the tabs are
// narrow. Here the tree lives INSIDE the Code tab, which lets an existing
// full-width surface (Delivery, the Vault, Memory) mount unchanged.
//
// This is assembly. Every tab other than Code is a surface that already exists,
// mounted into this body rather than rebuilt:
//   Work, NAUT-Flow, Designer, Artifacts, Settings, Project details
//        window.xnautCreateProjectManagementPanel(label, host, { project, section })
//        Given a project, that panel now renders NO chrome of its own: no
//        project dropdown, no project rail, no section nav (XNAUT-342). This
//        module used to hide all three with CSS, which is a disguise rather
//        than a fold, and the Work tab lost its New ticket button to it.
//   Delivery   window.xnautCreateDeliveryPanel(label, host, { project, hideProjectSelect:true })
//        It drew its own project dropdown at the top of its left column. The
//        workspace now carries that control for every tab (XNAUT-435), so
//        Delivery's is switched off here: two selects for one project is two
//        ways to switch, and whichever one you did not use is then wrong.
//   Vault      window.xnautCreateVaultPane(label, host, { vault:'work', scopePrefix, projectKey, hideChat:true })
//   Memory     window.xnautCreateMemoryPanel(label, host, { project })
// Nothing falls back to opening its own tab; every tab renders in this body.
//
// TWO HELPERS ARE DUPLICATED HERE, both small, both because the original is a
// closure inside another module with nothing exported to call:
//   1. HLJS_LANG and the highlight pass: LIFTED OUT, 2026-09-13. Both now live
//      in code-render.js and every code surface shares them. What follows
//      described the duplication before it was removed, kept because it says
//      why it existed. It was, from vault-pane.js's
//      showFileInCenter (vault-pane.js:463). It is defined inside
//      createVaultPane, so it is unreachable from outside that pane. This is
//      the obvious candidate for lifting into a shared module: two viewers that
//      paint code should paint it identically by construction, not by care.
//   2. The lazy directory tree, from right-pane-files.js. That module builds
//      exactly ONE view instance and registers it with the right-pane host;
//      mounting that instance in a second container would move it out of the
//      right pane. Only the row shape, the sort and the expand-on-click load
//      are reproduced, and the same `list_directory` command answers both.
//   3. `ago`, from project-management-panel.js's overview facts (its `ago` is
//      also a closure inside createPanel). The four numbers it formatted moved
//      into this header when the Overview tab stopped existing (XNAUT-342).
// Diff painting is NOT duplicated: this workspace shows files, not diffs, so
// vault-pane.js's diffLineHtml and delivery-panel.js's copy stay where they are.
(function () {
  'use strict';

  const invoke = (...a) => window.__TAURI__.core.invoke(...a);

  const esc = (value) => String(value == null ? '' : value).replace(/[&<>"']/g, (c) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  }[c]));

  const panes = new Map();
  let counter = 0;
  const nextLabel = () => `wsp-${Date.now().toString(36)}-${(counter += 1)}`;

  const TABS = [
    ['code', 'Code'], ['work', 'Work'], ['delivery', 'Delivery'],
    ['nautflow', 'NAUT-Flow'], ['vault', 'Vault'], ['memory', 'Memory'],
  ];
  // The three-dot menu: configured once, not read daily. These are NOT tabs and
  // do not join the strip; each opens over the body as a sheet and closes back
  // to whatever tab was underneath (XNAUT-342).
  const MENU = [
    ['designer', 'Designer'], ['artifacts', 'Artifacts'],
    ['settings', 'Settings'], ['details', 'Project details'],
  ];
  const TAB_KEYS = new Set(TABS.map(([key]) => key));
  const LABEL = new Map([...TABS, ...MENU]);
  // Which Project Management section each hosted view asks that panel for.
  const PM_SECTION = {
    work: 'work', nautflow: 'nautflow', designer: 'designer',
    artifacts: 'artifacts', settings: 'settings', details: 'details',
  };

  const ICON_CHEVRON = '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" width="12" height="12"><path d="M6 4l4 4-4 4"/></svg>';
  const ICON_FOLDER = '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" width="14" height="14"><path d="M1.5 3.5h4l1.5 2h7.5v7a.5.5 0 0 1-.5.5H2a.5.5 0 0 1-.5-.5v-9z"/></svg>';
  const ICON_FILE = '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" width="14" height="14"><path d="M4 1.5h5l3 3V14a.5.5 0 0 1-.5.5h-7.5A.5.5 0 0 1 3.5 14V2a.5.5 0 0 1 .5-.5z"/><path d="M9 1.5v3h3"/></svg>';

  // Same skip list as right-pane-files.js: a repository root whose first three
  // rows are .git, node_modules and target is not a view of the work.
  const ROOT_SKIP = new Set(['.git', 'node_modules', 'target']);

  // Monaco is deliberately absent from index.html. The first Edit click adds
  // both local bundle assets; a read-only Code tab downloads and evaluates
  // none of Monaco. The worker source is also embedded in that local bundle:
  // WebKit can refuse Worker URLs served through an installed app's custom
  // protocol, while a blob made from already-loaded bytes works offline.
  let monacoPromise = null;
  let monacoWorkerUrl = '';
  function loadMonaco() {
    if (window.XnautMonacoBundle && window.XnautMonacoBundle.monaco) {
      return Promise.resolve(window.XnautMonacoBundle.monaco);
    }
    if (monacoPromise) return monacoPromise;
    const asset = (path) => new URL(path, document.baseURI).href;
    window.MonacoEnvironment = {
      getWorker() {
        const source = window.XnautMonacoBundle && window.XnautMonacoBundle.workerSource;
        if (!source) throw new Error('Monaco worker source is not loaded');
        if (!monacoWorkerUrl) {
          monacoWorkerUrl = URL.createObjectURL(new Blob([source], { type: 'text/javascript' }));
        }
        return new Worker(monacoWorkerUrl, { name: 'xnaut-monaco-editor-worker' });
      },
    };
    monacoPromise = Promise.all([
      new Promise((resolve, reject) => {
        const link = document.createElement('link');
        link.rel = 'stylesheet';
        link.href = asset('js/vendor/monaco.bundle.css');
        link.dataset.xnautMonaco = 'style';
        link.onload = resolve;
        link.onerror = () => reject(new Error('the bundled Monaco stylesheet could not be loaded'));
        document.head.appendChild(link);
      }),
      new Promise((resolve, reject) => {
        const script = document.createElement('script');
        script.src = asset('js/vendor/monaco.bundle.js');
        script.dataset.xnautMonaco = 'script';
        script.onload = resolve;
        script.onerror = () => reject(new Error('the bundled Monaco editor could not be loaded'));
        document.head.appendChild(script);
      }),
    ]).then(() => {
      if (!window.XnautMonacoBundle || !window.XnautMonacoBundle.monaco) {
        throw new Error('the bundled Monaco editor loaded without its API');
      }
      return window.XnautMonacoBundle.monaco;
    }).catch((error) => {
      monacoPromise = null;
      throw error;
    });
    return monacoPromise;
  }

  const MONACO_LANGUAGE = {
    c: 'c', cc: 'cpp', cpp: 'cpp', cxx: 'cpp', css: 'css', go: 'go', h: 'cpp', hpp: 'cpp',
    html: 'html', htm: 'html', java: 'java', js: 'javascript', jsx: 'javascript',
    json: 'json', md: 'markdown', markdown: 'markdown', py: 'python', rs: 'rust',
    sh: 'shell', bash: 'shell', toml: 'ini', ts: 'typescript', tsx: 'typescript',
    xml: 'xml', yaml: 'yaml', yml: 'yaml',
  };

  function rgbHex(value, fallback) {
    const match = String(value || '').match(/rgba?\((\d+)[, ]+(\d+)[, ]+(\d+)/);
    if (!match) return fallback;
    return `#${match.slice(1, 4).map((part) => Number(part).toString(16).padStart(2, '0')).join('')}`;
  }

  // Resolve custom properties through actual computed color rather than
  // handing Monaco strings such as `var(--foreground)`, which its theme API
  // does not accept.
  function tokenColor(token, fallback) {
    const probe = document.createElement('span');
    probe.style.cssText = `position:fixed;visibility:hidden;color:var(${token},${fallback})`;
    document.body.appendChild(probe);
    const color = rgbHex(getComputedStyle(probe).color, fallback);
    probe.remove();
    return color;
  }

  function isLightColor(hex) {
    const match = String(hex).match(/^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i);
    if (!match) return matchMedia('(prefers-color-scheme: light)').matches;
    const [red, green, blue] = match.slice(1).map((part) => parseInt(part, 16) / 255);
    return (0.2126 * red) + (0.7152 * green) + (0.0722 * blue) > 0.5;
  }

  function defineMonacoTheme(monaco) {
    const bg = tokenColor('--editor-surface', '#1e1e1e');
    const fg = tokenColor('--foreground', '#fafafa');
    const muted = tokenColor('--muted-foreground', '#a1a1a1');
    const focus = tokenColor('--sidebar-primary', '#1447e6');
    const added = tokenColor('--git-added', '#81b88b');
    const deleted = tokenColor('--git-deleted', '#c74e39');
    const light = isLightColor(bg);
    monaco.editor.defineTheme('xnaut-code', {
      base: light ? 'vs' : 'vs-dark', inherit: true, rules: [],
      colors: {
        'editor.background': bg,
        'editor.foreground': fg,
        'editorLineNumber.foreground': muted,
        'editorLineNumber.activeForeground': fg,
        'editorCursor.foreground': fg,
        'editor.selectionBackground': `${focus}66`,
        'editor.inactiveSelectionBackground': `${focus}33`,
        'editorGutter.background': bg,
        'diffEditor.insertedTextBackground': `${added}33`,
        'diffEditor.removedTextBackground': `${deleted}33`,
      },
    });
    monaco.editor.setTheme('xnaut-code');
  }


  // A file we refuse to print rather than print as mojibake. read_file returns
  // a String, so bytes that are not text arrive already mangled; the extension
  // is the only honest signal available before the read, and a NUL byte is the
  // honest signal after it.
  const BINARY_EXT = new Set([
    'png', 'jpg', 'jpeg', 'gif', 'bmp', 'ico', 'icns', 'webp', 'tiff', 'avif', 'heic',
    'pdf', 'zip', 'gz', 'tgz', 'bz2', 'xz', '7z', 'rar', 'dmg', 'pkg', 'deb', 'rpm',
    'exe', 'dll', 'dylib', 'so', 'o', 'a', 'class', 'jar', 'wasm', 'bin', 'img',
    'db', 'sqlite', 'sqlite3', 'woff', 'woff2', 'ttf', 'otf', 'eot',
    'mp3', 'mp4', 'm4a', 'mov', 'avi', 'mkv', 'wav', 'flac', 'ogg', 'webm', 'psd',
  ]);

  const extOf = (path) => (String(path).split('/').pop().split('.').pop() || '').toLowerCase();
  const baseOf = (path) => String(path).split('/').pop();
  const trimSlash = (path) => String(path || '').replace(/\/+$/, '');

  function relativeTo(root, path) {
    const clean = trimSlash(root);
    const p = String(path || '');
    if (clean && p.startsWith(clean + '/')) return p.slice(clean.length + 1);
    return p;
  }

  // The vault folder for a project, same rule as the PM panel's
  // stageDocumentRef (project-management-panel.js:954): the project NAME, with
  // path-hostile characters replaced, falling back to the key.
  function vaultFolder(project) {
    if (!project) return '';
    const name = String(project.name || project.key || '').replace(/[\\/:*?"<>|]/g, '-').trim();
    return name || String(project.key || '');
  }

  function injectStyles() {
    if (document.getElementById('workspace-styles')) return;
    const style = document.createElement('style');
    style.id = 'workspace-styles';
    style.textContent = `
      .wsp { display:flex; flex:1 1 auto; flex-direction:column; min-width:0; min-height:0; overflow:hidden;
        color:var(--text-primary,#e0e0e0); background:var(--bg-primary,#0a0a0f);
        font-family:var(--font-ui,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif); font-size:12px; }
      .wsp * { box-sizing:border-box; }
      /* .wsp-code is display:flex, which beats a bare [hidden]. */
      .wsp [hidden] { display:none !important; }
      .wsp-head { display:flex; align-items:center; gap:10px; flex:0 0 auto; padding:10px 14px;
        border-bottom:1px solid var(--border,#2a2a2f); }
      .wsp-name { font-size:13px; font-weight:600; }
      .wsp-facts { display:flex; align-items:center; gap:14px; flex:0 0 auto; }
      .wsp-fact { display:flex; align-items:baseline; gap:5px; white-space:nowrap; }
      .wsp-fact label { color:var(--text-secondary,#a0a0a0); font-size:10px; text-transform:uppercase; letter-spacing:0.04em; }
      .wsp-fact strong { color:var(--text-primary,#e0e0e0); font-size:12px; font-weight:600; }
      /* Why a number is absent (XNAUT-341). Quiet on purpose: it is an
         explanation sitting beside facts, not a warning about them. */
      .wsp-why { color:var(--text-secondary,#a0a0a0); font-size:10px; font-style:italic;
        white-space:nowrap; overflow:hidden; text-overflow:ellipsis; max-width:240px; }
      .wsp-root { font-family:var(--font-mono,monospace); font-size:11px; color:var(--text-secondary,#a0a0a0);
        overflow:hidden; text-overflow:ellipsis; white-space:nowrap; min-width:0; }
      .wsp-spacer { flex:1 1 auto; }
      .wsp-dots { border:1px solid var(--border,#2a2a2f); background:transparent; color:var(--text-secondary,#a0a0a0);
        border-radius:var(--radius-md,6px); width:26px; height:24px; cursor:pointer; padding:0; line-height:1; }
      .wsp-dots:hover { background:var(--bg-tertiary,#2a2a2f); color:var(--text-primary,#e0e0e0); }
      .wsp-tabs { display:flex; align-items:center; gap:2px; flex:0 0 auto; padding:0 10px;
        border-bottom:1px solid var(--border,#2a2a2f); }
      .wsp-tabs button { border:none; background:transparent; color:var(--text-secondary,#a0a0a0);
        font-size:12px; padding:8px 12px; cursor:pointer; border-bottom:2px solid transparent; }
      .wsp-tabs button:hover { color:var(--text-primary,#e0e0e0); }
      .wsp-tabs button.active { color:var(--text-primary,#e0e0e0); border-bottom-color:var(--text-primary,#e0e0e0); }
      /* The project switcher (XNAUT-435). It sits between the tab strip and the
         body rather than inside the Code tab, because it has to be on EVERY
         tab: the Delivery tab's own select is gone and the sidebar tree can be
         replaced by the Sessions list, so on most tabs this is the only way to
         change project. Constrained to the left column's width so it reads as
         the head of that column — above the file tree on Code, above whatever
         list the hosted surface puts there on the others. */
      .wsp-projbar { display:flex; align-items:flex-end; flex:0 0 auto; padding-top:6px;
        border-bottom:1px solid var(--border,#2a2a2f); }
      .wsp-projbar .xps { width:240px; }
      .wsp-body { position:relative; display:flex; flex:1 1 auto; min-height:0; min-width:0; overflow:hidden; }
      .wsp-surface { display:flex; flex:1 1 auto; min-width:0; min-height:0; overflow:hidden; }
      /* A configuration surface from the three-dot menu, over the body rather
         than beside the tabs. */
      .wsp-sheet { position:absolute; inset:0; z-index:20; display:flex; flex-direction:column;
        background:var(--bg-primary,#0a0a0f); }
      .wsp-sheet-head { display:flex; align-items:center; gap:10px; flex:0 0 auto; padding:8px 14px;
        border-bottom:1px solid var(--border,#2a2a2f); }
      .wsp-sheet-title { font-size:12px; font-weight:600; }
      .wsp-sheet-close { border:1px solid var(--border,#2a2a2f); background:transparent; color:var(--text-secondary,#a0a0a0);
        border-radius:var(--radius-md,6px); width:24px; height:22px; cursor:pointer; padding:0; line-height:1; }
      .wsp-sheet-close:hover { background:var(--bg-tertiary,#2a2a2f); color:var(--text-primary,#e0e0e0); }
      .wsp-sheet-body { display:flex; flex:1 1 auto; min-width:0; min-height:0; overflow:hidden; }
      .wsp-code { display:flex; flex:1 1 auto; min-width:0; min-height:0; overflow:hidden; }
      .wsp-tree { flex:0 0 240px; min-width:0; overflow:auto; padding:4px 0;
        border-right:1px solid var(--border,#2a2a2f); background:var(--bg-secondary,#141419); }
      .wsp-row { display:flex; align-items:center; gap:4px; height:24px; padding-right:8px; font-size:12px;
        color:var(--text-primary,#e0e0e0); cursor:pointer; white-space:nowrap; overflow:hidden; user-select:none; }
      .wsp-row:hover { background:var(--bg-tertiary,#2a2a2f); }
      .wsp-row.active { background:var(--bg-tertiary,#2a2a2f); }
      .wsp-row.dirty .wsp-rowname::after { content:' M'; color:var(--git-modified,#e2c08d); }
      .wsp-chevron { flex:0 0 14px; display:flex; align-items:center; justify-content:center;
        color:var(--text-secondary,#a0a0a0); transition:transform 0.1s; }
      .wsp-chevron.open { transform:rotate(90deg); }
      .wsp-icon { flex:0 0 16px; display:flex; align-items:center; justify-content:center; color:var(--text-secondary,#a0a0a0); }
      .wsp-rowname { flex:1 1 auto; min-width:0; overflow:hidden; text-overflow:ellipsis; }
      .wsp-file { display:flex; flex-direction:column; flex:1 1 auto; min-width:0; min-height:0; overflow:hidden; }
      .wsp-ftabs { display:flex; align-items:center; gap:2px; flex:0 0 auto; overflow-x:auto;
        border-bottom:1px solid var(--border,#2a2a2f); }
      .wsp-ftab { display:flex; align-items:center; gap:6px; padding:6px 8px 6px 10px; cursor:pointer;
        font-size:12px; color:var(--text-secondary,#a0a0a0); border-right:1px solid var(--border,#2a2a2f); white-space:nowrap; }
      .wsp-ftab.active { color:var(--text-primary,#e0e0e0); background:var(--bg-secondary,#141419); }
      .wsp-ftab-close { color:var(--text-secondary,#a0a0a0); font-size:13px; line-height:1; }
      .wsp-ftab-close:hover { color:var(--text-primary,#e0e0e0); }
      .wsp-view { display:flex; flex-direction:column; flex:1 1 auto; min-height:0; overflow:hidden; }
      .wsp-filebar { display:flex; align-items:center; gap:6px; flex:0 0 auto; min-height:34px; padding:4px 8px;
        border-bottom:1px solid var(--border,#2a2a2f); background:var(--bg-secondary,#141419); }
      .wsp-filepath { flex:1 1 auto; min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;
        color:var(--text-secondary,#a0a0a0); font-family:var(--font-mono,monospace); font-size:11px; }
      .wsp-fileaction { border:1px solid var(--border,#2a2a2f); border-radius:var(--radius-md,6px);
        background:transparent; color:var(--text-primary,#e0e0e0); font:inherit; font-size:11px; padding:4px 8px; cursor:pointer; }
      .wsp-fileaction:hover { background:var(--bg-tertiary,#2a2a2f); }
      .wsp-fileaction.primary { background:var(--primary,#e5e5e5); color:var(--primary-foreground,#171717); }
      .wsp-fileaction:disabled { opacity:.55; cursor:default; }
      .wsp-reader { flex:1 1 auto; min-height:0; overflow:auto; }
      .wsp-monaco { flex:1 1 auto; min-height:0; background:var(--editor-surface,#1e1e1e); }
      .wsp-monaco .monaco-editor { --monaco-monospace-font:var(--font-mono,ui-monospace,"SF Mono",Menlo,monospace);
        font-family:var(--font-mono,ui-monospace,"SF Mono",Menlo,monospace) !important; }
      .wsp-monaco .monaco-editor .view-lines { font-family:var(--font-mono,ui-monospace,"SF Mono",Menlo,monospace) !important; }
      .wsp-edit-error { flex:0 1 auto; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;
        color:var(--danger,#e5534b); font-size:11px; }
      .wsp-view pre { margin:0; padding:8px 0; font-family:var(--font-mono,monospace); font-size:12px; line-height:1.5; }
      .wsp-view pre code { display:block; }
      .wsp-ln { display:inline-block; width:4em; padding-right:14px; text-align:right; user-select:none;
        color:var(--text-secondary,#a0a0a0); opacity:0.6; }
      .wsp-msg { padding:16px 18px; color:var(--text-secondary,#a0a0a0); line-height:1.6; }
      .wsp-msg strong { color:var(--text-primary,#e0e0e0); }
      .wsp-msg.error strong { color:var(--danger,#e5534b); }
      .wsp-menu { position:fixed; z-index:10000; min-width:170px; padding:4px; background:var(--bg-primary,#0a0a0f);
        border:1px solid var(--border,#2a2a2f); border-radius:var(--radius-md,6px);
        box-shadow:var(--elev-floating,0 4px 16px rgba(0,0,0,0.4)); }
      .wsp-menu-item { display:block; width:100%; text-align:left; border:none; background:transparent;
        color:var(--text-primary,#e0e0e0); font-size:12px; padding:6px 10px; border-radius:4px; cursor:pointer; }
      .wsp-menu-item:hover { background:var(--bg-tertiary,#2a2a2f); }
      @media (max-width:900px) { .wsp-tree { flex-basis:180px; } .wsp-filepath { display:none; } }
    `;
    document.head.appendChild(style);
  }

  async function listDir(path) {
    const result = await invoke('list_directory', { path });
    if (result && Array.isArray(result.entries)) return result.entries;
    if (Array.isArray(result)) return result;
    return [];
  }

  // Directories first, dotfiles last, then case-insensitive by name. Same order
  // as right-pane-files.js, so the two trees do not disagree about a repo.
  function sortEntries(entries) {
    return entries.slice().sort((a, b) => {
      if (!!a.is_directory !== !!b.is_directory) return a.is_directory ? -1 : 1;
      const aDot = String(a.name).startsWith('.');
      const bDot = String(b.name).startsWith('.');
      if (aDot !== bDot) return aDot ? 1 : -1;
      return String(a.name).localeCompare(String(b.name), undefined, { sensitivity: 'base' });
    });
  }

  // Duplicated from vault-pane.js's showFileInCenter. See the header.
  // Delegates to code-render.js, which is the one painter for every code
  // surface in the app. This used to be its own copy of the highlight pass,
  // with its own font size, which is how the Code tab and the Vault tab ended
  // up rendering the same file two different ways.
  function highlightWithLineNumbers(content, ext) {
    if (typeof window.xnautRenderCode === 'function') {
      return window.xnautRenderCode(content, ext ? `x.${ext}` : '');
    }
    return `<pre class="hljs"><code>${esc(content)}</code></pre>`;
  }

  let menuEl = null;
  function closeMenu() {
    if (menuEl) { menuEl.remove(); menuEl = null; }
    document.removeEventListener('mousedown', onMenuDismiss, true);
    document.removeEventListener('keydown', onMenuKey, true);
  }
  function onMenuDismiss(event) { if (menuEl && !menuEl.contains(event.target)) closeMenu(); }
  function onMenuKey(event) { if (event.key === 'Escape') closeMenu(); }

  async function createWorkspacePanel(tabId, parent, opts) {
    opts = opts || {};
    injectStyles();
    const label = nextLabel();

    const pane = document.createElement('section');
    pane.className = 'wsp';
    pane.innerHTML = `
      <header class="wsp-head">
        <span class="wsp-name"></span>
        <span class="wsp-root"></span>
        <span class="wsp-spacer"></span>
        <span class="wsp-facts">
          <span class="wsp-fact"><label>Last commit</label><strong data-fact="lastcommit">—</strong></span>
          <span class="wsp-fact"><label>Uncommitted</label><strong data-fact="changes">—</strong></span>
          <span class="wsp-fact"><label>Worktrees</label><strong data-fact="worktrees">—</strong></span>
          <span class="wsp-fact"><label>Tickets</label><strong data-fact="tickets">—</strong></span>
          <span class="wsp-why" data-fact-why hidden></span>
        </span>
        <button class="wsp-dots" title="More about this project" aria-label="More about this project">&#8943;</button>
      </header>
      <nav class="wsp-tabs" role="tablist"></nav>
      <div class="wsp-projbar"></div>
      <div class="wsp-body">
        <div class="wsp-code">
          <div class="wsp-tree"></div>
          <div class="wsp-file">
            <div class="wsp-ftabs" hidden></div>
            <div class="wsp-view"></div>
          </div>
        </div>
        <div class="wsp-surface" hidden></div>
        <div class="wsp-sheet" hidden>
          <header class="wsp-sheet-head">
            <span class="wsp-sheet-title"></span>
            <span class="wsp-spacer"></span>
            <button class="wsp-sheet-close" title="Close" aria-label="Close">&times;</button>
          </header>
          <div class="wsp-sheet-body"></div>
        </div>
      </div>`;
    parent.appendChild(pane);

    const $ = (selector) => pane.querySelector(selector);
    const treeEl = $('.wsp-tree');
    const codeEl = $('.wsp-code');
    const surfaceEl = $('.wsp-surface');
    const sheetEl = $('.wsp-sheet');
    const sheetBodyEl = $('.wsp-sheet-body');
    const ftabsEl = $('.wsp-ftabs');
    const viewEl = $('.wsp-view');
    const projbarEl = $('.wsp-projbar');

    const state = {
      projectKey: '',
      project: null,
      projects: [],
      worktree: String(opts.worktree || ''),
      tab: 'code',
      pendingAction: String(opts.action || ''),
      pendingTicket: String(opts.ticket || ''),
      open: [],          // absolute paths, in tab order
      active: '',        // the file being read
      treeGeneration: 0, // invalidates in-flight directory loads after a re-root
      surfaceGeneration: 0,
      surfaceEntry: null,
      sheetGeneration: 0,
      sheetEntry: null,
      factsGeneration: 0,
      files: new Map(),  // path -> { disk, draft, mode }
      dirty: new Set(),  // absolute paths reported by git after a write
    };

    let codeEditor = null;
    let codeModel = null;
    let originalModel = null;
    let modelChange = null;
    const themeObserver = new MutationObserver(() => {
      const monaco = window.XnautMonacoBundle && window.XnautMonacoBundle.monaco;
      if (monaco && codeEditor) defineMonacoTheme(monaco);
    });
    themeObserver.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['data-theme', 'style'],
    });

    function activeRecord() { return state.files.get(state.active); }

    function keepDraft() {
      const record = activeRecord();
      if (record && codeModel && record.mode === 'edit') record.draft = codeModel.getValue();
    }

    function disposeCodeEditor() {
      keepDraft();
      try { modelChange?.dispose(); } catch (_error) { /* already disposed */ }
      try { codeEditor?.dispose(); } catch (_error) { /* already disposed */ }
      try { originalModel?.dispose(); } catch (_error) { /* already disposed */ }
      try { codeModel?.dispose(); } catch (_error) { /* already disposed */ }
      modelChange = null;
      codeEditor = null;
      originalModel = null;
      codeModel = null;
    }

    const root = () => state.worktree || (state.project && state.project.source_path) || '';

    function message(host, text, kind) {
      host.innerHTML = `<div class="wsp-msg${kind === 'error' ? ' error' : ''}"><strong>${esc(text)}</strong></div>`;
    }

    // ── Code: the tree ────────────────────────────────────────────────────
    function makeRow(entry, depth) {
      const wrap = document.createElement('div');
      const row = document.createElement('div');
      row.className = 'wsp-row';
      row.style.paddingLeft = `${8 + depth * 14}px`;
      row.title = entry.path;
      if (entry.is_directory) row.dataset.dir = entry.path;
      else row.dataset.file = entry.path;
      row.innerHTML = `
        <span class="wsp-chevron">${entry.is_directory ? ICON_CHEVRON : ''}</span>
        <span class="wsp-icon">${entry.is_directory ? ICON_FOLDER : ICON_FILE}</span>
        <span class="wsp-rowname">${esc(entry.name)}</span>`;
      wrap.appendChild(row);

      if (entry.is_directory) {
        let childrenEl = null;
        let expanded = false;
        let loading = false;
        row.onclick = async () => {
          const chevron = row.querySelector('.wsp-chevron');
          if (expanded) {
            expanded = false;
            chevron.classList.remove('open');
            if (childrenEl) childrenEl.hidden = true;
            return;
          }
          expanded = true;
          chevron.classList.add('open');
          if (childrenEl) { childrenEl.hidden = false; return; }
          if (loading) return;
          loading = true;
          childrenEl = document.createElement('div');
          wrap.appendChild(childrenEl);
          const generation = state.treeGeneration;
          try {
            const entries = sortEntries(await listDir(entry.path));
            if (generation !== state.treeGeneration) return;
            if (!entries.length) {
              childrenEl.innerHTML = `<div class="wsp-msg" style="padding-left:${22 + (depth + 1) * 14}px">empty</div>`;
            } else {
              for (const child of entries) childrenEl.appendChild(makeRow(child, depth + 1));
            }
          } catch (error) {
            if (generation === state.treeGeneration) {
              childrenEl.innerHTML = `<div class="wsp-msg error"><strong>${esc(String(error))}</strong></div>`;
            }
          } finally {
            loading = false;
          }
        };
      } else {
        row.onclick = () => openFile(entry.path);
      }
      return wrap;
    }

    async function loadTree() {
      state.treeGeneration += 1;
      const generation = state.treeGeneration;
      const dir = root();
      if (!dir) {
        // A project with no checkout on this machine has no code to open, and
        // an empty tree would read as an empty repository.
        message(treeEl, state.projectKey
          ? `${state.projectKey} has no source path, so there is no checkout to open here.`
          : 'No project selected.');
        return;
      }
      treeEl.innerHTML = '<div class="wsp-msg">Loading…</div>';
      try {
        let entries = await listDir(dir);
        if (generation !== state.treeGeneration) return;
        entries = sortEntries(entries.filter((item) => !ROOT_SKIP.has(item.name)));
        treeEl.innerHTML = '';
        if (!entries.length) { message(treeEl, 'This directory is empty.'); return; }
        for (const entry of entries) treeEl.appendChild(makeRow(entry, 0));
        markActiveRow();
      } catch (error) {
        if (generation === state.treeGeneration) {
          message(treeEl, `Could not list ${dir}: ${String(error)}`, 'error');
        }
      }
    }

    function markActiveRow() {
      treeEl.querySelectorAll('.wsp-row').forEach((row) => {
        row.classList.toggle('active', !!state.active && row.dataset.file === state.active);
        row.classList.toggle('dirty', !!row.dataset.file && state.dirty.has(row.dataset.file));
      });
    }

    async function refreshDirtyRows() {
      const dir = root();
      if (!dir) return;
      try {
        const files = (await invoke('git_uncommitted_files', { repo: dir })) || [];
        const prefix = trimSlash(dir) + '/';
        state.dirty = new Set(files.map((file) => {
          const path = String(file.path || '');
          return path.startsWith('/') ? path : prefix + path;
        }));
        markActiveRow();
        renderFileTabs();
        loadFacts();
      } catch (_error) { /* the existing tree state remains honest */ }
    }

    // ── Code: the open files ──────────────────────────────────────────────
    function renderFileTabs() {
      ftabsEl.hidden = state.open.length === 0;
      ftabsEl.innerHTML = state.open.map((path) => `
        <div class="wsp-ftab${path === state.active ? ' active' : ''}" data-open-file="${esc(path)}" title="${esc(path)}">
          <span>${esc(baseOf(path))}${state.dirty.has(path) ? ' •' : ''}</span>
          <span class="wsp-ftab-close" data-close-file="${esc(path)}" role="button" aria-label="Close ${esc(baseOf(path))}">&times;</span>
        </div>`).join('');
      ftabsEl.querySelectorAll('[data-open-file]').forEach((el) => {
        el.onclick = (event) => {
          if (event.target.closest('[data-close-file]')) return;
          selectFile(el.dataset.openFile);
        };
      });
      ftabsEl.querySelectorAll('[data-close-file]').forEach((el) => {
        el.onclick = (event) => { event.stopPropagation(); closeFile(el.dataset.closeFile); };
      });
    }

    function openFile(path) {
      if (!state.open.includes(path)) state.open.push(path);
      selectFile(path);
    }

    function closeFile(path) {
      if (state.active === path) disposeCodeEditor();
      state.open = state.open.filter((item) => item !== path);
      state.files.delete(path);
      if (state.active === path) {
        state.active = state.open[state.open.length - 1] || '';
        if (state.active) renderActiveFile();
        else viewEl.innerHTML = emptyCodeHtml();
      }
      renderFileTabs();
      markActiveRow();
    }

    function emptyCodeHtml() {
      return '<div class="wsp-msg">No file open. Pick one from the tree.</div>';
    }

    function selectFile(path) {
      if (state.active && state.active !== path) disposeCodeEditor();
      state.active = path;
      renderFileTabs();
      markActiveRow();
      renderActiveFile();
    }

    async function renderActiveFile() {
      const path = state.active;
      if (!path) { viewEl.innerHTML = emptyCodeHtml(); return; }
      disposeCodeEditor();
      const ext = extOf(path);
      if (BINARY_EXT.has(ext)) {
        // Not read at all: read_file returns a String, so asking for a PNG
        // would hand us mangled bytes to print.
        message(viewEl, `${baseOf(path)} is a binary file (.${ext}). It is not shown as text.`);
        return;
      }
      viewEl.innerHTML = '<div class="wsp-msg">Loading…</div>';
      let content;
      try {
        content = await invoke('read_file', { path });
      } catch (error) {
        if (state.active !== path) return;
        const text = String(error);
        const gone = /No such file|not found|os error 2|ENOENT/i.test(text);
        message(viewEl, gone
          ? `${baseOf(path)} is gone from disk. It was open here, and it is no longer at ${path}.`
          : `Could not read ${baseOf(path)}: ${text}`, 'error');
        return;
      }
      if (state.active !== path) return;
      if (content == null) {
        message(viewEl, `Could not read ${baseOf(path)}: the backend returned nothing.`, 'error');
        return;
      }
      // A NUL byte in a String is not text, whatever the extension claimed.
      if (content.indexOf('\u0000') !== -1) {
        message(viewEl, `${baseOf(path)} is a binary file. It is not shown as text.`);
        return;
      }
      let record = state.files.get(path);
      if (!record) {
        record = { disk: content, draft: content, mode: 'view' };
        state.files.set(path, record);
      } else if (record.draft === record.disk) {
        // No local draft: a file changed by an agent while its tab was hidden
        // should show what is really on disk when it is selected again.
        record.disk = content;
        record.draft = content;
      } else {
        record.disk = content;
      }
      if (record.mode === 'edit') await renderEditor(path, record);
      else if (record.mode === 'diff') await renderDiff(path, record);
      else renderViewer(path, record);
    }

    function fileToolbar(path, actions, error) {
      return `<div class="wsp-filebar">
        <span class="wsp-filepath" title="${esc(path)}">${esc(relativeTo(root(), path))}</span>
        ${error ? `<span class="wsp-edit-error" role="alert" title="${esc(error)}">${esc(error)}</span>` : ''}
        ${actions.map((action) => `<button class="wsp-fileaction${action.primary ? ' primary' : ''}"
          data-file-action="${action.key}"${action.disabled ? ' disabled' : ''}>${esc(action.label)}</button>`).join('')}
      </div>`;
    }

    function wireOpenInEditor(path) {
      const button = viewEl.querySelector('[data-file-action="external"]');
      if (button) button.onclick = () => window.xnautOpenInEditor(path);
    }

    function renderViewer(path, record, error) {
      record.mode = 'view';
      viewEl.innerHTML = fileToolbar(path, [
        { key: 'edit', label: 'Edit', primary: true },
        { key: 'external', label: 'Open in editor' },
      ], error) + `<div class="wsp-reader">${highlightWithLineNumbers(record.disk, extOf(path))}</div>`;
      viewEl.querySelector('[data-file-action="edit"]').onclick = () => {
        record.draft = record.disk;
        record.mode = 'edit';
        renderEditor(path, record);
      };
      wireOpenInEditor(path);
    }

    function editorFace() {
      let sample = viewEl.querySelector('.xcr');
      let remove = false;
      if (!sample) {
        sample = document.createElement('pre');
        sample.className = 'xcr';
        sample.style.cssText = 'position:fixed;visibility:hidden';
        document.body.appendChild(sample);
        remove = true;
      }
      const style = getComputedStyle(sample);
      const face = {
        fontFamily: style.fontFamily,
        fontSize: parseFloat(style.fontSize) || 12,
        lineHeight: parseFloat(style.lineHeight) || 18,
      };
      if (remove) sample.remove();
      return face;
    }

    function editorOptions(path, face) {
      return {
        automaticLayout: true,
        ariaLabel: `Editing ${baseOf(path)}`,
        fontFamily: face.fontFamily,
        fontSize: face.fontSize,
        lineHeight: face.lineHeight,
        minimap: { enabled: false },
        padding: { top: 8 },
        renderWhitespace: 'selection',
        scrollBeyondLastLine: false,
        tabSize: 2,
        theme: 'xnaut-code',
      };
    }

    async function renderEditor(path, record, error) {
      disposeCodeEditor();
      const face = editorFace();
      const started = performance.now();
      viewEl.innerHTML = fileToolbar(path, [
        { key: 'cancel', label: 'Cancel' },
        { key: 'save', label: 'Save', primary: true },
        { key: 'external', label: 'Open in editor' },
      ], error) + '<div class="wsp-monaco" data-monaco-mode="edit"><div class="wsp-msg">Loading editor…</div></div>';
      wireOpenInEditor(path);
      viewEl.querySelector('[data-file-action="cancel"]').onclick = () => {
        record.draft = record.disk;
        disposeCodeEditor();
        renderViewer(path, record);
      };
      viewEl.querySelector('[data-file-action="save"]').onclick = () => reviewChanges(path, record);
      let monaco;
      try {
        monaco = await loadMonaco();
      } catch (loadError) {
        if (state.active === path) renderViewer(path, record, `Could not load Monaco: ${String(loadError)}`);
        return;
      }
      if (state.active !== path || record.mode !== 'edit') return;
      defineMonacoTheme(monaco);
      const host = viewEl.querySelector('.wsp-monaco[data-monaco-mode="edit"]');
      if (!host) return;
      host.innerHTML = '';
      codeModel = monaco.editor.createModel(
        record.draft,
        MONACO_LANGUAGE[extOf(path)] || 'plaintext',
        monaco.Uri.parse(`inmemory://xnaut/${encodeURIComponent(label)}/edit/${encodeURIComponent(path)}`),
      );
      codeEditor = monaco.editor.create(host, { ...editorOptions(path, face), model: codeModel });
      modelChange = codeModel.onDidChangeContent(() => { record.draft = codeModel.getValue(); });
      viewEl.dataset.monacoLoadMs = String(Math.round(performance.now() - started));
      codeEditor.focus();
    }

    async function reviewChanges(path, record) {
      if (codeModel) record.draft = codeModel.getValue();
      let disk;
      try {
        disk = await invoke('read_file', { path });
      } catch (error) {
        await renderEditor(path, record, `Could not refresh ${baseOf(path)}: ${String(error)}`);
        return;
      }
      if (state.active !== path) return;
      record.disk = String(disk == null ? '' : disk);
      record.mode = 'diff';
      await renderDiff(path, record);
    }

    async function renderDiff(path, record, error) {
      disposeCodeEditor();
      const face = editorFace();
      viewEl.innerHTML = fileToolbar(path, [
        { key: 'back', label: 'Back to edit' },
        { key: 'write', label: 'Write the file', primary: true },
        { key: 'external', label: 'Open in editor' },
      ], error) + '<div class="wsp-monaco" data-monaco-mode="diff"><div class="wsp-msg">Loading diff…</div></div>';
      wireOpenInEditor(path);
      let monaco;
      try {
        monaco = await loadMonaco();
      } catch (loadError) {
        if (state.active === path) await renderEditor(path, record, `Could not load Monaco diff: ${String(loadError)}`);
        return;
      }
      if (state.active !== path || record.mode !== 'diff') return;
      defineMonacoTheme(monaco);
      const host = viewEl.querySelector('.wsp-monaco[data-monaco-mode="diff"]');
      if (!host) return;
      host.innerHTML = '';
      const language = MONACO_LANGUAGE[extOf(path)] || 'plaintext';
      originalModel = monaco.editor.createModel(
        record.disk,
        language,
        monaco.Uri.parse(`inmemory://xnaut/${encodeURIComponent(label)}/original/${encodeURIComponent(path)}`),
      );
      codeModel = monaco.editor.createModel(
        record.draft,
        language,
        monaco.Uri.parse(`inmemory://xnaut/${encodeURIComponent(label)}/modified/${encodeURIComponent(path)}`),
      );
      codeEditor = monaco.editor.createDiffEditor(host, {
        ...editorOptions(path, face),
        ariaLabel: `Review changes to ${baseOf(path)}`,
        originalEditable: false,
        readOnly: true,
        renderSideBySide: true,
      });
      codeEditor.setModel({ original: originalModel, modified: codeModel });
      viewEl.querySelector('[data-file-action="back"]').onclick = () => {
        record.draft = codeModel.getValue();
        record.mode = 'edit';
        renderEditor(path, record);
      };
      viewEl.querySelector('[data-file-action="write"]').onclick = () => writeCurrent(path, record);
    }

    async function writeCurrent(path, record) {
      if (codeModel) record.draft = codeModel.getValue();
      const button = viewEl.querySelector('[data-file-action="write"]');
      if (button) button.disabled = true;
      try {
        // The backend checks the live writer lease HERE, after the diff has
        // been read. An agent that takes the lease while the human reviews
        // still wins; the browser never writes around that check.
        await invoke('code_edit_save', { path, content: record.draft });
      } catch (error) {
        await renderDiff(path, record, String(error));
        return;
      }
      record.disk = record.draft;
      record.mode = 'view';
      disposeCodeEditor();
      renderViewer(path, record);
      await refreshDirtyRows();
    }

    // ── The hosted surfaces ───────────────────────────────────────────────
    function disposeEntry(entry) {
      if (!entry) return;
      try { entry.dispose?.(); } catch (_e) { /* already gone */ }
      try { entry.destroy?.(); } catch (_e) { /* already gone */ }
    }

    // Mounts one hosted surface into a container and hands the entry back.
    // `stale()` is asked after the await, because a tab switch or a project
    // change during it must not leave a second surface mounted behind the one
    // on screen.
    async function mountInto(name, container, stale) {
      container.innerHTML = '';
      if (!state.projectKey) {
        message(container, 'No project selected, so there is nothing to show here yet.');
        return null;
      }
      const host = document.createElement('div');
      host.style.cssText = 'display:flex; flex:1 1 auto; min-width:0; min-height:0; overflow:hidden;';
      container.appendChild(host);
      let entry;
      try {
        entry = await surfaceFactory(name, host);
      } catch (error) {
        if (!stale()) message(container, `${LABEL.get(name) || name} failed to open: ${String(error)}`, 'error');
        return null;
      }
      if (stale() || !host.isConnected) { disposeEntry(entry); return null; }
      if (!entry) message(container, `${LABEL.get(name) || name} is not available in this build.`);
      return entry || null;
    }

    function disposeSurface() {
      state.surfaceGeneration += 1;
      disposeEntry(state.surfaceEntry);
      state.surfaceEntry = null;
      surfaceEl.innerHTML = '';
    }

    async function mountSurface(name) {
      disposeSurface();
      const generation = state.surfaceGeneration;
      const stale = () => generation !== state.surfaceGeneration;
      const entry = await mountInto(name, surfaceEl, stale);
      if (stale()) { disposeEntry(entry); return; }
      state.surfaceEntry = entry;
    }

    // ── The three-dot sheet ───────────────────────────────────────────────
    // Designer, Artifacts, Settings and Project details are things you
    // configure once, so they are not tabs: the sheet covers the body, names
    // what it is, and closes back to the tab that was underneath (XNAUT-342).
    function closeSheet() {
      state.sheetGeneration += 1;
      disposeEntry(state.sheetEntry);
      state.sheetEntry = null;
      sheetBodyEl.innerHTML = '';
      sheetEl.hidden = true;
      $('.wsp-sheet-title').textContent = '';
    }

    async function openSheet(name) {
      closeSheet();
      const generation = state.sheetGeneration;
      const stale = () => generation !== state.sheetGeneration;
      $('.wsp-sheet-title').textContent = LABEL.get(name) || name;
      sheetEl.hidden = false;
      const entry = await mountInto(name, sheetBodyEl, stale);
      if (stale()) { disposeEntry(entry); return; }
      state.sheetEntry = entry;
    }

    // Every one of these globals is grepped and assigned: delivery-panel.js:1713,
    // vault-pane.js:1928, memory-panel.js:414, project-management-panel.js:4531.
    // A missing one is a silent no-op in JavaScript, so each is checked before
    // it is called and the tab says so rather than rendering nothing.
    function surfaceFactory(name, host) {
      if (name === 'delivery') {
        if (typeof window.xnautCreateDeliveryPanel !== 'function') return null;
        // No select of its own: the switcher above the tabs is the workspace's
        // one project control, and Delivery's was the second (XNAUT-435).
        return window.xnautCreateDeliveryPanel(`${label}-delivery`, host, {
          project: state.projectKey,
          hideProjectSelect: true,
        });
      }
      if (name === 'memory') {
        if (typeof window.xnautCreateMemoryPanel !== 'function') return null;
        return window.xnautCreateMemoryPanel(`${label}-memory`, host, { project: state.projectKey });
      }
      if (name === 'vault') {
        if (typeof window.xnautCreateVaultPane !== 'function') return null;
        // The project's Docs ARE the work vault scoped to its folder, with the
        // chat off. Same call the PM panel's Docs tab makes
        // (project-management-panel.js:813).
        return window.xnautCreateVaultPane(`${label}-vault`, host, {
          vault: 'work',
          scopePrefix: vaultFolder(state.project),
          projectKey: state.projectKey,
          hideChat: true,
        });
      }
      const section = PM_SECTION[name];
      if (section) {
        if (typeof window.xnautCreateProjectManagementPanel !== 'function') return null;
        // Work, NAUT-Flow and the three-dot views are sections of the Projects
        // panel, so the panel is what renders them. Handed a project it draws
        // no chrome of its own (XNAUT-342): the tabs above and the sidebar
        // beside are the only copy of that choice on screen.
        // `pendingAction` is consumed here, once: it is what the caller asked
        // to DO on arrival, not a place, so it must not fire again when the
        // tab is revisited.
        const action = state.pendingAction;
        state.pendingAction = '';
        const ticket = state.pendingTicket;
        state.pendingTicket = '';
        return window.xnautCreateProjectManagementPanel(`${label}-${name}`, host, {
          project: state.projectKey,
          section,
          action,
          ticket,
        });
      }
      return null;
    }

    // ── Tabs ──────────────────────────────────────────────────────────────
    function renderTabs() {
      $('.wsp-tabs').innerHTML = TABS.map(([key, text]) =>
        `<button data-wsp-tab="${key}" class="${state.tab === key ? 'active' : ''}">${esc(text)}</button>`).join('');
      $('.wsp-tabs').querySelectorAll('[data-wsp-tab]').forEach((button) => {
        button.onclick = () => showTab(button.dataset.wspTab);
      });
    }

    function showTab(name) {
      // Choosing a tab dismisses the sheet: leaving it up would cover the tab
      // the strip says is active.
      closeSheet();
      state.tab = TAB_KEYS.has(name) ? name : 'code';
      renderTabs();
      const isCode = state.tab === 'code';
      codeEl.hidden = !isCode;
      surfaceEl.hidden = isCode;
      if (isCode) {
        // The Code view is never torn down, so the open files and the selected
        // file survive a trip through the other tabs.
        disposeSurface();
        try { codeEditor?.layout(); } catch (_error) { /* not mounted yet */ }
        return;
      }
      mountSurface(state.tab);
    }

    // One door for both kinds of destination, because a caller asking for
    // "settings" means the surface and does not know it is not a tab.
    function show(name) {
      if (name && !TAB_KEYS.has(name) && LABEL.has(name)) { openSheet(name); return; }
      showTab(name);
    }

    // ── The three-dot menu ────────────────────────────────────────────────
    $('.wsp-dots').onclick = (event) => {
      closeMenu();
      menuEl = document.createElement('div');
      menuEl.className = 'wsp-menu';
      for (const [key, text] of MENU) {
        const button = document.createElement('button');
        button.className = 'wsp-menu-item';
        button.dataset.wspMenu = key;
        button.textContent = text;
        button.onclick = () => { closeMenu(); openSheet(key); };
        menuEl.appendChild(button);
      }
      document.body.appendChild(menuEl);
      window.xnautPlaceAtClick(menuEl, event.clientX, event.clientY);
      document.addEventListener('mousedown', onMenuDismiss, true);
      document.addEventListener('keydown', onMenuKey, true);
    };

    $('.wsp-sheet-close').onclick = () => closeSheet();

    // ── Project resolution ────────────────────────────────────────────────
    function paintHead() {
      $('.wsp-name').textContent = (state.project && (state.project.name || state.project.key))
        || state.projectKey || 'No project';
      const dir = root();
      const suffix = state.worktree ? ` · ${baseOf(trimSlash(state.worktree))}` : '';
      $('.wsp-root').textContent = dir ? dir + suffix : 'no source path';
      $('.wsp-root').title = dir || '';
    }

    // ── The project switcher ──────────────────────────────────────────────
    // The front door (XNAUT-435). Until this, the sidebar's project tree was
    // the only thing that could change which project the workspace is showing,
    // and the Sessions list replaces that tree — so on a machine with the
    // Sessions list up there was no way to switch at all, and no way in except
    // by clicking a file in the file system.
    //
    // It is the same control the Delivery tab used to draw for its own list
    // (project-select.js, lifted out of delivery-panel.js), in the same place:
    // the top of the left column. Delivery no longer draws its own, so there is
    // one select and not two.
    function paintProjectSelect() {
      // Assigned in project-select.js:78, which index.html loads before this
      // file. Grepped and checked rather than assumed: an undefined global here
      // would throw inside setProject and leave the workspace half-painted.
      if (!window.xnautProjectSelect || typeof window.xnautProjectSelect.html !== 'function') {
        projbarEl.innerHTML = '';
        console.error('[workspace] xnautProjectSelect is not loaded; the project switcher cannot render');
        return;
      }
      projbarEl.innerHTML = window.xnautProjectSelect.html(state.projects, state.projectKey);
      window.xnautProjectSelect.bind(projbarEl, (key) => {
        if (!key || key === state.projectKey) return;
        switchProject(key);
      });
    }

    // Switching is done IN PLACE, on this pane, rather than by calling
    // xnautOpenWorkspace: the workspace is a singleton tab, so re-opening it
    // would route back to this same pane through updateOptions and do the same
    // work one indirection later. The tab is kept — a person reading Work stays
    // on Work — and the open files are not, because they are paths into the
    // previous project's checkout.
    async function switchProject(key) {
      const tab = state.tab;
      await setProject({ project: key });
      show(tab);
    }

    // Cmd+P (Ctrl+P elsewhere) reaches the switcher from anywhere in the
    // workspace, which is what André asked for before the control moved to the
    // left column. It FOCUSES the select rather than opening it: a native
    // <select> has no programmatic open, and faking one with a custom listbox
    // would be a second control that behaves almost like the first. From the
    // focus, the arrow keys and typing pick a project the way they do in any
    // other select on the page.
    function onWorkspaceKey(event) {
      if (event.key !== 'p' && event.key !== 'P') return;
      if (!(event.metaKey || event.ctrlKey) || event.altKey || event.shiftKey) return;
      // Only the workspace that is actually on screen answers: panes.get keeps
      // torn-down panes out, but a workspace behind another tab is still in the
      // document and must not steal the key.
      if (!pane.isConnected || !pane.getClientRects().length) return;
      const select = projbarEl.querySelector('.xps-select');
      if (!select) return;
      event.preventDefault();
      select.focus();
    }
    document.addEventListener('keydown', onWorkspaceKey);

    // ── The header's live numbers ─────────────────────────────────────────
    // The Overview tab is gone (XNAUT-342): a dashboard of links to the other
    // tabs is indirection, and what it was actually read for is four numbers.
    // They belong beside the project's name. Every one is measured on this
    // machine, and one that cannot be read stays "—" rather than becoming a
    // plausible zero.
    //
    // XNAUT-341: a dash on its own was the remaining dishonesty. It looks
    // identical whether the folder is not a repository, the command failed, or
    // the number simply has not arrived yet. Three different situations, one
    // of which is a bug and two of which are fine. So every dash this function
    // can produce now carries the reason beside it, and the reason is the thing
    // under test. The rule the ticket exists to enforce: a number on a screen
    // is read now, and a number that could not be read says why.
    const GIT_FACTS = ['lastcommit', 'changes', 'worktrees'];

    function ago(ms) {
      if (!ms) return '—';
      const mins = Math.round(Math.max(0, Date.now() - ms) / 60000);
      if (mins < 60) return `${mins}m ago`;
      const hrs = Math.round(mins / 60);
      return hrs < 48 ? `${hrs}h ago` : `${Math.round(hrs / 24)}d ago`;
    }

    async function loadFacts() {
      state.factsGeneration += 1;
      const generation = state.factsGeneration;
      const whyEl = pane.querySelector('[data-fact-why]');
      const set = (key, value) => {
        const el = pane.querySelector(`[data-fact="${key}"]`);
        if (el) el.textContent = value;
      };
      // The reason sits once beside the row rather than three times inside it,
      // because the same sentence repeated under each label is noise. It is
      // also put on each dash it explains, so hovering the dash answers the
      // question where it was asked.
      //
      // Reasons ACCUMULATE. Two of these can be true at once: a repository with
      // no commits whose worktree list also failed has two different absences,
      // and a chip that kept only the last one assigned would report the second
      // and quietly drop the first.
      const reasons = [];
      const because = (keys, reason) => {
        keys.forEach((key) => {
          const el = pane.querySelector(`[data-fact="${key}"]`);
          if (el) el.title = reason;
        });
        if (!whyEl || reasons.includes(reason)) return;
        reasons.push(reason);
        whyEl.textContent = reasons.join('; ');
        whyEl.title = reasons.join('; ');
        whyEl.hidden = false;
      };
      ['lastcommit', 'changes', 'worktrees', 'tickets'].forEach((key) => {
        set(key, '—');
        const el = pane.querySelector(`[data-fact="${key}"]`);
        if (el) el.removeAttribute('title');
      });
      if (whyEl) { whyEl.textContent = ''; whyEl.hidden = true; }
      if (!state.projectKey) {
        because(['lastcommit', 'changes', 'worktrees', 'tickets'], 'no project selected');
        return;
      }
      const key = state.projectKey.toUpperCase();
      try {
        const tickets = (await invoke('pm_ticket_list', { project: state.projectKey })) || [];
        if (generation !== state.factsGeneration) return;
        // The rows are counted rather than trusted: the command filters, and a
        // count that came from somewhere else would be a number about the wrong
        // project sitting under this project's name.
        set('tickets', String(tickets.filter((item) => String(item.project || '').toUpperCase() === key).length));
      } catch (_error) {
        if (generation !== state.factsGeneration) return;
        because(['tickets'], 'the ticket store could not be read');
      }
      const dir = root();
      if (!dir) {
        // Registered without a checkout on this machine. Nothing is broken and
        // there is nothing to fix; the row should say that rather than imply a
        // failure.
        because(GIT_FACTS, 'no checkout on this machine');
        return;
      }
      let facts = null;
      try {
        facts = await invoke('project_facts', { path: dir });
      } catch (_error) {
        if (generation !== state.factsGeneration) return;
        because(GIT_FACTS, 'the git facts could not be read');
        return;
      }
      if (generation !== state.factsGeneration) return;
      // A command that resolves nothing is not a repository that answered
      // "no"; it is the call itself having gone missing, and saying so is how
      // an unregistered command stops looking like a plain folder.
      if (!facts) {
        because(GIT_FACTS, 'the git facts could not be read');
        return;
      }
      if (!facts.is_repo) {
        because(GIT_FACTS, String(facts.unavailable || 'not a git repository'));
        return;
      }
      set('changes', facts.changes == null ? '—' : String(facts.changes));
      set('worktrees', facts.worktrees == null ? '—' : String(facts.worktrees));
      if (facts.changes == null || facts.worktrees == null) {
        because(GIT_FACTS.filter((k) => k !== 'lastcommit'), 'git answered only in part');
      }
      if (facts.last_commit_ms) {
        set('lastcommit', ago(facts.last_commit_ms));
      } else if (facts.no_commits) {
        // A real repository nobody has committed to yet. "never" is the answer;
        // a dash would read as the number having failed to load.
        set('lastcommit', 'never');
        because(['lastcommit'], 'this repository has no commits yet');
      } else {
        because(['lastcommit'], 'the last commit could not be read');
      }
    }

    async function setProject(next) {
      next = next || {};
      // The project arrives as an argument, never read from the sidebar: the
      // sidebar that will select it is XNAUT-335 and is being built in parallel.
      const given = next.project;
      if (given && typeof given === 'object') {
        state.project = given;
        state.projectKey = String(given.key || given.name || '');
      } else if (given) {
        state.projectKey = String(given);
        state.project = null;
      }
      if (typeof next.worktree === 'string') state.worktree = next.worktree;

      // Always asked for, even when the caller handed us the project object:
      // the switcher below the tabs lists every project, so the list is not an
      // optional lookup any more, it is what that control is made of.
      try {
        state.projects = (await invoke('pm_project_list')) || [];
      } catch (_error) {
        state.projects = [];
      }
      if (state.projectKey && !state.project) {
        const key = state.projectKey.toUpperCase();
        state.project = state.projects.find((item) => String(item.key || '').toUpperCase() === key)
          || state.projects.find((item) => String(item.name || '') === state.projectKey)
          || null;
        if (state.project && state.project.key) state.projectKey = state.project.key;
      }
      paintHead();
      paintProjectSelect();
      // A different project is a different sheet: what was open in it belonged
      // to the project that is no longer selected.
      closeSheet();
      // A different checkout is a different set of files: drop what was open
      // rather than leave tabs pointing into the previous worktree.
      disposeCodeEditor();
      state.open = [];
      state.active = '';
      state.files.clear();
      state.dirty.clear();
      renderFileTabs();
      viewEl.innerHTML = emptyCodeHtml();
      loadFacts();
      await loadTree();
      // A file asked for by whoever opened the workspace. Done after the tree
      // is rooted, because the tree is what the tab bar and the row highlight
      // are drawn against.
      if (next.file) openFile(String(next.file));
      // A sheet asked for by whoever opened the workspace, so the sidebar's
      // three-dot menu can land straight on Settings.
      if (next.action) state.pendingAction = String(next.action);
      // A ticket link (ticket-links.js) lands on the Work tab with it open.
      if (next.ticket) state.pendingTicket = String(next.ticket);
      if (next.sheet) openSheet(String(next.sheet));
    }

    await setProject(opts);
    show(opts.tab || 'code');

    const entry = {
      kind: 'workspace',
      label,
      pane,
      async updateOptions(next) {
        next = next || {};
        const changed = (next.project && next.project !== state.projectKey && next.project !== state.project)
          || (typeof next.worktree === 'string' && next.worktree !== state.worktree);
        if (changed) await setProject(next);
        // A file asked for when the project did not change: setProject never
        // ran, so nothing would have opened it.
        else if (next.file) openFile(String(next.file));
        if (next.action) state.pendingAction = String(next.action);
        if (!changed && next.sheet) openSheet(String(next.sheet));
        show(next.tab || state.tab);
      },
      destroy() {
        document.removeEventListener('keydown', onWorkspaceKey);
        themeObserver.disconnect();
        disposeCodeEditor();
        disposeSurface();
        closeSheet();
        closeMenu();
        if (pane.parentNode) pane.parentNode.removeChild(pane);
        panes.delete(label);
      },
    };
    entry.dispose = entry.destroy;
    panes.set(label, entry);
    return entry;
  }

  function destroyWorkspacePanel(label) {
    const entry = panes.get(label);
    if (entry) entry.destroy();
  }

  window.xnautCreateWorkspacePanel = createWorkspacePanel;
  window.xnautDestroyWorkspacePanel = destroyWorkspacePanel;

  // The one entry point. Takes its project and worktree as arguments so it can
  // be driven by the new sidebar (XNAUT-335), by another panel, or by hand from
  // the console: window.xnautOpenWorkspace({ project: 'XNAUT' }).
  // Open one file in the workspace's code view, working out which project it
  // belongs to from its path.
  //
  // Longest prefix by path component, the same rule the run registry uses to
  // attribute a run to a project (XNAUT-346), and load-bearing for the same
  // reason: one project's source_path is a prefix of nearly every other, so a
  // first match would send every file to it.
  window.xnautOpenFileInWorkspace = async function (path) {
    const file = String(path || '');
    if (!file) return false;
    let projects = [];
    try {
      projects = (await window.__TAURI__.core.invoke('pm_project_list')) || [];
    } catch (_error) {
      projects = [];
    }
    const parts = file.split('/').filter(Boolean);
    let best = null;
    let bestDepth = -1;
    projects.forEach((project) => {
      const root = String(project.source_path || '').replace(/\/+$/, '');
      if (!root) return;
      const rootParts = root.split('/').filter(Boolean);
      if (rootParts.length > parts.length) return;
      if (rootParts.some((part, i) => part !== parts[i])) return;
      if (rootParts.length > bestDepth) { best = project; bestDepth = rootParts.length; }
    });
    if (!best) return false;
    // The file may sit in a worktree beside the checkout rather than in it;
    // root the tree at the directory the file is actually under.
    window.xnautOpenWorkspace({ project: best.key || best.name, tab: 'code', file });
    return true;
  };

  window.xnautOpenWorkspace = function (opts) {
    opts = opts || {};
    if (window.xnautHomeContext) window.xnautHomeContext();
    return window.xnautAttachSingletonPanelTab('Workspace', 'xnautCreateWorkspacePanel', opts);
  };
})();
