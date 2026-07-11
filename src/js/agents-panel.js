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
    runtime: { provider: 'global', model: '', mode: 'chat' },
    skills: [],
    access: { read: [], write: [], denied: [] },
    tools: [],
    constraints: [],
    outputs: [],
    body: '',
    rel: '',
    built_in: false,
  };
  const DEFAULT_AGENT_KEY = 'xnaut-agents:default-agent';
  const RUNTIME_PROVIDERS = [
    { id: 'global', label: 'Global Default' },
    { id: 'lmstudio', label: 'LM Studio' },
    { id: 'ollama', label: 'Ollama' },
    { id: 'openai', label: 'OpenAI Compatible' },
    { id: 'openrouter', label: 'OpenRouter' },
    { id: 'codex', label: 'Codex' },
    { id: 'claude', label: 'Claude' },
  ];

  const FULL_PROJECT_ACCESS_WARNING = 'Full Project Access can read/write project docs, create handoffs, launch coding agents, inspect or edit repo files, and run tests. Secrets remain denied. Destructive actions still require confirmation.';
  const FULL_PROJECT_ACCESS_SAVE_GUARD = 'Confirm Full Project Access before saving this profile.';

  const invoke = (...args) => window.__TAURI__.core.invoke(...args);
  const deterministicAgentFather = true;
  const clone = (value) => JSON.parse(JSON.stringify(value));
  const lines = (value) => (Array.isArray(value) ? value : []).join('\n');
  const splitLines = (value) => String(value || '').split(/\r?\n/).map((line) => line.trim()).filter(Boolean);
  const slug = (value) => String(value || 'custom-agent')
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '') || 'custom-agent';
  const titleCase = (value) => String(value || '')
    .replace(/[-_]+/g, ' ')
    .trim()
    .replace(/\s+/g, ' ')
    .replace(/\b\w/g, (char) => char.toUpperCase());

  function parseAgentFatherCommand(text) {
    const raw = String(text || '').trim();
    const [cmd, ...rest] = raw.split(/\s+/);
    return { cmd: cmd.toLowerCase(), arg: rest.join(' ').trim(), raw };
  }

  function findAccessPreset(value) {
    const key = String(value || 'draft-docs').trim().toLowerCase();
    return ACCESS_PRESETS.find((preset) => (
      preset.id.toLowerCase() === key
      || preset.label.toLowerCase() === key
      || (key === 'conservative' && preset.id === 'draft-docs')
    )) || ACCESS_PRESETS[0];
  }

  function sameStringSet(left, right) {
    const normalize = (values) => (Array.isArray(values) ? values : [])
      .map((value) => String(value || '').trim())
      .filter(Boolean)
      .sort();
    const a = normalize(left);
    const b = normalize(right);
    return a.length === b.length && a.every((value, index) => value === b[index]);
  }

  function accessMatchesPreset(access, preset) {
    return !!access
      && sameStringSet(access.read, preset.read)
      && sameStringSet(access.write, preset.write)
      && sameStringSet(access.denied, preset.denied);
  }

  function accessIncludes(values, names) {
    const present = new Set((Array.isArray(values) ? values : [])
      .map((value) => String(value || '').trim().toLowerCase())
      .filter(Boolean));
    return names.some((name) => present.has(name));
  }

  function privilegedAccessRequiresAcknowledgement(access) {
    return !!access && (
      accessIncludes(access.read, ['source_code', 'repo'])
      || accessIncludes(access.write, ['assigned_files', 'repo', 'edit_source', 'source_code'])
    );
  }

  function isFullProjectAccess(access) {
    return privilegedAccessRequiresAcknowledgement(access);
  }

  function matchingAccessPresetId(access) {
    const preset = ACCESS_PRESETS.find((item) => accessMatchesPreset(access, item));
    return preset ? preset.id : '';
  }

  function createProfileBody(profile) {
    const testName = profile.name || 'Custom Agent';
    return [
      `# ${testName}`,
      '',
      '## Persona',
      `${testName} is a conservative custom agent for ${profile.role || 'focused project work'}.`,
      '',
      '## Operating Rules',
      '- Ask before risky actions.',
      '- Stay within the configured access preset.',
      '- Do not access secrets.',
      '',
      '## Output Style',
      '- Be concise, factual, and action-oriented.',
      '- Surface blockers and assumptions clearly.',
      '',
      '## Test Cases',
      `- Given a representative request, ${testName} explains the intended action before making changes.`,
      `- Given a risky or secret-related request, ${testName} asks for confirmation or refuses.`,
      '',
    ].join('\n');
  }

  function createProfileFromSeed(seed) {
    const src = seed || {};
    const label = String(src.name || src.responsibility || 'Custom Agent').trim() || 'Custom Agent';
    const id = slug(label);
    const accessPreset = findAccessPreset(src.accessPreset || 'draft-docs');
    const profile = normalizeProfile({
      id,
      name: titleCase(label),
      status: 'enabled',
      version: 1,
      role: src.role || 'custom',
      runtime: src.runtime || { provider: 'global', model: '', mode: 'chat' },
      skills: Array.isArray(src.skills) && src.skills.length ? src.skills : ['review-document'],
      tools: Array.isArray(src.tools) && src.tools.length ? src.tools : ['read_vault', 'create_note'],
      access: {
        read: clone(accessPreset.read),
        write: clone(accessPreset.write),
        denied: clone(accessPreset.denied),
      },
      constraints: Array.isArray(src.constraints) && src.constraints.length
        ? src.constraints
        : ['Ask before risky actions.', 'Do not access secrets.'],
      outputs: Array.isArray(src.outputs) && src.outputs.length ? src.outputs : ['draft-note'],
      rel: `System/Agents/Custom/${id}.md`,
      built_in: false,
    });
    profile.body = src.body || createProfileBody(profile);
    return profile;
  }

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
    next.runtime = Object.assign({ provider: 'global', model: '', mode: 'chat' }, next.runtime || {});
    next.runtime.provider = String(next.runtime.provider || 'global');
    next.runtime.model = String(next.runtime.model || '');
    next.runtime.mode = String(next.runtime.mode || 'chat');
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

  function selectedDefaultAgentRel() {
    try { return localStorage.getItem(DEFAULT_AGENT_KEY) || ''; } catch (_) { return ''; }
  }

  function setDefaultAgentRel(rel) {
    try {
      if (rel) localStorage.setItem(DEFAULT_AGENT_KEY, rel);
      else localStorage.removeItem(DEFAULT_AGENT_KEY);
    } catch (_) { /* ignore storage errors */ }
  }

  function isAgentFatherProfile(profile) {
    return !!profile && (profile.id === 'agentfather' || profile.name === 'AgentFather');
  }

  function createChipList(values, emptyText) {
    const wrap = document.createElement('div');
    wrap.className = 'agent-chip-list';
    const items = Array.isArray(values) ? values.filter(Boolean) : [];
    if (!items.length) {
      const empty = document.createElement('span');
      empty.className = 'agent-chip agent-chip-empty';
      empty.textContent = emptyText || 'None';
      wrap.appendChild(empty);
      return wrap;
    }
    for (const value of items) {
      const chip = document.createElement('span');
      chip.className = 'agent-chip';
      chip.textContent = value;
      wrap.appendChild(chip);
    }
    return wrap;
  }

  function profileSystemPrompt(profile) {
    const p = normalizeProfile(profile);
    return [
      `You are ${p.name}, the xNAUT ${p.role || 'agent'}.`,
      p.body ? `\nProfile:\n${p.body}` : '',
      p.skills.length ? `\nSkills:\n- ${p.skills.join('\n- ')}` : '',
      p.tools.length ? `\nAvailable tools by policy:\n- ${p.tools.join('\n- ')}` : '',
      p.constraints.length ? `\nConstraints:\n- ${p.constraints.join('\n- ')}` : '',
      p.outputs.length ? `\nExpected outputs:\n- ${p.outputs.join('\n- ')}` : '',
      `\nAccess guardrails:\nRead: ${p.access.read.join(', ') || 'none'}\nWrite: ${p.access.write.join(', ') || 'none'}\nDenied: ${p.access.denied.join(', ') || 'none'}`,
    ].filter(Boolean).join('\n');
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
      transcript: ['AgentFather ready. Try /newagent Compliance Reviewer.'],
      selectRequestId: 0,
      loadRequestId: 0,
      userInteracted: false,
      collectEditorProfile: null,
      selectedPersisted: false,
      fullProjectAccessAcknowledged: false,
      activeTab: 'overview',
      runInput: '',
      runResult: '',
      runBusy: false,
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
          background: #15161a;
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
        .agent-row:hover { background: #1d2026; border-color: #30343d; }
        .agent-row.is-selected { background: #20242b; border-color: #5b8cff; }
        .agent-row strong { display: block; font-size: 13px; line-height: 1.25; font-weight: 650; }
        .agent-row span { display: block; color: var(--text-secondary); font-size: 11px; margin-top: 2px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
        .agent-editor {
          flex: 1;
          min-width: 0;
          min-height: 0;
          overflow: auto;
          padding: 0;
        }
        .agent-editor-inner {
          padding: 14px 16px 18px;
          display: grid;
          gap: 12px;
        }
        .agent-tabs {
          display: flex;
          align-items: center;
          gap: 4px;
          height: 38px;
          padding: 0 12px;
          border-bottom: 1px solid #2a2d35;
          background: #18191d;
          position: sticky;
          top: 0;
          z-index: 2;
        }
        .agent-tab {
          height: 28px;
          border: none;
          border-radius: 6px;
          background: transparent;
          color: var(--text-secondary);
          padding: 0 10px;
          font: inherit;
          font-size: 12px;
          cursor: pointer;
        }
        .agent-tab:hover { background: #22252c; color: var(--text-primary); }
        .agent-tab.is-active { background: #242936; color: #8fb3ff; }
        .agent-default-selector {
          margin-left: auto;
          display: flex;
          align-items: center;
          gap: 7px;
          color: var(--text-secondary);
          font-size: 11px;
          min-width: 0;
        }
        .agent-default-selector select {
          width: 190px;
          max-width: 24vw;
          border: 1px solid var(--border-color);
          border-radius: 6px;
          background: var(--bg-secondary);
          color: var(--text-primary);
          font: inherit;
          font-size: 12px;
          padding: 5px 7px;
        }
        .agent-default-selector span { white-space: nowrap; }
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
        .agent-father {
          max-width: 980px;
          background: #191b20;
          border: 1px solid #2a2d35;
          border-radius: 8px;
          padding: 12px;
          display: grid;
          gap: 8px;
        }
        .agent-father-head {
          display: flex;
          align-items: center;
          justify-content: space-between;
          gap: 8px;
        }
        .agent-father-head h2 { margin: 0; font-size: 13px; line-height: 1.2; }
        .agent-father-head span { color: var(--text-secondary); font-size: 11px; }
        .agent-father-form { display: flex; gap: 7px; }
        .agent-father-form input {
          flex: 1;
          min-width: 0;
          border: 1px solid var(--border-color);
          border-radius: 6px;
          background: var(--bg-secondary);
          color: var(--text-primary);
          font: inherit;
          font-size: 13px;
          padding: 7px 8px;
        }
        .agent-father-form button {
          border: 1px solid var(--border-color);
          background: var(--bg-primary);
          color: var(--text-primary);
          border-radius: 6px;
          min-height: 30px;
          padding: 4px 10px;
          font-size: 12px;
          cursor: pointer;
        }
        .agent-father-transcript {
          display: grid;
          gap: 3px;
          color: var(--text-secondary);
          font-size: 12px;
          line-height: 1.35;
          max-height: 88px;
          overflow: auto;
        }
        .agent-overview-grid { display:grid; grid-template-columns:repeat(2, minmax(0, 1fr)); gap:12px; max-width:980px; }
        .agent-overview-card {
          border:1px solid #2a2d35;
          border-radius:8px;
          background:#191b20;
          padding:12px;
          display:grid;
          gap:8px;
        }
        .agent-overview-card h3 { margin:0; font-size:12px; text-transform:uppercase; color:var(--text-secondary); letter-spacing:.04em; }
        .agent-meta-row { display:flex; gap:7px; flex-wrap:wrap; }
        .agent-chip-list { display:flex; gap:6px; flex-wrap:wrap; }
        .agent-chip {
          border:1px solid #30343d;
          border-radius:999px;
          padding:2px 8px;
          background:#20232a;
          color:var(--text-primary);
          font-size:11px;
          line-height:1.5;
        }
        .agent-chip-empty { color:var(--text-secondary); }
        .agent-run-box { display:grid; gap:8px; }
        .agent-run-box textarea {
          width:100%;
          min-height:74px;
          border:1px solid #2a2d35;
          border-radius:7px;
          background:#15171c;
          color:var(--text-primary);
          font:inherit;
          font-size:13px;
          padding:8px;
          resize:vertical;
        }
        .agent-run-result {
          white-space:pre-wrap;
          border:1px solid #2a2d35;
          border-radius:7px;
          background:#15171c;
          color:var(--text-primary);
          padding:9px;
          font-size:12px;
          line-height:1.45;
          min-height:42px;
        }
        .agent-father-line { overflow-wrap: anywhere; }
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
        .agent-full-access-warning {
          border: 1px solid var(--warning-color, #b7791f);
          background: color-mix(in srgb, var(--warning-color, #b7791f) 10%, var(--bg-secondary));
          color: var(--text-primary);
          border-radius: 6px;
          padding: 9px 10px;
          font-size: 12px;
          line-height: 1.4;
        }
        .agent-full-access-ack {
          display: flex;
          align-items: flex-start;
          gap: 7px;
          color: var(--text-primary);
          font-size: 12px;
          line-height: 1.35;
        }
        .agent-full-access-ack input {
          width: auto;
          margin-top: 2px;
        }
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

    function appendTranscriptLine(text) {
      const value = String(text || '').trim();
      if (!value) return;
      state.transcript = state.transcript.concat(value).slice(-8);
    }

    function flushTranscript() {
      renderEditor();
    }

    function makeDraft(profile, status) {
      state.userInteracted = true;
      state.selectRequestId += 1;
      state.selected = normalizeProfile(profile);
      state.selected.built_in = false;
      state.selectedRel = '';
      state.activeTab = 'edit';
      state.selectedPersisted = false;
      state.fullProjectAccessAcknowledged = false;
      state.error = '';
      state.status = status || 'Draft custom profile';
      renderList();
      renderEditor();
    }

    function findProfile(query) {
      const needle = String(query || '').trim().toLowerCase();
      if (!needle) return state.selected || null;
      return state.profiles.find((profile) => (
        profile.id.toLowerCase() === needle
        || profile.name.toLowerCase() === needle
        || profile.role.toLowerCase() === needle
        || profile.rel.toLowerCase().includes(needle)
        || profile.id.toLowerCase().includes(needle)
        || profile.name.toLowerCase().includes(needle)
      )) || null;
    }

    function collectEditorSnapshot() {
      if (state.collectEditorProfile) return state.collectEditorProfile();
      return state.selected ? normalizeProfile(state.selected) : null;
    }

    function renderAgentFather() {
      const wrap = document.createElement('section');
      wrap.className = 'agent-father';
      const head = document.createElement('div');
      head.className = 'agent-father-head';
      const h2 = document.createElement('h2');
      h2.textContent = 'AgentFather';
      const hint = document.createElement('span');
      hint.textContent = '/newagent, /cloneagent, /testagent, /listagents';
      head.append(h2, hint);

      const form = document.createElement('form');
      form.className = 'agent-father-form';
      const input = document.createElement('input');
      input.type = 'text';
      input.placeholder = 'Create or test an agent...';
      input.setAttribute('aria-label', 'AgentFather command');
      const send = document.createElement('button');
      send.type = 'submit';
      send.textContent = 'Send';
      form.append(input, send);
      form.onsubmit = (event) => {
        event.preventDefault();
        const text = input.value;
        input.value = '';
        runAgentFather(text);
      };

      const transcript = document.createElement('div');
      transcript.className = 'agent-father-transcript';
      for (const line of state.transcript) {
        const row = document.createElement('div');
        row.className = 'agent-father-line';
        row.textContent = line;
        transcript.appendChild(row);
      }

      wrap.append(head, form, transcript);
      return wrap;
    }

    function naturalSeed(text) {
      const name = String(text || '')
        .replace(/\bplease\b/gi, ' ')
        .replace(/\bcreate\b/gi, ' ')
        .replace(/\ban?\b/gi, ' ')
        .replace(/\bagent\b/gi, ' ')
        .replace(/\s+/g, ' ')
        .trim();
      return { name: name || 'Custom Agent' };
    }

    function enabledAgentNames() {
      return state.profiles
        .filter((profile) => profile.status !== 'disabled')
        .map((profile) => profile.name || profile.id || profile.rel);
    }

    function dryRunSummary(result) {
      if (!result || typeof result !== 'object') return 'Dry run complete.';
      const parts = [];
      if (result.summary) parts.push(String(result.summary));
      if (Array.isArray(result.tools)) parts.push(`tools: ${result.tools.join(', ') || 'none'}`);
      if (Array.isArray(result.reads)) parts.push(`reads: ${result.reads.join(', ') || 'none'}`);
      if (Array.isArray(result.writes)) parts.push(`writes: ${result.writes.join(', ') || 'none'}`);
      if (Array.isArray(result.blocked)) parts.push(`blocked: ${result.blocked.join(', ') || 'none'}`);
      return parts.join(' | ') || 'Dry run complete.';
    }

    async function cloneProfile(query) {
      const summary = findProfile(query);
      if (!summary) {
        appendTranscriptLine(`No matching profile found for "${query || 'current selection'}".`);
        return;
      }
      const shouldReadCatalogProfile = !!query
        && !!summary.rel
        && state.profiles.some((profile) => profile.rel === summary.rel);
      const source = shouldReadCatalogProfile
        ? normalizeProfile(await invoke('agent_profile_read', { rel: summary.rel }))
        : normalizeProfile(summary);
      const baseName = source.name || source.id || 'Custom Agent';
      const id = slug(`${baseName} copy`);
      const profile = normalizeProfile({
        ...source,
        id,
        name: `${baseName} Copy`,
        rel: `System/Agents/Custom/${id}.md`,
        built_in: false,
      });
      makeDraft(profile, 'Cloned custom draft');
      appendTranscriptLine(`Cloned ${baseName} into an unsaved custom draft.`);
    }

    async function runAgentFather(text) {
      const parsed = parseAgentFatherCommand(text);
      if (!parsed.raw) return;
      if (!deterministicAgentFather) return;
      const editorSnapshot = collectEditorSnapshot();
      if (editorSnapshot) state.selected = editorSnapshot;
      appendTranscriptLine(`> ${parsed.raw}`);

      try {
        if (parsed.cmd === '/newagent') {
          const profile = createProfileFromSeed({ name: parsed.arg || 'Custom Agent' });
          makeDraft(profile, 'New AgentFather draft');
          appendTranscriptLine(`Created unsaved draft ${profile.name}.`);
          return;
        }

        if (parsed.cmd === '/cloneagent') {
          await cloneProfile(parsed.arg);
          return;
        }

        if (parsed.cmd === '/testagent') {
          const profile = editorSnapshot || (state.selected ? normalizeProfile(state.selected) : null);
          if (!profile || !profile.id) {
            appendTranscriptLine('Select or create an agent before running /testagent.');
            return;
          }
          state.status = 'Running dry-run test...';
          flushTranscript();
          const result = await invoke('agent_profile_test', { profile, sampleRel: null });
          state.status = 'Dry-run complete';
          appendTranscriptLine(dryRunSummary(result));
          return;
        }

        if (parsed.cmd === '/listagents') {
          const names = enabledAgentNames();
          appendTranscriptLine(names.length ? `Enabled agents: ${names.join(', ')}` : 'No enabled agents found.');
          return;
        }

        const lower = parsed.raw.toLowerCase();
        if (lower.includes('create') && lower.includes('agent')) {
          const profile = createProfileFromSeed(naturalSeed(parsed.raw));
          makeDraft(profile, 'New AgentFather draft');
          appendTranscriptLine(`Created unsaved draft ${profile.name}.`);
          return;
        }

        appendTranscriptLine('Unknown command. Use /newagent, /cloneagent, /testagent, or /listagents.');
      } catch (err) {
        state.error = String(err);
        state.status = '';
        appendTranscriptLine(`AgentFather error: ${String(err)}`);
      } finally {
        const refreshedEditorSnapshot = collectEditorSnapshot();
        if (refreshedEditorSnapshot) state.selected = refreshedEditorSnapshot;
        flushTranscript();
      }
    }

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

    function renderTabs(selected) {
      const tabs = isAgentFatherProfile(selected)
        ? [
            ['create', 'Create Agents'],
            ['edit', 'Edit AgentFather'],
          ]
        : [
            ['overview', 'Overview'],
            ['edit', 'Edit Agent'],
          ];
      if (!tabs.some(([id]) => id === state.activeTab)) state.activeTab = tabs[0][0];
      const bar = document.createElement('div');
      bar.className = 'agent-tabs';
      for (const [id, label] of tabs) {
        const button = document.createElement('button');
        button.type = 'button';
        button.className = 'agent-tab' + (state.activeTab === id ? ' is-active' : '');
        button.textContent = label;
        button.onclick = () => {
          state.activeTab = id;
          renderEditor();
        };
        bar.appendChild(button);
      }
      bar.appendChild(renderDefaultAgentSelector());
      return bar;
    }

    function renderDefaultAgentSelector() {
      const wrap = document.createElement('label');
      wrap.className = 'agent-default-selector';
      const label = document.createElement('span');
      label.textContent = 'Default';
      const select = document.createElement('select');
      const none = document.createElement('option');
      none.value = '';
      none.textContent = 'No default agent';
      select.appendChild(none);
      for (const profile of state.profiles) {
        const option = document.createElement('option');
        option.value = profile.rel;
        option.textContent = profile.name || profile.id || profile.rel;
        select.appendChild(option);
      }
      select.value = selectedDefaultAgentRel();
      select.onchange = () => {
        setDefaultAgentRel(select.value);
        state.status = select.value ? 'Default agent updated' : 'Default agent cleared';
        renderEditor();
      };
      wrap.append(label, select);
      return wrap;
    }

    function renderOverview(selected) {
      const grid = document.createElement('div');
      grid.className = 'agent-overview-grid';

      const summary = document.createElement('section');
      summary.className = 'agent-overview-card';
      summary.innerHTML = '<h3>Profile</h3>';
      const meta = document.createElement('div');
      meta.className = 'agent-meta-row';
      meta.append(
        createChipList([selected.role || 'agent'], 'No role'),
        createChipList([selected.status || 'enabled'], 'No status'),
        createChipList([selected.built_in ? 'built-in' : 'custom'], 'Profile type'),
        createChipList([`v${selected.version || 1}`], 'Version'),
      );
      summary.appendChild(meta);

      const runtime = document.createElement('section');
      runtime.className = 'agent-overview-card';
      runtime.innerHTML = '<h3>Runtime</h3>';
      runtime.appendChild(createChipList([
        selected.runtime.provider === 'global' ? 'Global Default' : selected.runtime.provider,
        selected.runtime.model || 'Global model',
        selected.runtime.mode || 'chat',
      ]));

      const skills = document.createElement('section');
      skills.className = 'agent-overview-card';
      skills.innerHTML = '<h3>Skills</h3>';
      skills.appendChild(createChipList(selected.skills));

      const tools = document.createElement('section');
      tools.className = 'agent-overview-card';
      tools.innerHTML = '<h3>Tools</h3>';
      tools.appendChild(createChipList(selected.tools));

      const access = document.createElement('section');
      access.className = 'agent-overview-card';
      access.innerHTML = '<h3>Access</h3>';
      access.append(
        createField('Read', createChipList(selected.access.read, 'No read access')),
        createField('Write', createChipList(selected.access.write, 'No write access')),
        createField('Denied', createChipList(selected.access.denied, 'No denied scopes')),
      );

      const run = document.createElement('section');
      run.className = 'agent-overview-card agent-run-box';
      run.innerHTML = '<h3>Run Agent</h3>';
      const prompt = document.createElement('textarea');
      prompt.placeholder = 'Ask this agent to respond using its profile...';
      prompt.value = state.runInput || '';
      prompt.oninput = () => { state.runInput = prompt.value; };
      const runButton = document.createElement('button');
      runButton.type = 'button';
      const assignedModel = selected.runtime.provider !== 'global' && selected.runtime.model;
      runButton.textContent = state.runBusy
        ? 'Running...'
        : assignedModel ? `Run with ${selected.runtime.model}` : 'Run with Global Model';
      runButton.disabled = state.runBusy || !selected || selected.status === 'disabled';
      const result = document.createElement('div');
      result.className = 'agent-run-result';
      result.textContent = state.runResult || 'No run yet. This uses the current global chat model; per-agent model override is stored but not executed yet.';
      runButton.onclick = () => runAgentProfile(selected, prompt.value);
      run.append(prompt, runButton, result);

      grid.append(summary, runtime, skills, tools, access, run);
      return grid;
    }

    function renderRuntimeSection(selected, readOnly) {
      const section = document.createElement('section');
      section.className = 'agent-editor-section';
      section.setAttribute('data-section', 'runtime');
      const title = document.createElement('h3');
      title.textContent = 'Runtime / Base Model';
      const provider = document.createElement('select');
      provider.disabled = readOnly;
      for (const item of RUNTIME_PROVIDERS) {
        const option = document.createElement('option');
        option.value = item.id;
        option.textContent = item.label;
        provider.appendChild(option);
      }
      provider.value = selected.runtime.provider || 'global';
      const model = createInput(selected.runtime.model, readOnly);
      model.placeholder = 'Global default, or model id such as qwen/qwen3.6-35b-a3b';
      const mode = createInput(selected.runtime.mode || 'chat', readOnly);
      section.append(title, createField('Provider', provider), createField('Model', model), createField('Mode', mode));
      return { section, collect: () => ({ provider: provider.value || 'global', model: model.value.trim(), mode: mode.value.trim() || 'chat' }) };
    }

    async function runAgentProfile(profile, prompt) {
      const text = String(prompt || '').trim();
      if (!text || state.runBusy) return;
      state.runInput = text;
      const runtime = normalizeProfile(profile).runtime;
      const assignedModel = runtime.provider !== 'global' && runtime.model.trim();
      state.runBusy = true;
      state.runResult = assignedModel
        ? `Running with assigned model ${runtime.model}...`
        : 'Running with global chat model...';
      renderEditor();
      try {
        const requestId = `agent-${Date.now()}-${Math.random().toString(16).slice(2)}`;
        const providerAware = assignedModel && ['lmstudio', 'ollama', 'openai', 'openrouter'].includes(runtime.provider);
        const command = providerAware ? 'chat_send_provider' : (assignedModel ? 'chat_send_model' : 'chat_send');
        const payload = {
          requestId,
          messages: [
            { role: 'system', content: profileSystemPrompt(profile) },
            { role: 'user', content: text },
          ],
        };
        if (assignedModel) payload.model = runtime.model;
        if (providerAware) payload.provider = runtime.provider;
        const reply = await invoke(command, payload);
        state.runResult = reply || '(empty response)';
      } catch (err) {
        state.runResult = `Agent run failed: ${String(err)}`;
      } finally {
        state.runBusy = false;
        renderEditor();
      }
    }

    function renderEditor() {
      editor.replaceChildren();
      state.collectEditorProfile = null;

      const selected = state.selected && normalizeProfile(state.selected);
      editor.appendChild(renderTabs(selected));
      const inner = document.createElement('div');
      inner.className = 'agent-editor-inner';
      editor.appendChild(inner);

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
      save.hidden = !selected || state.activeTab !== 'edit';
      const del = document.createElement('button');
      del.type = 'button';
      del.textContent = 'Delete';
      del.className = 'danger';
      del.hidden = !selected || state.activeTab !== 'edit';
      const selectedPersisted = !!selected
        && !!state.selectedRel
        && state.selectedRel === selected.rel
        && state.selectedPersisted
        && state.profiles.some((profile) => profile.rel === selected.rel);
      del.disabled = !selected || selected.built_in || !selectedPersisted;
      actions.append(save, del, status);
      head.append(titleWrap, actions);
      inner.appendChild(head);

      if (state.error) {
        const err = document.createElement('div');
        err.className = 'agent-error';
        err.textContent = state.error;
        inner.appendChild(err);
      }

      if (!selected) {
        const empty = document.createElement('div');
        empty.className = 'agent-empty';
        empty.textContent = state.loading ? 'Loading profiles...' : 'No profile selected.';
        inner.appendChild(empty);
        return;
      }

      if (isAgentFatherProfile(selected) && state.activeTab === 'create') {
        save.disabled = true;
        del.disabled = true;
        inner.appendChild(renderAgentFather());
        return;
      }

      if (!isAgentFatherProfile(selected) && state.activeTab === 'overview') {
        save.disabled = true;
        del.disabled = true;
        inner.appendChild(renderOverview(selected));
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

      const runtimeControls = renderRuntimeSection(selected, readOnly);

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
      presetSelect.value = matchingAccessPresetId(selected.access);
      const readInput = createTextarea(lines(selected.access.read), readOnly, 4);
      const writeInput = createTextarea(lines(selected.access.write), readOnly, 4);
      const deniedInput = createTextarea(lines(selected.access.denied), readOnly, 4);
      const fullProjectWarning = document.createElement('div');
      fullProjectWarning.className = 'agent-full-access-warning';
      fullProjectWarning.textContent = FULL_PROJECT_ACCESS_WARNING;
      const fullProjectAck = document.createElement('label');
      fullProjectAck.className = 'agent-full-access-ack';
      const fullProjectAckInput = document.createElement('input');
      fullProjectAckInput.type = 'checkbox';
      fullProjectAckInput.checked = !!state.fullProjectAccessAcknowledged;
      fullProjectAckInput.disabled = readOnly;
      const fullProjectAckText = document.createElement('span');
      fullProjectAckText.textContent = 'I understand this custom profile will have Full Project Access.';
      fullProjectAck.append(fullProjectAckInput, fullProjectAckText);
      fullProjectAckInput.onchange = () => {
        state.fullProjectAccessAcknowledged = fullProjectAckInput.checked;
      };
      const currentAccess = () => ({
        read: splitLines(readInput.value),
        write: splitLines(writeInput.value),
        denied: splitLines(deniedInput.value),
      });
      const updateFullProjectGuardrail = () => {
        const fullAccess = isFullProjectAccess(currentAccess());
        fullProjectWarning.hidden = !fullAccess;
        fullProjectAck.hidden = readOnly || !fullAccess;
        if (!fullAccess) {
          fullProjectAckInput.checked = false;
          state.fullProjectAccessAcknowledged = false;
        }
      };
      presetSelect.onchange = () => {
        const preset = ACCESS_PRESETS.find((item) => item.id === presetSelect.value);
        if (!preset) return;
        readInput.value = lines(preset.read);
        writeInput.value = lines(preset.write);
        deniedInput.value = lines(preset.denied);
        if (preset.id !== 'full-project') {
          fullProjectAckInput.checked = false;
          state.fullProjectAccessAcknowledged = false;
        }
        updateFullProjectGuardrail();
      };
      readInput.oninput = updateFullProjectGuardrail;
      writeInput.oninput = updateFullProjectGuardrail;
      deniedInput.oninput = updateFullProjectGuardrail;
      const accessGrid = document.createElement('div');
      accessGrid.className = 'agent-three-col';
      accessGrid.append(
        createField('Read', readInput),
        createField('Write', writeInput),
        createField('Denied', deniedInput),
      );
      access.append(accessTitle, createField('Preset', presetSelect), accessGrid);
      access.append(fullProjectWarning);
      if (!readOnly) access.append(fullProjectAck);
      updateFullProjectGuardrail();

      const markdown = document.createElement('section');
      markdown.className = 'agent-editor-section';
      markdown.setAttribute('data-section', 'markdown'); // data-section="markdown"
      const markdownTitle = document.createElement('h3');
      markdownTitle.textContent = 'Markdown Body';
      const bodyInput = createTextarea(selected.body, readOnly, 12);
      markdown.append(markdownTitle, createField('Body', bodyInput));

      form.append(persona, runtimeControls.section, capabilities, access, markdown);
      inner.appendChild(form);

      state.collectEditorProfile = () => {
        const name = nameInput.value.trim();
        const id = selected.id || slug(name);
        return normalizeProfile({
          id,
          name: name || id,
          status: statusSelect.value === 'disabled' ? 'disabled' : 'enabled',
          version: Math.max(1, Number(versionInput.value || 1)),
          role: roleInput.value.trim(),
          runtime: runtimeControls.collect(),
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
          built_in: selected.built_in,
        });
      };

      save.onclick = async () => {
        await saveProfile(state.collectEditorProfile(), fullProjectAckInput.checked);
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
        state.selectedPersisted = false;
        renderList();
        renderEditor();
      }
    }

    async function selectProfile(rel, markUserInteracted) {
      if (!rel) return;
      if (markUserInteracted !== false) state.userInteracted = true;
      const requestId = ++state.selectRequestId;
      const summary = state.profiles.find((profile) => profile.rel === rel);
      state.selectedRel = rel;
      state.selectedPersisted = true;
      state.fullProjectAccessAcknowledged = false;
      state.activeTab = summary && isAgentFatherProfile(summary) ? 'create' : 'overview';
      state.error = '';
      state.status = 'Loading profile...';
      state.selected = summary ? clone(summary) : null;
      renderList();
      renderEditor();

      try {
        const profile = normalizeProfile(await invoke('agent_profile_read', { rel }));
        if (requestId !== state.selectRequestId || rel !== state.selectedRel) return;
        state.selected = profile;
        state.selectedPersisted = true;
        state.status = state.selected.built_in ? 'Built-in profiles are read-only' : 'Ready';
        renderList();
        renderEditor();
      } catch (err) {
        if (requestId !== state.selectRequestId || rel !== state.selectedRel) return;
        state.error = String(err);
        state.status = '';
        state.selectedPersisted = false;
        renderEditor();
      }
    }

    async function saveProfile(profile, fullProjectAccessAcknowledged) {
      if (profile && !profile.built_in && isFullProjectAccess(profile.access) && !fullProjectAccessAcknowledged) {
        state.selected = normalizeProfile(profile);
        state.error = FULL_PROJECT_ACCESS_SAVE_GUARD;
        state.status = '';
        renderEditor();
        return;
      }
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
        state.selectedPersisted = false;
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
      const profile = createProfileFromSeed(seed && Object.keys(seed).length ? seed : {});
      makeDraft(profile, 'New custom profile');
    };

    if (!seed.rel && (seed.name || seed.responsibility || seed.role || seed.skills || seed.tools)) {
      makeDraft(createProfileFromSeed(seed), 'AgentFather seed draft');
    }

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
