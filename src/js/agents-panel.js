(function () {
  'use strict';

  const ACCESS_PRESETS = [
    {
      id: 'draft-docs',
      label: 'Can Draft Docs',
      read: ['vault'],
      write: ['vault'],
      denied: ['source_code', 'terminal', 'secrets'],
    },
    {
      id: 'full-project',
      label: 'Full Project Access',
      read: ['vault', 'source_code', 'test_output'],
      write: ['assigned_files', 'vault'],
      denied: ['secrets'],
    },
  ];

  const EMPTY_PROFILE = {
    id: '',
    name: '',
    status: 'enabled',
    version: 1,
    role: '',
    skills: [],
    access: { read: [], write: [], denied: [] },
    tools: [],
    constraints: [],
    outputs: [],
    body: '',
    rel: '',
    built_in: false,
  };

  const invoke = (...args) => window.__TAURI__.core.invoke(...args);
  const clone = (value) => JSON.parse(JSON.stringify(value));
  const lines = (value) => (Array.isArray(value) ? value : []).join('\n');
  const splitLines = (value) => String(value || '').split(/\r?\n/).map((line) => line.trim()).filter(Boolean);
  const slug = (value) => String(value || 'custom-agent')
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '') || 'custom-agent';

  function createField(label, control, hint) {
    const wrap = document.createElement('label');
    wrap.className = 'agent-field';
    const text = document.createElement('span');
    text.textContent = label;
    wrap.append(text, control);
    if (hint) {
      const small = document.createElement('small');
      small.textContent = hint;
      wrap.appendChild(small);
    }
    return wrap;
  }

  function createInput(value, disabled) {
    const input = document.createElement('input');
    input.value = value || '';
    input.disabled = !!disabled;
    return input;
  }

  function createTextarea(value, disabled, rows) {
    const textarea = document.createElement('textarea');
    textarea.value = value || '';
    textarea.rows = rows || 3;
    textarea.disabled = !!disabled;
    return textarea;
  }

  function createStatusSelect(value, disabled) {
    const select = document.createElement('select');
    select.disabled = !!disabled;
    for (const status of ['enabled', 'disabled']) {
      const option = document.createElement('option');
      option.value = status;
      option.textContent = status.charAt(0).toUpperCase() + status.slice(1);
      select.appendChild(option);
    }
    select.value = value === 'disabled' ? 'disabled' : 'enabled';
    return select;
  }

  function normalizeProfile(profile) {
    const next = Object.assign({}, EMPTY_PROFILE, profile || {});
    next.status = next.status === 'disabled' ? 'disabled' : 'enabled';
    next.version = Number(next.version || 1);
    next.skills = Array.isArray(next.skills) ? next.skills : [];
    next.tools = Array.isArray(next.tools) ? next.tools : [];
    next.constraints = Array.isArray(next.constraints) ? next.constraints : [];
    next.outputs = Array.isArray(next.outputs) ? next.outputs : [];
    next.access = Object.assign({ read: [], write: [], denied: [] }, next.access || {});
    next.access.read = Array.isArray(next.access.read) ? next.access.read : [];
    next.access.write = Array.isArray(next.access.write) ? next.access.write : [];
    next.access.denied = Array.isArray(next.access.denied) ? next.access.denied : [];
    next.built_in = !!next.built_in;
    return next;
  }

  function createAgentsPanel(tabId, host, opts) {
    if (!host) return null;

    const seed = (opts && opts.agentFatherSeed) || {};
    const state = {
      profiles: [],
      selectedRel: seed.rel || '',
      selected: null,
      loading: true,
      error: '',
      status: 'Loading agent library...',
      selectRequestId: 0,
      loadRequestId: 0,
      userInteracted: false,
    };

    const pane = document.createElement('div');
    pane.className = 'agents-panel';
    pane.style.cssText = [
      'height:100%',
      'display:flex',
      'min-height:0',
      'background:var(--bg-primary)',
      'color:var(--text-primary)',
      'overflow:hidden',
    ].join(';');

    pane.innerHTML = `
      <style>
        .agents-panel * { box-sizing: border-box; }
        .agent-library {
          width: 260px;
          min-width: 260px;
          border-right: 1px solid var(--border-color);
          background: var(--bg-secondary);
          display: flex;
          flex-direction: column;
          min-height: 0;
        }
        .agent-library-head {
          padding: 12px;
          border-bottom: 1px solid var(--border-color);
          display: flex;
          align-items: center;
          justify-content: space-between;
          gap: 8px;
        }
        .agent-library-head h2 { margin: 0; font-size: 15px; line-height: 1.2; }
        .agent-library-head button,
        .agent-actions button {
          border: 1px solid var(--border-color);
          background: var(--bg-primary);
          color: var(--text-primary);
          border-radius: 6px;
          min-height: 28px;
          padding: 4px 9px;
          font-size: 12px;
          cursor: pointer;
        }
        .agent-library-head button:disabled,
        .agent-actions button:disabled,
        .agent-field input:disabled,
        .agent-field textarea:disabled,
        .agent-field select:disabled {
          opacity: 0.58;
          cursor: not-allowed;
        }
        .agent-list-scroll { overflow: auto; min-height: 0; padding: 8px; }
        .agent-section-title {
          color: var(--text-secondary);
          font-size: 11px;
          font-weight: 700;
          letter-spacing: .04em;
          text-transform: uppercase;
          margin: 10px 4px 5px;
        }
        .agent-list { display: flex; flex-direction: column; gap: 4px; margin-bottom: 10px; }
        .agent-row {
          width: 100%;
          border: 1px solid transparent;
          background: transparent;
          color: var(--text-primary);
          text-align: left;
          border-radius: 6px;
          padding: 7px 8px;
          cursor: pointer;
        }
        .agent-row:hover { background: var(--bg-primary); border-color: var(--border-color); }
        .agent-row.is-selected { background: var(--bg-primary); border-color: var(--accent-color); }
        .agent-row strong { display: block; font-size: 13px; line-height: 1.25; font-weight: 650; }
        .agent-row span { display: block; color: var(--text-secondary); font-size: 11px; margin-top: 2px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
        .agent-editor {
          flex: 1;
          min-width: 0;
          min-height: 0;
          overflow: auto;
          padding: 14px 16px 18px;
        }
        .agent-editor-head {
          display: flex;
          align-items: center;
          justify-content: space-between;
          gap: 12px;
          margin-bottom: 12px;
        }
        .agent-title h2 { margin: 0; font-size: 17px; line-height: 1.25; }
        .agent-title div { margin-top: 2px; color: var(--text-secondary); font-size: 12px; }
        .agent-actions { display: flex; align-items: center; gap: 7px; }
        .agent-actions .danger { color: var(--danger-color, #d14); }
        .agent-status { color: var(--text-secondary); font-size: 12px; margin-left: auto; }
        .agent-editor-grid { display: grid; grid-template-columns: minmax(0, 1fr); gap: 12px; max-width: 980px; }
        .agent-editor-section {
          border-top: 1px solid var(--border-color);
          padding-top: 12px;
          display: grid;
          gap: 10px;
        }
        .agent-editor-section h3 {
          margin: 0;
          font-size: 12px;
          color: var(--text-secondary);
          text-transform: uppercase;
          letter-spacing: .04em;
        }
        .agent-field { display: grid; gap: 5px; font-size: 12px; color: var(--text-secondary); }
        .agent-field input,
        .agent-field textarea,
        .agent-field select {
          width: 100%;
          border: 1px solid var(--border-color);
          border-radius: 6px;
          background: var(--bg-secondary);
          color: var(--text-primary);
          font: inherit;
          font-size: 13px;
          padding: 7px 8px;
        }
        .agent-field textarea {
          resize: vertical;
          min-height: 72px;
          line-height: 1.35;
        }
        .agent-field small { color: var(--text-secondary); font-size: 11px; }
        .agent-two-col { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 10px; }
        .agent-three-col { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 10px; }
        .agent-empty, .agent-error {
          color: var(--text-secondary);
          border-top: 1px solid var(--border-color);
          padding-top: 16px;
          font-size: 13px;
        }
        .agent-error { color: var(--danger-color, #d14); }
        @media (max-width: 760px) {
          .agents-panel { flex-direction: column; }
          .agent-library { width: 100%; min-width: 0; height: 230px; border-right: 0; border-bottom: 1px solid var(--border-color); }
          .agent-two-col, .agent-three-col { grid-template-columns: 1fr; }
          .agent-editor-head { align-items: flex-start; flex-direction: column; }
          .agent-actions { width: 100%; flex-wrap: wrap; }
        }
      </style>
      <aside class="agent-library">
        <div class="agent-library-head">
          <h2>Agent Library</h2>
          <button type="button" class="agent-new">New</button>
        </div>
        <div class="agent-list-scroll">
          <div class="agent-section-title">Built-in</div>
          <div class="agent-list" data-list="built-in"></div>
          <div class="agent-section-title">Custom</div>
          <div class="agent-list" data-list="custom"></div>
        </div>
      </aside>
      <section class="agent-editor"></section>`;

    host.replaceChildren(pane);

    const builtInList = pane.querySelector('[data-list="built-in"]');
    const customList = pane.querySelector('[data-list="custom"]');
    const editor = pane.querySelector('.agent-editor');
    const newButton = pane.querySelector('.agent-new');

    function renderList() {
      const drawGroup = (target, items, emptyText) => {
        target.replaceChildren();
        if (!items.length) {
          const empty = document.createElement('div');
          empty.className = 'agent-empty';
          empty.style.padding = '7px 8px';
          empty.style.borderTop = '0';
          empty.textContent = emptyText;
          target.appendChild(empty);
          return;
        }
        for (const profile of items) {
          const row = document.createElement('button');
          row.type = 'button';
          row.className = 'agent-row' + (profile.rel === state.selectedRel ? ' is-selected' : '');
          row.dataset.rel = profile.rel;
          const name = document.createElement('strong');
          name.textContent = profile.name || profile.id || profile.rel;
          const role = document.createElement('span');
          role.textContent = profile.role || profile.status || profile.rel;
          row.append(name, role);
          row.onclick = () => selectProfile(profile.rel);
          target.appendChild(row);
        }
      };

      const builtIns = state.profiles.filter((profile) => profile.built_in);
      const custom = state.profiles.filter((profile) => !profile.built_in);
      drawGroup(builtInList, builtIns, state.loading ? 'Loading...' : 'No built-in agents');
      drawGroup(customList, custom, 'No custom agents');
    }

    function renderEditor() {
      editor.replaceChildren();

      const selected = state.selected && normalizeProfile(state.selected);
      const title = selected ? selected.name || 'Untitled agent' : 'Agent Library';
      const subtitle = selected
        ? `${selected.built_in ? 'Built-in profile' : 'Custom profile'}${selected.rel ? ' - ' + selected.rel : ''}`
        : 'Select an agent to edit or create a custom profile.';

      const head = document.createElement('div');
      head.className = 'agent-editor-head';
      const titleWrap = document.createElement('div');
      titleWrap.className = 'agent-title';
      const h2 = document.createElement('h2');
      h2.textContent = title;
      const sub = document.createElement('div');
      sub.textContent = subtitle;
      titleWrap.append(h2, sub);

      const actions = document.createElement('div');
      actions.className = 'agent-actions';
      const status = document.createElement('span');
      status.className = 'agent-status';
      status.textContent = state.error || state.status || '';
      const save = document.createElement('button');
      save.type = 'button';
      save.textContent = 'Save';
      save.disabled = !selected || selected.built_in;
      const del = document.createElement('button');
      del.type = 'button';
      del.textContent = 'Delete';
      del.className = 'danger';
      del.disabled = !selected || selected.built_in || !selected.rel;
      actions.append(save, del, status);
      head.append(titleWrap, actions);
      editor.appendChild(head);

      if (state.error) {
        const err = document.createElement('div');
        err.className = 'agent-error';
        err.textContent = state.error;
        editor.appendChild(err);
      }

      if (!selected) {
        const empty = document.createElement('div');
        empty.className = 'agent-empty';
        empty.textContent = state.loading ? 'Loading profiles...' : 'No profile selected.';
        editor.appendChild(empty);
        return;
      }

      const readOnly = selected.built_in;
      const form = document.createElement('div');
      form.className = 'agent-editor-grid';

      const persona = document.createElement('section');
      persona.className = 'agent-editor-section';
      persona.setAttribute('data-section', 'persona'); // data-section="persona"
      const personaTitle = document.createElement('h3');
      personaTitle.textContent = 'Persona';
      const nameInput = createInput(selected.name, readOnly);
      const roleInput = createInput(selected.role, readOnly);
      const statusSelect = createStatusSelect(selected.status, readOnly);
      const versionInput = createInput(String(selected.version || 1), readOnly);
      versionInput.type = 'number';
      versionInput.min = '1';
      const personaGrid = document.createElement('div');
      personaGrid.className = 'agent-two-col';
      personaGrid.append(
        createField('Name', nameInput),
        createField('Role', roleInput),
        createField('Status', statusSelect),
        createField('Version', versionInput),
      );
      persona.append(personaTitle, personaGrid);

      const capabilities = document.createElement('section');
      capabilities.className = 'agent-editor-section';
      capabilities.setAttribute('data-section', 'capabilities');
      const capTitle = document.createElement('h3');
      capTitle.textContent = 'Capabilities';
      const skillsInput = createTextarea(lines(selected.skills), readOnly, 4);
      const toolsInput = createTextarea(lines(selected.tools), readOnly, 4);
      const constraintsInput = createTextarea(lines(selected.constraints), readOnly, 4);
      const outputsInput = createTextarea(lines(selected.outputs), readOnly, 4);
      const capGrid = document.createElement('div');
      capGrid.className = 'agent-two-col';
      capGrid.append(
        createField('Skills', skillsInput, 'One skill per line.'),
        createField('Tools', toolsInput, 'One tool per line.'),
        createField('Constraints', constraintsInput, 'One constraint per line.'),
        createField('Outputs', outputsInput, 'One output per line.'),
      );
      capabilities.append(capTitle, capGrid);

      const access = document.createElement('section');
      access.className = 'agent-editor-section';
      access.setAttribute('data-section', 'access'); // data-section="access"
      const accessTitle = document.createElement('h3');
      accessTitle.textContent = 'Access';
      const presetSelect = document.createElement('select');
      presetSelect.disabled = readOnly;
      const blank = document.createElement('option');
      blank.value = '';
      blank.textContent = 'Manual';
      presetSelect.appendChild(blank);
      for (const preset of ACCESS_PRESETS) {
        const option = document.createElement('option');
        option.value = preset.id;
        option.textContent = preset.label;
        presetSelect.appendChild(option);
      }
      const readInput = createTextarea(lines(selected.access.read), readOnly, 4);
      const writeInput = createTextarea(lines(selected.access.write), readOnly, 4);
      const deniedInput = createTextarea(lines(selected.access.denied), readOnly, 4);
      presetSelect.onchange = () => {
        const preset = ACCESS_PRESETS.find((item) => item.id === presetSelect.value);
        if (!preset) return;
        readInput.value = lines(preset.read);
        writeInput.value = lines(preset.write);
        deniedInput.value = lines(preset.denied);
      };
      const accessGrid = document.createElement('div');
      accessGrid.className = 'agent-three-col';
      accessGrid.append(
        createField('Read', readInput),
        createField('Write', writeInput),
        createField('Denied', deniedInput),
      );
      access.append(accessTitle, createField('Preset', presetSelect), accessGrid);

      const markdown = document.createElement('section');
      markdown.className = 'agent-editor-section';
      markdown.setAttribute('data-section', 'markdown'); // data-section="markdown"
      const markdownTitle = document.createElement('h3');
      markdownTitle.textContent = 'Markdown Body';
      const bodyInput = createTextarea(selected.body, readOnly, 12);
      markdown.append(markdownTitle, createField('Body', bodyInput));

      form.append(persona, capabilities, access, markdown);
      editor.appendChild(form);

      save.onclick = async () => {
        const name = nameInput.value.trim();
        const id = selected.id || slug(name);
        const profile = normalizeProfile({
          id,
          name: name || id,
          status: statusSelect.value === 'disabled' ? 'disabled' : 'enabled',
          version: Math.max(1, Number(versionInput.value || 1)),
          role: roleInput.value.trim(),
          skills: splitLines(skillsInput.value),
          access: {
            read: splitLines(readInput.value),
            write: splitLines(writeInput.value),
            denied: splitLines(deniedInput.value),
          },
          tools: splitLines(toolsInput.value),
          constraints: splitLines(constraintsInput.value),
          outputs: splitLines(outputsInput.value),
          body: bodyInput.value,
          rel: selected.rel || `System/Agents/Custom/${slug(name || id)}.md`,
          built_in: false,
        });
        await saveProfile(profile);
      };

      del.onclick = () => deleteProfile(selected);
    }

    async function loadProfiles(preferredRel, forceSelect) {
      const requestId = ++state.loadRequestId;
      state.loading = true;
      state.error = '';
      state.status = 'Loading agent library...';
      renderList();
      renderEditor();

      try {
        await invoke('agent_profiles_seed');
        if (requestId !== state.loadRequestId) return;
        const profiles = ((await invoke('agent_profiles_list')) || []).map(normalizeProfile);
        if (requestId !== state.loadRequestId) return;
        state.profiles = profiles;
        const rel = preferredRel || state.selectedRel || (state.profiles[0] && state.profiles[0].rel) || '';
        state.loading = false;
        state.status = `${state.profiles.length} profiles`;
        renderList();
        if (rel && (!!forceSelect || !state.userInteracted)) await selectProfile(rel, false);
        else renderEditor();
      } catch (err) {
        if (requestId !== state.loadRequestId) return;
        state.loading = false;
        state.error = String(err);
        state.status = '';
        renderList();
        renderEditor();
      }
    }

    async function selectProfile(rel, markUserInteracted) {
      if (!rel) return;
      if (markUserInteracted !== false) state.userInteracted = true;
      const requestId = ++state.selectRequestId;
      state.selectedRel = rel;
      state.error = '';
      state.status = 'Loading profile...';
      const summary = state.profiles.find((profile) => profile.rel === rel);
      state.selected = summary ? clone(summary) : null;
      renderList();
      renderEditor();

      try {
        const profile = normalizeProfile(await invoke('agent_profile_read', { rel }));
        if (requestId !== state.selectRequestId || rel !== state.selectedRel) return;
        state.selected = profile;
        state.status = state.selected.built_in ? 'Built-in profiles are read-only' : 'Ready';
        renderList();
        renderEditor();
      } catch (err) {
        if (requestId !== state.selectRequestId || rel !== state.selectedRel) return;
        state.error = String(err);
        state.status = '';
        renderEditor();
      }
    }

    async function saveProfile(profile) {
      state.error = '';
      state.status = 'Saving...';
      renderEditor();
      try {
        const saved = normalizeProfile(await invoke('agent_profile_save', { profile }));
        state.selected = saved;
        state.selectedRel = saved.rel;
        await loadProfiles(saved.rel, true);
        state.status = 'Saved';
        renderEditor();
      } catch (err) {
        state.error = String(err);
        state.status = '';
        renderEditor();
      }
    }

    async function deleteProfile(profile) {
      if (!profile || profile.built_in || !profile.rel) return;
      if (!window.confirm(`Delete ${profile.name || profile.rel}?`)) return;
      state.error = '';
      state.status = 'Deleting...';
      renderEditor();
      try {
        await invoke('agent_profile_delete', { rel: profile.rel });
        state.selected = null;
        state.selectedRel = '';
        await loadProfiles('', true);
        state.status = 'Deleted';
        renderEditor();
      } catch (err) {
        state.error = String(err);
        state.status = '';
        renderEditor();
      }
    }

    newButton.onclick = () => {
      state.userInteracted = true;
      state.selectRequestId += 1;
      const profile = normalizeProfile(clone(EMPTY_PROFILE));
      profile.id = slug(seed.name || 'custom-agent');
      profile.name = seed.name || 'Custom Agent';
      profile.role = seed.role || '';
      profile.body = seed.body || '# Custom Agent\n\n';
      profile.access = {
        read: clone(ACCESS_PRESETS[0].read),
        write: clone(ACCESS_PRESETS[0].write),
        denied: clone(ACCESS_PRESETS[0].denied),
      };
      state.selected = profile;
      state.selectedRel = '';
      state.error = '';
      state.status = 'New custom profile';
      renderList();
      renderEditor();
    };

    renderList();
    renderEditor();
    loadProfiles(state.selectedRel);

    return { kind: 'agents', tabId, pane, seed, accessPresets: ACCESS_PRESETS };
  }

  window.xnautAgentAccessPresets = ACCESS_PRESETS;
  window.xnautCreateAgentsPanel = createAgentsPanel;
  window.xnautOpenAgentFather = function (seed) {
    if (window.xnautAttachAgentsTab) {
      return window.xnautAttachAgentsTab({ agentFatherSeed: seed || {} });
    }
    return null;
  };
})();
