// Create Worktree modal — v1.6, modeled on Orca's worktree creator.
//
// Singleton overlay: built on first xnautOpenWorktreeModal() call, reused
// after. Same architecture as markdown-pane.js / tasks-panel.js: IIFE +
// 'use strict', window.xnaut* export, scoped <style>, escape helper.
//
// Source tabs: Smart and Forge URL actually create (via
// scaffold_task_from_issue). Branch and Name are forward-stubs for v1.6.1
// — they render but disable Create with a muted note.
(function () {
  'use strict';

  const invoke = (...a) => window.__TAURI__.core.invoke(...a);

  function escapeText(s) {
    return String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
  }

  // owner / repo / number from common issue+PR URL shapes:
  //   /issues/N  /pulls/N  /pull/N  /-/issues/N  /-/merge_requests/N
  function parseForgeUrl(text) {
    const m = String(text || '').trim()
      .match(/^https?:\/\/[^/]+\/(.+?)\/([^/]+?)\/(?:-\/)?(?:issues|pulls|pull|merge_requests)\/(\d+)\b/);
    if (!m) return null;
    return { owner: m[1], repo: m[2], number: parseInt(m[3], 10) };
  }

  function injectStyles() {
    if (document.getElementById('worktree-modal-styles')) return;
    const st = document.createElement('style');
    st.id = 'worktree-modal-styles';
    st.textContent = `
.wtm-overlay { position:fixed; inset:0; z-index:1000; display:flex; align-items:center; justify-content:center; background:rgba(0,0,0,.55); }
.wtm-overlay[hidden] { display:none !important; }
.wtm-card { display:flex; flex-direction:column; width:92%; max-width:560px; max-height:85vh; overflow:auto; background:var(--editor-surface, #1b1d23); color:var(--text, #d7dae0); border:1px solid var(--border, rgba(255,255,255,.1)); border-radius:var(--radius-md, 8px); box-shadow:0 18px 50px rgba(0,0,0,.5); font-size:13px; }
.wtm-header { display:flex; align-items:center; justify-content:space-between; padding:14px 16px 8px; }
.wtm-title { font-size:15px; font-weight:600; }
.wtm-close { background:transparent; border:none; color:var(--text-muted, #8a8f98); cursor:pointer; padding:2px; display:flex; }
.wtm-close svg { width:14px; height:14px; }
.wtm-close:hover { color:var(--text, #e8eaf0); }
.wtm-body { display:flex; flex-direction:column; gap:6px; padding:4px 16px 12px; }
.wtm-label { font-size:11px; font-weight:600; text-transform:uppercase; letter-spacing:.05em; color:var(--text-muted, #8a8f98); padding-top:8px; }
.wtm-body select, .wtm-body input[type=text] { width:100%; box-sizing:border-box; background:var(--input-bg, rgba(255,255,255,.05)); border:1px solid var(--border, rgba(255,255,255,.12)); border-radius:var(--radius-md, 6px); color:inherit; padding:6px 9px; font:inherit; outline:none; }
.wtm-body select:focus, .wtm-body input[type=text]:focus { border-color:var(--accent, #4f8cff); }
.wtm-tabs { display:flex; gap:2px; border:1px solid var(--border, rgba(255,255,255,.12)); border-radius:var(--radius-md, 6px); padding:2px; }
.wtm-tab { flex:1 1 0; background:transparent; border:none; color:var(--text-muted, #8a8f98); padding:4px 8px; cursor:pointer; font:inherit; font-size:12px; border-radius:4px; }
.wtm-tab.active { background:var(--accent-bg, rgba(79,140,255,.18)); color:var(--text, #e8eaf0); }
.wtm-src { display:flex; flex-direction:column; gap:6px; padding-top:2px; }
.wtm-chip { display:inline-block; max-width:100%; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; align-self:flex-start; font-size:12px; padding:3px 10px; border-radius:999px; border:1px solid var(--accent, #4f8cff); color:var(--text, #e8eaf0); background:var(--accent-bg, rgba(79,140,255,.12)); }
.wtm-chip.wtm-chip-muted { border-color:var(--border, rgba(255,255,255,.16)); color:var(--text-muted, #8a8f98); background:transparent; }
.wtm-note { font-size:12px; color:var(--text-muted, #8a8f98); font-style:italic; }
.wtm-advanced { padding-top:8px; }
.wtm-advanced summary { cursor:pointer; font-size:12px; color:var(--text-muted, #8a8f98); }
.wtm-footer { display:flex; align-items:center; gap:8px; padding:10px 16px 14px; border-top:1px solid var(--border, rgba(255,255,255,.08)); }
.wtm-error { flex:1 1 auto; min-width:0; font-size:12px; color:#f85149; white-space:pre-wrap; }
.wtm-spacer { flex:1 1 auto; }
.wtm-btn { background:transparent; border:1px solid var(--border, rgba(255,255,255,.16)); border-radius:var(--radius-md, 6px); color:var(--text, #d7dae0); padding:6px 13px; font:inherit; cursor:pointer; white-space:nowrap; }
.wtm-btn:hover { border-color:var(--text-muted, #8a8f98); }
.wtm-create { background:var(--accent, #4f8cff); border-color:var(--accent, #4f8cff); color:var(--accent-foreground,#fff); font-weight:600; }
.wtm-create:disabled { opacity:.45; cursor:default; }
.wtm-kbd { opacity:.7; font-weight:400; padding-left:4px; }
`;
    document.head.appendChild(st);
  }

  let overlay = null;
  const ui = {};
  const state = {
    ctx: null, projects: [], agents: [],
    source: 'smart', parsed: null, creating: false, branchesFor: null,
  };

  function buildModal() {
    if (overlay) return;
    injectStyles();
    overlay = document.createElement('div');
    overlay.className = 'wtm-overlay';
    overlay.hidden = true;
    overlay.innerHTML = `
      <div class="wtm-card" role="dialog" aria-modal="true" aria-label="Create Worktree">
        <div class="wtm-header">
          <span class="wtm-title">Create Worktree</span>
          <button class="wtm-close" aria-label="Close">
            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><line x1="4" y1="4" x2="12" y2="12"/><line x1="12" y1="4" x2="4" y2="12"/></svg>
          </button>
        </div>
        <div class="wtm-body">
          <label class="wtm-label">Project</label>
          <select class="wtm-project"></select>
          <input class="wtm-project-path" type="text" placeholder="/absolute/path/to/repo" spellcheck="false" hidden>

          <label class="wtm-label">Source</label>
          <div class="wtm-tabs">
            <button class="wtm-tab" data-src="smart">Smart</button>
            <button class="wtm-tab" data-src="url">Forge URL</button>
            <button class="wtm-tab" data-src="branch">Branch</button>
            <button class="wtm-tab" data-src="name">Name</button>
          </div>
          <div class="wtm-src" data-src-pane="smart"><span class="wtm-chip wtm-smart-chip"></span></div>
          <div class="wtm-src" data-src-pane="url" hidden>
            <input class="wtm-url-input" type="text" placeholder="https://forge…/owner/repo/issues/123" spellcheck="false">
            <span class="wtm-chip wtm-url-chip" hidden></span>
          </div>
          <div class="wtm-src" data-src-pane="branch" hidden>
            <select class="wtm-branch"></select>
            <div class="wtm-note">checkout of existing branches lands in v1.6.1</div>
          </div>
          <div class="wtm-src" data-src-pane="name" hidden>
            <input class="wtm-name-input" type="text" placeholder="task name" spellcheck="false">
            <div class="wtm-note">blank worktrees land in v1.6.1</div>
          </div>

          <label class="wtm-label">Agent</label>
          <select class="wtm-agent"></select>

          <details class="wtm-advanced">
            <summary>Advanced</summary>
            <label class="wtm-label">Base branch</label>
            <input class="wtm-base" type="text" value="development" spellcheck="false">
          </details>
        </div>
        <div class="wtm-footer">
          <span class="wtm-error" hidden></span>
          <span class="wtm-spacer"></span>
          <button class="wtm-btn wtm-cancel">Cancel</button>
          <button class="wtm-btn wtm-create">Create Worktree <span class="wtm-kbd">⌘↵</span></button>
        </div>
      </div>`;
    document.body.appendChild(overlay);

    const q = (sel) => overlay.querySelector(sel);
    ui.project = q('.wtm-project');
    ui.projectPath = q('.wtm-project-path');
    ui.smartChip = q('.wtm-smart-chip');
    ui.urlInput = q('.wtm-url-input');
    ui.urlChip = q('.wtm-url-chip');
    ui.branch = q('.wtm-branch');
    ui.nameInput = q('.wtm-name-input');
    ui.agent = q('.wtm-agent');
    ui.base = q('.wtm-base');
    ui.error = q('.wtm-error');
    ui.create = q('.wtm-create');
    ui.createLabel = ui.create.innerHTML;

    overlay.addEventListener('mousedown', (e) => { if (e.target === overlay) close(); });
    q('.wtm-close').onclick = close;
    q('.wtm-cancel').onclick = close;
    ui.create.onclick = create;

    overlay.querySelectorAll('.wtm-tab').forEach((b) => {
      b.onclick = () => setSource(b.dataset.src);
    });
    ui.urlInput.addEventListener('input', () => {
      state.parsed = parseForgeUrl(ui.urlInput.value);
      ui.urlChip.hidden = !state.parsed;
      if (state.parsed) {
        ui.urlChip.textContent = `#${state.parsed.number} ${state.parsed.owner}/${state.parsed.repo}`;
      }
      updateCreate();
    });
    ui.project.addEventListener('change', () => {
      ui.projectPath.hidden = ui.project.value !== '__manual__';
      if (state.source === 'branch') loadBranches();
      updateCreate();
    });
    ui.projectPath.addEventListener('input', updateCreate);
    ui.agent.addEventListener('change', updateCreate);
  }

  function onKeydown(e) {
    if (e.key === 'Escape') { e.preventDefault(); close(); return; }
    if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') { e.preventDefault(); create(); }
  }

  function selectedProjectPath() {
    if (!ui.project) return '';
    if (ui.project.value === '__manual__') return ui.projectPath.value.trim();
    return ui.project.value || '';
  }

  function setSource(src) {
    state.source = src;
    overlay.querySelectorAll('.wtm-tab').forEach((b) => b.classList.toggle('active', b.dataset.src === src));
    overlay.querySelectorAll('.wtm-src').forEach((p) => { p.hidden = p.dataset.srcPane !== src; });
    if (src === 'branch') loadBranches();
    updateCreate();
  }

  async function loadBranches() {
    const path = selectedProjectPath();
    if (!path || state.branchesFor === path) return;
    ui.branch.innerHTML = '<option>Loading branches…</option>';
    try {
      const branches = (await invoke('git_branches', { repo: path })) || [];
      state.branchesFor = path;
      ui.branch.innerHTML = branches.length
        ? branches.map((b) => `<option value="${escapeText(b)}">${escapeText(b)}</option>`).join('')
        : '<option value="">(no branches found)</option>';
    } catch (e) {
      ui.branch.innerHTML = `<option value="">failed: ${escapeText(String(e))}</option>`;
    }
  }

  function canCreate() {
    if (state.creating) return false;
    if (!selectedProjectPath()) return false;
    if (!ui.agent.value) return false;
    if (state.source === 'smart') return !!(state.ctx && state.ctx.issue);
    if (state.source === 'url') return !!state.parsed;
    return false; // branch + name are v1.6.1 forward-stubs
  }

  function updateCreate() {
    ui.create.disabled = !canCreate();
  }

  function showError(msg) {
    ui.error.textContent = msg;
    ui.error.hidden = !msg;
  }

  function populateProjects() {
    const repoName = String((state.ctx && state.ctx.repo) || '').split('/').pop();
    const opts = state.projects.map((p) =>
      `<option value="${escapeText(p.path)}">${escapeText(p.name || p.path)}</option>`);
    opts.push('<option value="__manual__">(pick path manually)</option>');
    ui.project.innerHTML = opts.join('');
    if (repoName) {
      const hit = state.projects.find((p) =>
        (p.name && p.name.endsWith(repoName)) || (p.path && p.path.endsWith(repoName)));
      if (hit) ui.project.value = hit.path;
    }
    ui.projectPath.hidden = ui.project.value !== '__manual__';
  }

  function populateAgents() {
    ui.agent.innerHTML = state.agents.map((a) =>
      `<option value="${escapeText(a.id)}" ${a.available ? '' : 'disabled'}>${escapeText(a.label || a.id)}</option>`).join('');
    const claude = state.agents.find((a) => a.id === 'claude' && a.available);
    const firstAvail = state.agents.find((a) => a.available);
    ui.agent.value = claude ? 'claude' : (firstAvail ? firstAvail.id : '');
  }

  async function openWorktreeModal(ctx) {
    buildModal();
    state.ctx = ctx || {};
    state.parsed = null;
    state.creating = false;
    state.branchesFor = null;

    // Reset per-open fields.
    ui.urlInput.value = '';
    ui.urlChip.hidden = true;
    ui.nameInput.value = '';
    ui.base.value = 'development';
    ui.branch.innerHTML = '';
    ui.create.disabled = true;
    ui.create.innerHTML = ui.createLabel;
    showError('');

    if (state.ctx.issue) {
      ui.smartChip.classList.remove('wtm-chip-muted');
      ui.smartChip.textContent = `#${state.ctx.issue.number} ${state.ctx.issue.title || ''}`;
    } else {
      ui.smartChip.classList.add('wtm-chip-muted');
      ui.smartChip.textContent = 'no issue attached — paste a Forge URL';
    }

    overlay.hidden = false;
    document.addEventListener('keydown', onKeydown, true);
    setSource(state.ctx.issue ? 'smart' : 'url');

    try {
      const [tasks, agents] = await Promise.all([invoke('tasks_list'), invoke('agent_list')]);
      state.projects = (tasks || []).filter((t) => t.kind === 'project');
      state.agents = agents || [];
      populateProjects();
      populateAgents();
    } catch (e) {
      showError(String(e));
    }
    updateCreate();
  }

  function close() {
    if (!overlay) return;
    overlay.hidden = true;
    document.removeEventListener('keydown', onKeydown, true);
  }

  async function create() {
    if (!canCreate()) return;
    const fromUrl = state.source === 'url';
    const number = fromUrl ? state.parsed.number : state.ctx.issue.number;
    const repo = fromUrl ? (state.parsed.repo || state.ctx.repo) : state.ctx.repo;
    if (!repo) { showError('No repository resolved — paste a full forge URL.'); return; }

    state.creating = true;
    ui.create.disabled = true;
    ui.create.innerHTML = 'Creating…';
    showError('');
    try {
      const spec = await invoke('scaffold_task_from_issue', {
        forgeIndex: Number.isInteger(state.ctx.forgeIndex) ? state.ctx.forgeIndex : 0,
        repo,
        number,
        agentId: ui.agent.value,
        localRepoPath: selectedProjectPath(),
        baseBranch: ui.base.value.trim() || 'development',
      });
      state.creating = false;
      close();
      if (window.xnautOpenLaunchSpec) window.xnautOpenLaunchSpec(spec);
    } catch (e) {
      state.creating = false;
      ui.create.innerHTML = ui.createLabel;
      showError(String(e));
      updateCreate();
    }
  }

  // Public API hook.
  window.xnautOpenWorktreeModal = openWorktreeModal;
})();
