// Right-pane Git view — v1.6 source control panel modeled on Orca.
//
// Architecture mirrors markdown-pane.js: IIFE + 'use strict', window.xnaut*
// exports, inline SVG icons, CSS vars with dark fallbacks. Registers itself
// as right-pane view 'git' via window.xnautRightPaneRegisterView, or queues
// the registration on window.__xnautRightPaneQueue if the host isn't up yet.
(function () {
  'use strict';

  const invoke = (...a) => window.__TAURI__.core.invoke(...a);

  function escapeText(s) {
    return String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
  }
  const basename = (p) => String(p).replace(/\/+$/, '').split('/').pop() || '';

  // ---- inline SVG icons -------------------------------------------------
  const ICONS = {
    refresh: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4"><path d="M13.5 8a5.5 5.5 0 1 1-1.6-3.9"/><path d="M13.5 1.5v3h-3"/></svg>',
    wand: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4"><path d="M10.5 5.5L2.5 13.5"/><path d="M10.5 5.5l2-2"/><path d="M12 1.5v1.5M14.75 4.25h-1.5M13.6 1.9l-1 1M13.6 6.6l-1-1"/></svg>',
    chevron: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6"><path d="M4 6l4 4 4-4"/></svg>',
    plus: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6"><path d="M8 3v10M3 8h10"/></svg>',
    minus: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6"><path d="M3 8h10"/></svg>',
    up: '<span class="rpg-arrow">&#8593;</span>',
    down: '<span class="rpg-arrow">&#8595;</span>',
  };

  // ---- styles -----------------------------------------------------------
  function injectStyles() {
    if (document.getElementById('right-pane-git-styles')) return;
    const st = document.createElement('style');
    st.id = 'right-pane-git-styles';
    st.textContent = `
      .rpg-root{display:flex;flex-direction:column;height:100%;min-height:0;overflow:hidden;font-size:12px;color:var(--foreground,#ddd);background:var(--card,#171717)}
      .rpg-tabs{display:flex;gap:2px;padding:6px 8px 0}
      .rpg-tab{flex:1;padding:5px 0;text-align:center;border:none;background:transparent;color:var(--muted-foreground,#a1a1a1);cursor:pointer;border-bottom:2px solid transparent;font:inherit}
      .rpg-tab.active{color:var(--foreground,#fafafa);border-bottom-color:var(--primary,#e5e5e5)}
      .rpg-status{display:flex;align-items:center;gap:8px;padding:8px 10px;border-bottom:1px solid var(--border,rgba(255,255,255,.07))}
      .rpg-ahead{color:var(--state-success,#10b981)} .rpg-behind{color:var(--state-warning,#f59e0b)}
      .rpg-branch{font-family:monospace;color:var(--muted-foreground,#a1a1a1);overflow:hidden;text-overflow:ellipsis;white-space:nowrap;flex:1}
      .rpg-iconbtn{display:inline-flex;align-items:center;justify-content:center;width:22px;height:22px;border:none;background:transparent;color:var(--muted-foreground,#a1a1a1);cursor:pointer;border-radius:4px;padding:0}
      .rpg-iconbtn:hover{background:var(--accent,#404040);color:var(--foreground,#fafafa)}
      .rpg-iconbtn svg{width:14px;height:14px}
      .rpg-iconbtn:disabled{opacity:.5;cursor:default}
      .rpg-spin svg{animation:rpg-spin 1s linear infinite}
      @keyframes rpg-spin{to{transform:rotate(360deg)}}
      .rpg-msgwrap{position:relative;padding:8px 10px 0}
      .rpg-msg{width:100%;min-height:44px;resize:vertical;background:var(--editor-surface,#1e1e1e);color:var(--foreground,#ddd);border:1px solid var(--border,rgba(255,255,255,.07));border-radius:6px;padding:6px 28px 6px 8px;font:inherit;box-sizing:border-box}
      .rpg-wand{position:absolute;top:12px;right:14px}
      .rpg-actions{display:flex;gap:6px;padding:8px 10px;border-bottom:1px solid var(--border,rgba(255,255,255,.07))}
      .rpg-btn{padding:5px 12px;border:none;border-radius:6px;background:var(--secondary,#262626);color:var(--foreground,#fafafa);cursor:pointer;font:inherit}
      .rpg-btn:hover:not(:disabled){background:var(--accent,#404040)}
      .rpg-btn:disabled{opacity:.45;cursor:default}
      .rpg-btn.primary{background:var(--sidebar-primary,#1447e6);color:#fff}
      .rpg-split{display:flex;margin-left:auto}
      .rpg-split .rpg-btn{border-radius:6px 0 0 6px}
      .rpg-split .rpg-chev{border-radius:0 6px 6px 0;border-left:1px solid var(--border,rgba(255,255,255,.2));padding:5px 4px;display:inline-flex;align-items:center}
      .rpg-split .rpg-chev svg{width:12px;height:12px}
      .rpg-menu{position:absolute;z-index:60;background:var(--popover,#171717);border:1px solid var(--border,rgba(255,255,255,.1));border-radius:6px;padding:4px;box-shadow:0 6px 20px rgba(0,0,0,.5);min-width:180px}
      .rpg-menu button{display:block;width:100%;text-align:left;padding:6px 10px;border:none;background:transparent;color:var(--foreground,#fafafa);cursor:pointer;border-radius:4px;font:inherit}
      .rpg-menu button:hover{background:var(--accent,#404040)}
      .rpg-scroll{flex:1;min-height:0;overflow-y:auto}
      .rpg-sechead{display:flex;align-items:center;gap:6px;padding:10px 10px 4px;color:var(--muted-foreground,#a1a1a1);font-size:10px;letter-spacing:.08em;text-transform:uppercase}
      .rpg-filter{margin:2px 10px 6px;padding:4px 8px;width:calc(100% - 20px);box-sizing:border-box;background:var(--editor-surface,#1e1e1e);border:1px solid var(--border,rgba(255,255,255,.07));border-radius:5px;color:var(--foreground,#ddd);font:inherit}
      .rpg-file{display:flex;align-items:center;gap:6px;padding:3px 10px;cursor:pointer;min-width:0}
      .rpg-file:hover{background:var(--accent,#404040)}
      .rpg-chip{width:14px;height:14px;border-radius:3px;display:inline-flex;align-items:center;justify-content:center;font-size:9px;font-weight:700;flex:0 0 auto;color:#111}
      .rpg-path{flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-family:monospace}
      .rpg-dir{color:var(--muted-foreground,#888)} .rpg-base{color:var(--foreground,#eee)}
      .rpg-adds{color:var(--git-added,#81b88b);flex:0 0 auto} .rpg-dels{color:var(--git-deleted,#c74e39);flex:0 0 auto}
      .rpg-stagebtns{display:flex;gap:2px;flex:0 0 auto;visibility:hidden}
      .rpg-file:hover .rpg-stagebtns{visibility:visible}
      .rpg-commit{display:flex;gap:8px;padding:0 10px;position:relative}
      .rpg-rail{display:flex;flex-direction:column;align-items:center;flex:0 0 10px}
      .rpg-dot{width:8px;height:8px;border-radius:50%;background:var(--muted-foreground,#a1a1a1);flex:0 0 auto;margin-top:5px}
      .rpg-line{width:1px;flex:1;background:var(--border,rgba(255,255,255,.12))}
      .rpg-cbody{padding:2px 0 10px;min-width:0;flex:1}
      .rpg-csub{color:var(--foreground,#eee);overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
      .rpg-cmeta{display:flex;gap:8px;align-items:center;color:var(--muted-foreground,#888);font-size:11px;margin-top:2px;flex-wrap:wrap}
      .rpg-sha{font-family:monospace}
      .rpg-refs{background:var(--secondary,#262626);border:1px solid var(--border,rgba(255,255,255,.1));border-radius:10px;padding:0 7px;font-size:10px}
      .rpg-empty{padding:24px 12px;text-align:center;color:var(--muted-foreground,#888)}
      .rpg-toasts{position:absolute;bottom:10px;left:10px;right:10px;display:flex;flex-direction:column;gap:6px;z-index:70;pointer-events:none}
      .rpg-toast{pointer-events:auto;background:var(--popover,#222);border:1px solid var(--border,rgba(255,255,255,.12));border-radius:6px;padding:7px 10px;box-shadow:0 4px 16px rgba(0,0,0,.5);word-break:break-word}
      .rpg-toast.err{border-color:var(--destructive,#ff6568);color:var(--destructive,#ff6568)}
      .rpg-toast a{color:var(--state-info,#6ea8ff)}
      .rpg-overlay{position:fixed;inset:0;background:rgba(0,0,0,.5);z-index:80;display:flex;align-items:center;justify-content:center}
      .rpg-dialog{background:var(--card,#171717);border:1px solid var(--border,rgba(255,255,255,.1));border-radius:8px;padding:14px;width:380px;display:flex;flex-direction:column;gap:8px}
      .rpg-dialog label{color:var(--muted-foreground,#a1a1a1);font-size:11px}
      .rpg-dialog input,.rpg-dialog select,.rpg-dialog textarea{width:100%;box-sizing:border-box;background:var(--editor-surface,#1e1e1e);color:var(--foreground,#ddd);border:1px solid var(--border,rgba(255,255,255,.07));border-radius:5px;padding:5px 8px;font:inherit}
    `;
    document.head.appendChild(st);
  }

  const STATUS_COLOR = {
    A: 'var(--git-added,#81b88b)', M: 'var(--git-modified,#e2c08d)',
    D: 'var(--git-deleted,#c74e39)', R: 'var(--git-renamed,#73c991)', '?': '#9a9a9a',
  };

  // ---- state ------------------------------------------------------------
  let rootEl = null;          // container given by host
  let repo = '';              // current repo root path
  let tab = 'all';            // 'all' | 'uncommitted'
  let aiAvailable = false;
  let aheadBehind = null;     // {ahead, behind, branch, upstream}
  let files = [];             // current tab's file list
  let commits = [];
  let filterText = '';
  let menuEl = null;
  let dialogEl = null;

  // ---- toasts -----------------------------------------------------------
  function toast(msgOrNode, isErr) {
    if (!rootEl) return;
    const box = rootEl.querySelector('.rpg-toasts');
    if (!box) return;
    const t = document.createElement('div');
    t.className = 'rpg-toast' + (isErr ? ' err' : '');
    if (typeof msgOrNode === 'string') t.textContent = msgOrNode.length > 200 ? msgOrNode.slice(0, 200) + '…' : msgOrNode;
    else t.appendChild(msgOrNode);
    box.appendChild(t);
    setTimeout(() => t.remove(), 6000);
  }

  // ---- data loading -----------------------------------------------------
  async function refresh() {
    if (!rootEl) return;
    if (!repo) { renderEmpty(); return; }
    try {
      aheadBehind = await invoke('git_ahead_behind', { repo });
    } catch (e) {
      aheadBehind = null;
      renderEmpty('Not a git repository (' + escapeText(String(e)).slice(0, 120) + ')');
      return;
    }
    try {
      if (tab === 'all') {
        [files, commits] = await Promise.all([
          invoke('git_outgoing_files', { repo }),
          invoke('git_outgoing_commits', { repo, limit: 50 }),
        ]);
      } else {
        files = await invoke('git_uncommitted_files', { repo });
        commits = [];
      }
    } catch (e) {
      files = []; commits = [];
      toast('git refresh failed: ' + String(e), true);
    }
    render();
  }

  // ---- actions ----------------------------------------------------------
  async function doCommit() {
    const ta = rootEl.querySelector('.rpg-msg');
    const msg = ta ? ta.value.trim() : '';
    if (!msg) return;
    try {
      const sha = await invoke('git_commit', { repo, message: msg });
      if (ta) ta.value = '';
      toast('Committed ' + String(sha).slice(0, 8));
      refresh();
    } catch (e) { toast('Commit failed: ' + String(e), true); }
  }

  async function doPush(forceWithLease) {
    try {
      const out = await invoke('git_push', { repo, forceWithLease: !!forceWithLease });
      toast(String(out || 'Pushed.'));
      refresh();
    } catch (e) { toast('Push failed: ' + String(e), true); throw e; }
  }

  async function doAiMessage(btn) {
    const ta = rootEl.querySelector('.rpg-msg');
    btn.disabled = true; btn.classList.add('rpg-spin');
    try {
      const msg = await invoke('git_ai_commit_message', { repo });
      if (ta) ta.value = String(msg || '');
      if (ta) ta.dispatchEvent(new Event('input'));
    } catch (e) { toast('AI message failed: ' + String(e), true); }
    btn.disabled = false; btn.classList.remove('rpg-spin');
  }

  async function doStage(path, stage) {
    try {
      await invoke(stage ? 'git_stage' : 'git_unstage', { repo, path });
      refresh();
    } catch (e) { toast((stage ? 'Stage' : 'Unstage') + ' failed: ' + String(e), true); }
  }

  // ---- push dropdown menu -----------------------------------------------
  function closeMenu() { if (menuEl) { menuEl.remove(); menuEl = null; } }
  function openPushMenu(anchor) {
    closeMenu();
    menuEl = document.createElement('div');
    menuEl.className = 'rpg-menu';
    const items = [
      ['Push', () => doPush(false).catch(() => {})],
      ['Force Push (with lease)', () => doPush(true).catch(() => {})],
      ['Push & Create PR…', async () => { try { await doPush(false); openPrDialog(); } catch (_) {} }],
      ['Create PR…', () => openPrDialog()],
    ];
    for (const [label, fn] of items) {
      const b = document.createElement('button');
      b.textContent = label;
      b.onclick = () => { closeMenu(); fn(); };
      menuEl.appendChild(b);
    }
    document.body.appendChild(menuEl);
    const r = anchor.getBoundingClientRect();
    menuEl.style.top = (r.bottom + 4) + 'px';
    menuEl.style.left = Math.max(8, r.right - menuEl.offsetWidth) + 'px';
    setTimeout(() => document.addEventListener('mousedown', onDocDown, { once: true }), 0);
    function onDocDown(e) { if (menuEl && !menuEl.contains(e.target)) closeMenu(); }
  }

  // ---- PR dialog (singleton overlay) -------------------------------------
  function closePrDialog() { if (dialogEl) { dialogEl.remove(); dialogEl = null; } }
  async function openPrDialog() {
    closePrDialog();
    let hosts = [];
    try { hosts = await invoke('forge_hosts'); } catch (e) { toast('forge_hosts failed: ' + String(e), true); return; }
    let prefillTitle = '';
    try {
      const last = await invoke('git_outgoing_commits', { repo, limit: 1 });
      if (last && last[0]) prefillTitle = last[0].subject || '';
    } catch (_) { /* prefill is best-effort */ }

    dialogEl = document.createElement('div');
    dialogEl.className = 'rpg-overlay';
    dialogEl.innerHTML = `
      <div class="rpg-dialog">
        <div style="font-weight:600">Create Pull Request</div>
        <label>Forge</label>
        <select class="rpg-pr-forge">${hosts.map((h, i) =>
          `<option value="${i}">${escapeText(h.kind)} — ${escapeText(h.owner)} (${escapeText(h.base_url)})</option>`).join('')}</select>
        <label>Base branch</label>
        <input class="rpg-pr-base" value="development">
        <label>Title</label>
        <input class="rpg-pr-title" value="${escapeText(prefillTitle)}">
        <label>Body</label>
        <textarea class="rpg-pr-body" rows="4"></textarea>
        <div style="display:flex;gap:6px;justify-content:flex-end">
          <button class="rpg-btn rpg-pr-cancel">Cancel</button>
          <button class="rpg-btn primary rpg-pr-create">Create PR</button>
        </div>
      </div>`;
    dialogEl.addEventListener('mousedown', (e) => { if (e.target === dialogEl) closePrDialog(); });
    dialogEl.querySelector('.rpg-pr-cancel').onclick = closePrDialog;
    dialogEl.querySelector('.rpg-pr-create').onclick = async (e) => {
      const btn = e.currentTarget;
      btn.disabled = true;
      const forgeIndex = Number(dialogEl.querySelector('.rpg-pr-forge').value || 0);
      const base = dialogEl.querySelector('.rpg-pr-base').value.trim() || 'development';
      const title = dialogEl.querySelector('.rpg-pr-title').value.trim();
      const body = dialogEl.querySelector('.rpg-pr-body').value;
      const head = (aheadBehind && aheadBehind.branch) || '';
      try {
        const url = await invoke('forge_create_pr', { forgeIndex, repo: basename(repo), head, base, title, body });
        closePrDialog();
        try { await navigator.clipboard.writeText(String(url)); } catch (_) {}
        const node = document.createElement('span');
        node.innerHTML = `PR created (copied): <a href="${escapeText(String(url))}" target="_blank">${escapeText(String(url))}</a>`;
        toast(node);
      } catch (err) {
        btn.disabled = false;
        toast('Create PR failed: ' + String(err), true);
      }
    };
    document.body.appendChild(dialogEl);
  }

  // ---- rendering ----------------------------------------------------------
  function renderEmpty(msg) {
    if (!rootEl) return;
    rootEl.innerHTML = `<div class="rpg-root"><div class="rpg-empty">${escapeText(msg || 'No folder open.')}</div><div class="rpg-toasts"></div></div>`;
  }

  function pathHtml(p) {
    const i = p.lastIndexOf('/');
    if (i < 0) return `<span class="rpg-base">${escapeText(p)}</span>`;
    return `<span class="rpg-dir">${escapeText(p.slice(0, i + 1))}</span><span class="rpg-base">${escapeText(p.slice(i + 1))}</span>`;
  }

  function render() {
    if (!rootEl) return;
    const ab = aheadBehind || { ahead: 0, behind: 0, branch: '', upstream: '' };
    const prevMsg = rootEl.querySelector('.rpg-msg');
    const msgVal = prevMsg ? prevMsg.value : '';
    const isAll = tab === 'all';
    const shown = files.filter((f) => !filterText || f.path.toLowerCase().includes(filterText.toLowerCase()));

    rootEl.innerHTML = `
      <div class="rpg-root">
        <div class="rpg-tabs">
          <button class="rpg-tab ${isAll ? 'active' : ''}" data-tab="all">All</button>
          <button class="rpg-tab ${!isAll ? 'active' : ''}" data-tab="uncommitted">Uncommitted</button>
        </div>
        <div class="rpg-status">
          <span class="rpg-ahead">${ICONS.up} ${ab.ahead} ahead</span>
          ${ab.behind > 0 ? `<span class="rpg-behind">${ICONS.down} ${ab.behind} behind</span>` : ''}
          <span class="rpg-branch" title="${escapeText(ab.upstream || '')}">${escapeText(ab.branch || '')}</span>
          <button class="rpg-iconbtn rpg-refresh" title="Refresh">${ICONS.refresh}</button>
        </div>
        <div class="rpg-msgwrap">
          <textarea class="rpg-msg" rows="2" placeholder="Commit message"></textarea>
          <button class="rpg-iconbtn rpg-wand" title="Generate commit message" ${aiAvailable ? '' : 'hidden'}>${ICONS.wand}</button>
        </div>
        <div class="rpg-actions">
          <button class="rpg-btn primary rpg-do-commit" disabled>Commit</button>
          <span class="rpg-split">
            <button class="rpg-btn rpg-do-push">Push</button>
            <button class="rpg-btn rpg-chev" title="Push options">${ICONS.chevron}</button>
          </span>
        </div>
        <div class="rpg-scroll">
          <div class="rpg-sechead">${isAll ? `COMMITTED ON BRANCH ${shown.length}` : `CHANGES ${shown.length}`}</div>
          <input class="rpg-filter" placeholder="Filter files…" value="${escapeText(filterText)}">
          <div class="rpg-files"></div>
          ${isAll ? `<div class="rpg-sechead">COMMITS</div><div class="rpg-commits"></div>` : ''}
        </div>
        <div class="rpg-toasts"></div>
      </div>`;
    rootEl.firstElementChild.style.position = 'relative';

    // tabs
    rootEl.querySelectorAll('.rpg-tab').forEach((b) => {
      b.onclick = () => { tab = b.dataset.tab; filterText = ''; refresh(); };
    });
    rootEl.querySelector('.rpg-refresh').onclick = () => refresh();

    // commit box
    const ta = rootEl.querySelector('.rpg-msg');
    ta.value = msgVal;
    const commitBtn = rootEl.querySelector('.rpg-do-commit');
    const syncCommitBtn = () => { commitBtn.disabled = !ta.value.trim(); };
    ta.addEventListener('input', syncCommitBtn);
    syncCommitBtn();
    commitBtn.onclick = doCommit;
    const wand = rootEl.querySelector('.rpg-wand');
    if (wand) wand.onclick = () => doAiMessage(wand);

    // push split-button
    rootEl.querySelector('.rpg-do-push').onclick = () => doPush(false).catch(() => {});
    const chev = rootEl.querySelector('.rpg-chev');
    chev.onclick = () => openPushMenu(chev);

    // filter
    const filterEl = rootEl.querySelector('.rpg-filter');
    filterEl.oninput = () => { filterText = filterEl.value; renderFiles(); };
    filterEl.value = filterText;

    renderFiles();
    if (isAll) renderCommits();
  }

  function renderFiles() {
    const box = rootEl && rootEl.querySelector('.rpg-files');
    if (!box) return;
    const isAll = tab === 'all';
    const shown = files.filter((f) => !filterText || f.path.toLowerCase().includes(filterText.toLowerCase()));
    const head = rootEl.querySelector('.rpg-sechead');
    if (head) head.textContent = isAll ? `COMMITTED ON BRANCH ${shown.length}` : `CHANGES ${shown.length}`;
    box.innerHTML = '';
    if (!shown.length) { box.innerHTML = '<div class="rpg-empty">No files.</div>'; return; }
    for (const f of shown) {
      const row = document.createElement('div');
      row.className = 'rpg-file';
      const color = STATUS_COLOR[f.status] || STATUS_COLOR['?'];
      row.innerHTML = `
        <span class="rpg-chip" style="background:${color}">${escapeText(f.status)}</span>
        <span class="rpg-path" title="${escapeText(f.path)}">${pathHtml(f.path)}</span>
        <span class="rpg-adds">+${f.additions}</span><span class="rpg-dels">−${f.deletions}</span>
        ${isAll ? '' : `<span class="rpg-stagebtns">
          <button class="rpg-iconbtn rpg-st" data-act="stage" title="Stage">${ICONS.plus}</button>
          <button class="rpg-iconbtn rpg-st" data-act="unstage" title="Unstage">${ICONS.minus}</button>
        </span>`}`;
      row.onclick = (e) => {
        if (e.target.closest('.rpg-st')) return;
        if (window.xnautOpenDiff) {
          window.xnautOpenDiff({ repo, path: f.path, staged: false, outgoing: isAll, title: f.path });
        } else {
          toast('Diff viewer not loaded.', true);
        }
      };
      row.querySelectorAll('.rpg-st').forEach((b) => {
        b.onclick = (e) => { e.stopPropagation(); doStage(f.path, b.dataset.act === 'stage'); };
      });
      box.appendChild(row);
    }
  }

  function renderCommits() {
    const box = rootEl && rootEl.querySelector('.rpg-commits');
    if (!box) return;
    box.innerHTML = '';
    if (!commits.length) { box.innerHTML = '<div class="rpg-empty">No outgoing commits.</div>'; return; }
    commits.forEach((c, i) => {
      const item = document.createElement('div');
      item.className = 'rpg-commit';
      item.innerHTML = `
        <span class="rpg-rail"><span class="rpg-dot"></span>${i < commits.length - 1 ? '<span class="rpg-line"></span>' : ''}</span>
        <span class="rpg-cbody">
          <div class="rpg-csub" title="${escapeText(c.subject)}">${escapeText(c.subject)}</div>
          <div class="rpg-cmeta">
            <span>${escapeText(c.author)}</span><span>${escapeText(c.date)}</span>
            <span class="rpg-sha">${escapeText(c.short_sha)}</span>
            ${c.refs ? `<span class="rpg-refs">${escapeText(c.refs)}</span>` : ''}
          </div>
        </span>`;
      box.appendChild(item);
    });
  }

  // ---- right-pane view contract -------------------------------------------
  const view = {
    mount(container) {
      injectStyles();
      rootEl = container;
      invoke('chat_check_endpoint').then((ok) => {
        aiAvailable = !!ok;
        const w = rootEl && rootEl.querySelector('.rpg-wand');
        if (w) w.hidden = !aiAvailable;
      }).catch(() => { aiAvailable = false; });
      if (repo) refresh(); else renderEmpty();
    },
    setRoot(root) {
      repo = root || '';
      if (rootEl) { if (repo) refresh(); else renderEmpty(); }
    },
    destroy() {
      closeMenu(); closePrDialog();
      if (rootEl) rootEl.innerHTML = '';
      rootEl = null;
    },
  };

  function register() {
    window.xnautRightPaneRegisterView('git', view);
  }
  if (typeof window.xnautRightPaneRegisterView === 'function') {
    register();
  } else {
    (window.__xnautRightPaneQueue = window.__xnautRightPaneQueue || []).push(register);
  }
})();
