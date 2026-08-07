// New project — a form, not a conversation.
//
// This replaces a chat that asked the same four things through a model. The
// model mislabelled forge URLs as "hosts" (our own variable name), offered a
// folder category as a project type, insisted on a host index when none was
// configured, and argued when corrected. Four known fields do not need a model
// to collect them.
//
// Order matters: only the local path is mandatory. Code has to live somewhere.
// Everything else — repo, agent — is optional, and skipping the repo is a
// complete, valid project rather than a dead end.
(function () {
  'use strict';

  const invoke = (...a) => window.__TAURI__.core.invoke(...a);

  function esc(s) {
    return String(s == null ? '' : s).replace(/[&<>"']/g, (c) => ({
      '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
    }[c]));
  }

  function injectStyles() {
    if (document.getElementById('rpnp-styles')) return;
    const st = document.createElement('style');
    st.id = 'rpnp-styles';
    st.textContent = `
      .rpnp { padding: 12px 14px; display: flex; flex-direction: column; gap: 14px;
        color: var(--text-primary, #ddd); font-size: 13px; overflow-y: auto; height: 100%; }
      .rpnp h3 { margin: 0; font-size: 13px; font-weight: 600; }
      .rpnp-field { display: flex; flex-direction: column; gap: 5px; }
      .rpnp-field label { font-size: 11px; letter-spacing: .04em; text-transform: uppercase;
        color: var(--text-muted, #777); }
      .rpnp-field .req { color: var(--amber, #F5B840); }
      .rpnp input, .rpnp select { background: var(--input-bg, rgba(255,255,255,.05));
        border: 1px solid var(--border-color, #3a3d45); border-radius: 6px; color: inherit;
        font: inherit; padding: 7px 9px; outline: none; }
      .rpnp input:focus, .rpnp select:focus { border-color: var(--accent, #4da3ff); }
      .rpnp-hint { font-size: 11px; color: var(--text-muted, #777); }
      .rpnp-repos { display: flex; gap: 6px; flex-wrap: wrap; }
      .rpnp-repo { padding: 5px 10px; border-radius: 999px; cursor: pointer;
        border: 1px solid var(--border-color, #3a3d45); background: transparent; color: inherit; font: inherit; }
      .rpnp-repo.on { border-color: var(--accent, #4da3ff); color: var(--accent, #4da3ff); }
      .rpnp-actions { display: flex; gap: 8px; align-items: center; margin-top: 2px; }
      .rpnp-btn { padding: 7px 14px; border-radius: 6px; border: 1px solid var(--border-color, #3a3d45);
        background: transparent; color: inherit; font: inherit; cursor: pointer; }
      .rpnp-btn.primary { background: var(--amber, #F5B840); border-color: transparent; color: #16130A; font-weight: 600; }
      .rpnp-btn:disabled { opacity: .45; cursor: not-allowed; }
      .rpnp-check { display: flex; flex-direction: column; gap: 4px; margin-top: 2px; }
      .rpnp-check-row { display: flex; gap: 8px; align-items: flex-start; font-size: 12px; }
      .rpnp-mark { width: 13px; flex: 0 0 auto; text-align: center; }
      .rpnp-mark.pass { color: #3fb950; } .rpnp-mark.fail { color: #ff5f56; }
      .rpnp-mark.skipped { color: var(--text-muted, #777); }
      .rpnp-check-hint { color: var(--text-muted, #888); font-size: 11px; }
      .rpnp-state { font-size: 12px; color: var(--text-muted, #888); }
      .rpnp-state.err { color: #ff5f56; }
    `;
    document.head.appendChild(st);
  }

  const REPOS = [['none', 'No repo'], ['github', 'GitHub'], ['gitlab', 'GitLab'], ['forgejo', 'Forgejo']];
  const AGENTS = [['', 'Choose later'], ['claude', 'Claude Code'], ['codex', 'Codex'], ['pi', 'Pi']];

  function mount(container) {
    injectStyles();
    const state = { kind: 'none' };

    container.innerHTML = `<div class="rpnp">
      <h3>New project</h3>

      <div class="rpnp-field">
        <label>Name <span class="req">*</span></label>
        <input class="rpnp-name" placeholder="Tony Stark" autocomplete="off">
      </div>

      <div class="rpnp-field">
        <label>Local path <span class="req">*</span></label>
        <input class="rpnp-path" placeholder="/Users/you/code/tony-stark" autocomplete="off" spellcheck="false">
        <div class="rpnp-hint">Created if it doesn't exist. The code has to live somewhere — this is the only required answer besides the name.</div>
      </div>

      <div class="rpnp-field">
        <label>Repository</label>
        <div class="rpnp-repos">${REPOS.map(([k, l]) =>
          `<button type="button" class="rpnp-repo${k === 'none' ? ' on' : ''}" data-kind="${k}">${esc(l)}</button>`).join('')}</div>
        <input class="rpnp-url" placeholder="git@github.com:you/tony-stark.git" autocomplete="off" spellcheck="false" hidden>
        <div class="rpnp-hint rpnp-repo-hint">Skipping is fine — a local-only project is complete. A sandbox needs a repo; local work does not.</div>
        <div class="rpnp-check" hidden></div>
      </div>

      <div class="rpnp-field">
        <label>Coding agent</label>
        <select class="rpnp-agent">${AGENTS.map(([v, l]) => `<option value="${v}">${esc(l)}</option>`).join('')}</select>
      </div>

      <div class="rpnp-actions">
        <button class="rpnp-btn primary rpnp-create" disabled>Create project</button>
        <button class="rpnp-btn rpnp-checkbtn" hidden>Check repo</button>
        <span class="rpnp-state"></span>
      </div>
    </div>`;

    const $ = (s) => container.querySelector(s);
    const name = $('.rpnp-name');
    const path = $('.rpnp-path');
    const url = $('.rpnp-url');
    const checkBox = $('.rpnp-check');
    const checkBtn = $('.rpnp-checkbtn');
    const createBtn = $('.rpnp-create');
    const stateEl = $('.rpnp-state');

    const say = (msg, err) => {
      stateEl.textContent = msg || '';
      stateEl.classList.toggle('err', !!err);
    };

    // Only name + path gate creation. A failing repo check is information, not
    // a blocker — the user may intend to fix the key afterwards.
    const sync = () => {
      createBtn.disabled = !(name.value.trim() && path.value.trim());
    };
    name.oninput = sync;
    path.oninput = sync;

    container.querySelectorAll('.rpnp-repo').forEach((b) => {
      b.onclick = () => {
        container.querySelectorAll('.rpnp-repo').forEach((x) => x.classList.toggle('on', x === b));
        state.kind = b.dataset.kind;
        const wantsRepo = state.kind !== 'none';
        url.hidden = !wantsRepo;
        checkBtn.hidden = !wantsRepo;
        checkBox.hidden = true;
        $('.rpnp-repo-hint').textContent = wantsRepo
          ? 'Paste the URL of the repo this should push to. Check it before creating — a missing SSH key or token fails later, somewhere unrelated.'
          : "Skipping is fine — a local-only project is complete. A sandbox needs a repo; local work does not.";
      };
    });

    checkBtn.onclick = async () => {
      checkBtn.disabled = true;
      say('Checking…');
      try {
        const rows = await invoke('repo_preflight', {
          path: path.value.trim() || null,
          url: url.value.trim() || null,
          kind: state.kind,
        });
        checkBox.hidden = false;
        checkBox.innerHTML = (rows || []).map((r) => {
          const mark = r.status === 'pass' ? '✓' : r.status === 'fail' ? '✗' : '–';
          return `<div class="rpnp-check-row">
            <span class="rpnp-mark ${esc(r.status)}">${mark}</span>
            <div><div>${esc(r.label)}<span class="rpnp-check-hint"> · ${esc(r.detail)}</span></div>
            ${r.hint ? `<div class="rpnp-check-hint">${esc(r.hint)}</div>` : ''}</div>
          </div>`;
        }).join('');
        say('');
      } catch (e) {
        say(String(e), true);
      } finally {
        checkBtn.disabled = false;
      }
    };

    createBtn.onclick = async () => {
      createBtn.disabled = true;
      say('Creating…');
      try {
        await invoke('project_create', {
          name: name.value.trim(),
          path: path.value.trim(),
          remote: state.kind === 'none' ? null : (url.value.trim() || null),
          agentId: $('.rpnp-agent').value || null,
        });
        say(`Created ${name.value.trim()}`);
        if (window.xnautSidebarRefresh) window.xnautSidebarRefresh();
      } catch (e) {
        say(String(e), true);
      } finally {
        createBtn.disabled = false;
        sync();
      }
    };

    sync();
  }

  const view = { mount, setRoot() {}, destroy() {} };
  if (window.xnautRightPaneRegisterView) window.xnautRightPaneRegisterView('newproject', view);
  else (window.__xnautRightPaneQueue = window.__xnautRightPaneQueue || []).push(['newproject', view]);
})();
