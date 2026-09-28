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
  // Friendly names for the provider keys we know; anything else shows its key.
  const PROVIDER_LABEL = {
    lmstudio: 'LM Studio (local)',
    ollama: 'Ollama (local)',
    openai: 'OpenAI',
    openrouter: 'OpenRouter',
    anthropic: 'Anthropic',
    perplexity: 'Perplexity',
    nautgate: 'NautGate',
  };

  // Providers come from what is CONFIGURED (Settings → AI Providers), unioned
  // with whatever the model catalogue has seen. Deriving from the catalogue
  // alone would hide a local provider whose server happens to be down — you
  // should still be able to pick LM Studio and start it afterwards.
  // Providers xNAUT supports, so one can be chosen before it is set up — the
  // option is labelled rather than hidden, since a hidden option looks like a
  // missing feature.
  const KNOWN = ['lmstudio', 'ollama', 'openai', 'openrouter', 'anthropic'];

  async function providerKeys() {
    const keys = new Set(KNOWN);
    const configured = new Set();
    try {
      const st = await invoke('settings_get');
      (st.llm_providers || []).forEach((p) => {
        if (p && p.name) { keys.add(String(p.name)); configured.add(String(p.name)); }
      });
      if (st.llm && st.llm.provider) { keys.add(String(st.llm.provider)); configured.add(String(st.llm.provider)); }
    } catch (_) { /* settings unreadable — fall back to the catalogue alone */ }
    const cat = window.xnautModelCatalog;
    ((cat && cat.all()) || []).forEach((m) => {
      if (m.provider) { keys.add(String(m.provider)); configured.add(String(m.provider)); }
    });
    return [...keys].filter(Boolean).sort().map((k) => ({ key: k, configured: configured.has(k) }));
  }

  // Shared with the project page's session picker, so both offer the same
  // providers and only one list has to be kept right.
  window.xnautProviderList = providerKeys;
  window.xnautProviderLabel = (k) => PROVIDER_LABEL[k] || k;

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
        <label>Coding provider</label>
        <select class="rpnp-provider"></select>
      </div>

      <div class="rpnp-field rpnp-model-field" hidden>
        <label>Model</label>
        <select class="rpnp-model"><option value="">Provider default</option></select>
        <div class="rpnp-hint rpnp-model-hint"></div>
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

    // Models depend on the provider, so the field only appears once one is
    // chosen, and lists what the catalogue actually knows for it.
    const providerSel = $('.rpnp-provider');
    const modelField = $('.rpnp-model-field');
    const modelSel = $('.rpnp-model');
    const modelHint = $('.rpnp-model-hint');

    async function fillProviders() {
      const list = await providerKeys();
      const keep = providerSel.value;
      providerSel.innerHTML = '<option value="">Choose later</option>'
        + list.map((p) => `<option value="${esc(p.key)}">${esc(PROVIDER_LABEL[p.key] || p.key)}${p.configured ? '' : ' — not configured'}</option>`).join('');
      if (list.some((p) => p.key === keep)) providerSel.value = keep;
    }

    function fillModels() {
      const p = providerSel.value;
      modelField.hidden = !p;
      if (!p) return;
      const cat = window.xnautModelCatalog;
      const models = (cat && cat.forProvider(p)) || [];
      modelSel.innerHTML = '<option value="">Provider default</option>'
        + models.map((m) => {
            const id = typeof m === 'string' ? m : (m.id || m.name || '');
            return id ? `<option value="${esc(id)}">${esc(id)}</option>` : '';
          }).join('');
      modelHint.textContent = models.length
        ? `${models.length} model${models.length === 1 ? '' : 's'} from the catalogue`
        : 'No catalogue entry yet for this provider — the default is used.';
    }
    providerSel.onchange = fillModels;
    // The catalogue refreshes in the background; re-fill both when it lands.
    window.addEventListener('xnaut-model-catalog-update', () => { fillProviders(); fillModels(); });
    fillProviders();
    if (window.xnautModelCatalog) window.xnautModelCatalog.refreshIfStale();

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

    // A PM key is 2-12 uppercase letters/digits. Derived from the name so the
    // user is not asked for a second identifier they do not care about.
    function keyFor(nm) {
      const k = String(nm).toUpperCase().replace(/[^A-Z0-9]/g, '').slice(0, 12);
      return k.length >= 2 ? k : null;
    }

    createBtn.onclick = async () => {
      createBtn.disabled = true;
      say('Creating…');
      const projectName = name.value.trim();
      try {
        await invoke('project_create', {
          name: projectName,
          path: path.value.trim(),
          remote: state.kind === 'none' ? null : (url.value.trim() || null),
          agentId: providerSel.value || null,
          model: modelSel.value || null,
        });

        // Also create the PM record, so the project HAS an overview to land on.
        // A clashing or invalid key is not fatal — the project exists either
        // way, and we simply do not navigate.
        const key = keyFor(projectName);
        if (key) {
          try {
            await invoke('pm_project_create', { request: { key, name: projectName, source_repo: url.value.trim() || '' } });
          } catch (e) {
            console.warn('[newproject] PM record not created (may already exist):', e);
          }
        }

        if (window.xnautSidebarRefresh) window.xnautSidebarRefresh();

        // Clear, so the form is ready for the next one rather than showing a
        // filled-in copy of what was just created.
        name.value = '';
        path.value = '';
        url.value = '';
        checkBox.hidden = true;
        checkBox.innerHTML = '';
        say('');

        // Land on the project — in its workspace, which takes the project as an
        // argument (XNAUT-342). This used to attach the standalone Projects
        // panel and then call xnautShowProject a tick later to select within
        // it; the workspace needs neither the second step nor the guessed
        // delay, because the project is what it is opened WITH.
        if (key) {
          if (typeof window.xnautOpenWorkspace === 'function') {
            window.xnautOpenWorkspace({ project: key, tab: 'work' });
          } else {
            console.error('[newproject] xnautOpenWorkspace is not loaded, so the new project cannot open');
          }
        }
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
  else (window.__xnautRightPaneQueue = window.__xnautRightPaneQueue || []).push({ key: 'newproject', view });
})();
