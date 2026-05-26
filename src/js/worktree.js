// Worktree-per-agent UI. Calls into the Rust commands defined in src-tauri/src/worktree.rs.
// Phase 2 of the Orca port — Phase 3 will plug agent launch into the "Create" flow.
(function () {
  'use strict';

  const $ = (id) => document.getElementById(id);
  const invoke = () => (window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke);

  let cachedRepoPath = null;

  function getRepoPath() {
    if (cachedRepoPath) return cachedRepoPath;
    // Try to infer from the focused terminal's cwd via the existing get_current_directory command.
    // Fallback: user types it in the modal field.
    const inv = invoke();
    if (!inv) return Promise.resolve(null);
    return inv('get_current_directory').then((p) => { cachedRepoPath = p; return p; }).catch(() => null);
  }

  function setStatus(msg, kind) {
    const el = $('worktree-status');
    if (!el) return;
    el.textContent = msg || '';
    el.dataset.kind = kind || 'info';
  }

  async function refreshList() {
    const inv = invoke();
    const repoInput = $('worktree-repo-path');
    const list = $('worktree-list');
    if (!list || !inv) return;
    const repo = (repoInput && repoInput.value.trim()) || (await getRepoPath());
    if (!repo) { list.innerHTML = '<div class="cap-meta">No repo path</div>'; return; }
    try {
      const items = await inv('worktree_list', { repoPath: repo });
      if (!items.length) {
        list.innerHTML = '<div class="cap-meta">No worktrees yet</div>';
        return;
      }
      list.innerHTML = items.map((w) => `
        <div class="worktree-row" data-path="${escapeAttr(w.path)}">
          <div class="worktree-row-main">
            <span class="worktree-branch">${w.branch ? escapeText(w.branch) : (w.is_detached ? '(detached)' : '(unknown)')}</span>
            <span class="worktree-path">${escapeText(w.path)}</span>
          </div>
          <button class="btn-icon worktree-remove" data-variant="destructive" title="Remove worktree" aria-label="Remove worktree">
            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor"><line x1="4" y1="4" x2="12" y2="12"/><line x1="12" y1="4" x2="4" y2="12"/></svg>
          </button>
        </div>`).join('');
      list.querySelectorAll('.worktree-remove').forEach((btn) => {
        btn.onclick = () => removeWorktree(btn.closest('.worktree-row').dataset.path, repo);
      });
    } catch (e) {
      list.innerHTML = `<div class="cap-meta" data-kind="error">${escapeText(String(e))}</div>`;
    }
  }

  async function createWorktree() {
    const inv = invoke();
    if (!inv) { setStatus('Tauri API not available', 'error'); return; }
    const repoInput = $('worktree-repo-path');
    const branchInput = $('worktree-branch');
    const baseInput = $('worktree-base');
    const pathInput = $('worktree-target-path');
    const repo = (repoInput.value.trim()) || (await getRepoPath());
    const branch = branchInput.value.trim();
    if (!repo) { setStatus('Repo path required', 'error'); return; }
    if (!branch) { setStatus('Branch name required', 'error'); return; }
    let target = pathInput.value.trim();
    if (!target) {
      target = await inv('worktree_suggest_path', { repoPath: repo, branch });
      pathInput.value = target;
    }
    setStatus('Creating worktree…', 'info');
    try {
      const w = await inv('worktree_add', {
        repoPath: repo,
        worktreePath: target,
        opts: {
          branch,
          base: baseInput.value.trim() || null,
          checkout_existing: false,
        },
      });
      setStatus(`Created ${w.branch || '(branch)'} at ${w.path}`, 'success');
      refreshList();
    } catch (e) {
      setStatus(String(e), 'error');
    }
  }

  async function removeWorktree(path, repo) {
    const inv = invoke();
    if (!inv) return;
    if (!confirm(`Remove worktree at ${path}?\n\nThis will also delete the branch if no other worktree uses it.`)) return;
    setStatus('Removing worktree…', 'info');
    try {
      await inv('worktree_remove', {
        repoPath: repo,
        worktreePath: path,
        opts: { force: false, delete_branch: true },
      });
      setStatus('Worktree removed', 'success');
      refreshList();
    } catch (e) {
      // Offer force on dirty worktree.
      if (String(e).includes('not clean')) {
        if (confirm('Worktree has uncommitted changes. Force-remove anyway?')) {
          try {
            await inv('worktree_remove', {
              repoPath: repo,
              worktreePath: path,
              opts: { force: true, delete_branch: true },
            });
            setStatus('Worktree force-removed', 'success');
            refreshList();
            return;
          } catch (e2) {
            setStatus(String(e2), 'error');
            return;
          }
        }
      }
      setStatus(String(e), 'error');
    }
  }

  async function openModal() {
    const modal = $('worktree-modal');
    if (!modal) return;
    modal.removeAttribute('hidden');
    setStatus('', 'info');
    // Default the repo path if it's empty
    const repoInput = $('worktree-repo-path');
    if (repoInput && !repoInput.value) {
      const p = await getRepoPath();
      if (p) repoInput.value = p;
    }
    // Suggest target path on branch change
    const branchInput = $('worktree-branch');
    const pathInput = $('worktree-target-path');
    if (branchInput && pathInput) {
      branchInput.oninput = async () => {
        const inv = invoke();
        const b = branchInput.value.trim();
        const repo = repoInput.value.trim();
        if (!inv || !b || !repo) return;
        try { pathInput.value = await inv('worktree_suggest_path', { repoPath: repo, branch: b }); } catch (e) {}
      };
    }
    refreshList();
  }

  function closeModal() {
    const modal = $('worktree-modal');
    if (modal) modal.setAttribute('hidden', '');
  }

  function escapeText(s) {
    return String(s).replace(/[&<>"']/g, (c) => ({ '&':'&amp;', '<':'&lt;', '>':'&gt;', '"':'&quot;', "'":'&#39;' }[c]));
  }
  function escapeAttr(s) { return escapeText(s); }

  // Wire up — runs after app.js DOMContentLoaded but before user clicks anything.
  function wire() {
    const btn = $('btn-worktree');
    if (btn) btn.onclick = openModal;
    const close = $('btn-close-worktree-modal');
    if (close) close.onclick = closeModal;
    const create = $('btn-create-worktree');
    if (create) create.onclick = createWorktree;
    const refresh = $('btn-refresh-worktrees');
    if (refresh) refresh.onclick = refreshList;
    const backdrop = document.querySelector('#worktree-modal .wt-modal-backdrop');
    if (backdrop) backdrop.onclick = closeModal;
    document.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') {
        const m = $('worktree-modal');
        if (m && !m.hasAttribute('hidden')) closeModal();
      }
    });
  }
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', wire);
  } else {
    wire();
  }
})();
