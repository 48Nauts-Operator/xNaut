// Automations panel — v1.6 scheduled agent runs with precheck + grace.
//
// Architecture mirrors markdown-pane.js: IIFE, window.xnaut* exports,
// pure-DOM pane living inside the parent webview. Backend is the Rust
// automation scheduler (automation_* commands); the "automation://fire"
// Tauri event notifies the UI when a schedule actually fires.
(function () {
  'use strict';

  // Lazy Tauri access — window.__TAURI__ may not exist at parse time.
  const invoke = (...a) => window.__TAURI__.core.invoke(...a);
  const listen = (...a) => window.__TAURI__.event.listen(...a);

  // label -> { kind, label, pane, listEl }
  const panes = new Map();
  let labelCounter = 0;
  const nextLabel = () => `autop-${Date.now().toString(36)}-${++labelCounter}`;

  function escapeText(s) {
    return String(s).replace(/[&<>"']/g, (c) => ({ '&':'&amp;', '<':'&lt;', '>':'&gt;', '"':'&quot;', "'":'&#39;' }[c]));
  }

  const ICONS = {
    clock: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" width="14" height="14"><circle cx="8" cy="8" r="6.5"/><path d="M8 4.5V8l2.5 1.5"/></svg>',
    play: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" width="12" height="12"><path d="M4.5 3l8 5-8 5z"/></svg>',
    pencil: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" width="12" height="12"><path d="M11.5 2.5l2 2L5 13l-2.7.7L3 11z"/></svg>',
    trash: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" width="12" height="12"><path d="M2.5 4.5h11M6.5 4.5V3h3v1.5M4 4.5l.7 9h6.6l.7-9"/></svg>',
    plus: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" width="14" height="14"><line x1="8" y1="3" x2="8" y2="13"/><line x1="3" y1="8" x2="13" y2="8"/></svg>',
    x: '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" width="12" height="12"><line x1="4" y1="4" x2="12" y2="12"/><line x1="12" y1="4" x2="4" y2="12"/></svg>',
  };

  const SCHEDULE_PRESETS = [
    ['hourly', 'Hourly'],
    ['daily@09:00', 'Daily at 09:00'],
    ['weekdays@09:00', 'Weekdays at 09:00'],
    ['every:30m', 'Every 30 min'],
    ['every:2h', 'Every 2 hours'],
  ];

  const TEMPLATES = [
    { tag: 'REPO HEALTH', title: 'Weekday repo audit',
      desc: 'Check dependencies, failing tests, and risky open changes each weekday.',
      preset: { schedule: 'weekdays@09:00', precheck: 'git -C . status --porcelain | head -1' } },
    { tag: 'RELEASE PREP', title: 'Release readiness',
      desc: 'Prepare a weekly release risk summary from the current project state.',
      preset: { schedule: 'weekdays@16:00' } },
    { tag: 'RECURRING REVIEW', title: 'Daily change review',
      desc: 'Scan recent work and call out correctness, UX, and test coverage risks.',
      preset: { schedule: 'daily@18:00' } },
    { tag: 'MAINTENANCE', title: 'Hourly queue check',
      desc: 'Look for stuck work, stale generated files, and failed local validation.',
      preset: { schedule: 'hourly' } },
  ];

  function blankAutomation() {
    return {
      id: '', name: '', prompt: '', precheck: null, precheck_timeout_secs: 60,
      project_path: '', workspace: 'worktree', branch: 'development', agent_id: '',
      session_mode: 'fresh', schedule: 'daily@09:00', grace_hours: 0,
      enabled: true, last_fired: null,
    };
  }

  function relTime(iso) {
    if (!iso) return 'never';
    const t = Date.parse(iso);
    if (isNaN(t)) return iso;
    const s = Math.floor((Date.now() - t) / 1000);
    if (s < 60) return 'just now';
    const m = Math.floor(s / 60);
    if (m < 60) return `${m}m ago`;
    const h = Math.floor(m / 60);
    if (h < 24) return `${h}h ago`;
    return `${Math.floor(h / 24)}d ago`;
  }

  // ---- toasts -------------------------------------------------------------
  function toast(msg) {
    injectStyles();
    let host = document.querySelector('.autop-toasts');
    if (!host) {
      host = document.createElement('div');
      host.className = 'autop-toasts';
      document.body.appendChild(host);
    }
    const t = document.createElement('div');
    t.className = 'autop-toast';
    t.textContent = String(msg);
    host.appendChild(t);
    setTimeout(() => t.remove(), 4000);
  }

  // ---- fire event ---------------------------------------------------------
  let fireWired = false;
  function wireFireListener() {
    if (fireWired) return;
    fireWired = true;
    try {
      listen('automation://fire', (ev) => {
        const a = ev && ev.payload && ev.payload.automation;
        if (!a) return;
        toast(`Automation fired: ${a.name}`);
        window.xnautAutomationFired && window.xnautAutomationFired(a);
      });
    } catch (e) {
      console.warn('[automations] fire listener not attached', e);
    }
  }

  // ---- list ---------------------------------------------------------------
  async function refreshList(entry) {
    let autos = [];
    try {
      autos = (await invoke('automation_list')) || [];
    } catch (e) {
      entry.listEl.innerHTML = `<div class="autop-empty">Failed to load automations: ${escapeText(String(e))}</div>`;
      return;
    }
    entry.listEl.innerHTML = '';
    if (!autos.length) {
      entry.listEl.innerHTML = '<div class="autop-empty">No automations yet — start from a template above.</div>';
      return;
    }
    for (const a of autos) entry.listEl.appendChild(autoRow(entry, a));
  }

  function autoRow(entry, a) {
    const row = document.createElement('div');
    row.className = 'autop-item';
    row.innerHTML = `
      <input type="checkbox" class="autop-toggle" ${a.enabled ? 'checked' : ''} title="Enabled">
      <span class="autop-item-name">${escapeText(a.name)}</span>
      <span class="autop-chip">${escapeText(a.schedule)}</span>
      <span class="autop-fired" title="${escapeText(a.last_fired || 'never fired')}">${escapeText(relTime(a.last_fired))}</span>
      <button class="autop-mini" data-act="run" title="Run now">${ICONS.play}<span>Run now</span></button>
      <button class="autop-mini" data-act="edit" title="Edit" aria-label="Edit">${ICONS.pencil}</button>
      <button class="autop-mini autop-danger" data-act="del" title="Delete" aria-label="Delete">${ICONS.trash}</button>`;
    row.querySelector('.autop-toggle').onchange = async (e) => {
      try {
        await invoke('automation_save', { automation: Object.assign({}, a, { enabled: e.target.checked }) });
      } catch (err) { toast(`Toggle failed: ${err}`); }
      refreshList(entry);
    };
    row.querySelector('[data-act="run"]').onclick = async () => {
      try {
        await invoke('automation_fire_now', { id: a.id });
        toast(`Run started: ${a.name}`);
        refreshList(entry);
      } catch (err) { toast(String(err)); }
    };
    row.querySelector('[data-act="edit"]').onclick = () => openModal(entry, a);
    row.querySelector('[data-act="del"]').onclick = async () => {
      if (!confirm(`Delete automation "${a.name}"?`)) return;
      try {
        await invoke('automation_delete', { id: a.id });
        toast(`Deleted: ${a.name}`);
      } catch (err) { toast(`Delete failed: ${err}`); }
      refreshList(entry);
    };
    return row;
  }

  // ---- modal (singleton) --------------------------------------------------
  let modalEl = null;
  function closeModal() {
    if (modalEl) { modalEl.remove(); modalEl = null; }
  }

  function openModal(entry, auto) {
    closeModal();
    const isCreate = !auto.id;
    const overlay = document.createElement('div');
    overlay.className = 'autop-overlay';
    overlay.innerHTML = `
      <div class="autop-modal" role="dialog" aria-label="Automation editor">
        <div class="autop-modal-head">
          <span>${isCreate ? 'New automation' : 'Edit automation'}</span>
          <button class="autop-mini" data-f="close" aria-label="Close">${ICONS.x}</button>
        </div>
        <div class="autop-modal-body">
          <label class="autop-field">
            <span class="autop-label">Name</span>
            <input class="autop-input" data-f="name" type="text" placeholder="Weekday repo audit">
          </label>
          <label class="autop-field">
            <span class="autop-label">Prompt</span>
            <textarea class="autop-input" data-f="prompt" rows="6"></textarea>
            <span class="autop-hint">Supports skills, file paths, and built-in commands</span>
          </label>
          <div class="autop-frow">
            <label class="autop-field autop-grow">
              <span class="autop-label">Precheck</span>
              <input class="autop-input autop-mono" data-f="precheck" type="text" placeholder="exit 0 to run, non-zero to skip">
            </label>
            <label class="autop-field">
              <span class="autop-label">Timeout</span>
              <select class="autop-input" data-f="timeout">
                <option value="30">30s</option><option value="60">1m</option><option value="300">5m</option>
              </select>
            </label>
          </div>
          <label class="autop-field">
            <span class="autop-label">Project</span>
            <select class="autop-input" data-f="project"></select>
            <input class="autop-input autop-mono" data-f="project-custom" type="text" placeholder="/absolute/path/to/project" hidden>
          </label>
          <div class="autop-frow">
            <div class="autop-field">
              <span class="autop-label">Workspace</span>
              <div class="autop-seg" data-f="workspace">
                <button type="button" data-v="worktree">Worktree</button>
                <button type="button" data-v="new_run">New run</button>
              </div>
            </div>
            <label class="autop-field autop-grow" data-f="branch-wrap">
              <span class="autop-label">Branch</span>
              <input class="autop-input" data-f="branch" type="text">
            </label>
          </div>
          <div class="autop-frow">
            <label class="autop-field autop-grow">
              <span class="autop-label">Agent</span>
              <select class="autop-input" data-f="agent"></select>
            </label>
            <div class="autop-field">
              <span class="autop-label">Session</span>
              <div class="autop-seg" data-f="session">
                <button type="button" data-v="fresh">Fresh</button>
                <button type="button" data-v="reuse">Reuse</button>
              </div>
            </div>
          </div>
          <div class="autop-frow">
            <label class="autop-field autop-grow">
              <span class="autop-label">Schedule</span>
              <select class="autop-input" data-f="schedule"></select>
              <input class="autop-input autop-mono" data-f="schedule-custom" type="text" placeholder="e.g. weekdays@07:30" hidden>
            </label>
            <label class="autop-field">
              <span class="autop-label">Grace</span>
              <select class="autop-input" data-f="grace">
                <option value="0">None</option><option value="6">6 hours</option>
                <option value="12">12 hours</option><option value="24">24 hours</option>
              </select>
            </label>
          </div>
        </div>
        <div class="autop-modal-foot">
          <button class="autop-btn" data-f="cancel">Cancel</button>
          <button class="autop-btn autop-primary" data-f="save">${isCreate ? '+ Create' : 'Save'}</button>
        </div>
      </div>`;
    document.body.appendChild(overlay);
    modalEl = overlay;

    const q = (f) => overlay.querySelector(`[data-f="${f}"]`);
    overlay.addEventListener('mousedown', (e) => { if (e.target === overlay) closeModal(); });
    q('close').onclick = closeModal;
    q('cancel').onclick = closeModal;

    // Segmented controls.
    function seg(name, val, onChange) {
      const box = q(name);
      const set = (v) => {
        box.dataset.value = v;
        box.querySelectorAll('button').forEach((b) => b.classList.toggle('autop-seg-on', b.dataset.v === v));
        if (onChange) onChange(v);
      };
      box.querySelectorAll('button').forEach((b) => { b.onclick = () => set(b.dataset.v); });
      set(val);
      return () => box.dataset.value;
    }
    const getWorkspace = seg('workspace', auto.workspace || 'worktree', (v) => {
      q('branch-wrap').hidden = v !== 'new_run';
    });
    const getSession = seg('session', auto.session_mode || 'fresh');

    // Plain values.
    q('name').value = auto.name || '';
    q('prompt').value = auto.prompt || '';
    q('precheck').value = auto.precheck || '';
    q('branch').value = auto.branch || 'development';
    const ensureOption = (sel, v, label) => {
      if (![...sel.options].some((o) => o.value === String(v))) {
        const o = document.createElement('option');
        o.value = String(v); o.textContent = label;
        sel.appendChild(o);
      }
      sel.value = String(v);
    };
    ensureOption(q('timeout'), auto.precheck_timeout_secs || 60, `${auto.precheck_timeout_secs}s`);
    ensureOption(q('grace'), auto.grace_hours || 0, `${auto.grace_hours} hours`);

    // Schedule select with custom escape hatch.
    const schedSel = q('schedule');
    const schedCustom = q('schedule-custom');
    schedSel.innerHTML = SCHEDULE_PRESETS.map(([v, l]) => `<option value="${escapeText(v)}">${escapeText(l)}</option>`).join('')
      + '<option value="__custom">Custom…</option>';
    const sched = auto.schedule || 'daily@09:00';
    if (SCHEDULE_PRESETS.some(([v]) => v === sched)) {
      schedSel.value = sched;
    } else {
      schedSel.value = '__custom';
      schedCustom.hidden = false;
      schedCustom.value = sched;
    }
    schedSel.onchange = () => { schedCustom.hidden = schedSel.value !== '__custom'; };

    // Project dropdown from tasks_list (kind=project) + manual path option.
    const projSel = q('project');
    const projCustom = q('project-custom');
    projSel.innerHTML = '<option value="">Loading projects…</option>';
    invoke('tasks_list').then((items) => {
      const projects = (items || []).filter((t) => t && t.kind === 'project');
      projSel.innerHTML = projects.map((p) =>
        `<option value="${escapeText(p.path)}">${escapeText(p.name)}</option>`).join('')
        + '<option value="__custom">Custom path…</option>';
      const cur = auto.project_path || '';
      if (cur && projects.some((p) => p.path === cur)) {
        projSel.value = cur;
      } else if (cur) {
        projSel.value = '__custom';
        projCustom.hidden = false;
        projCustom.value = cur;
      } else if (projects.length) {
        projSel.value = projects[0].path;
      } else {
        projSel.value = '__custom';
        projCustom.hidden = false;
      }
    }).catch((e) => {
      projSel.innerHTML = '<option value="__custom">Custom path…</option>';
      projCustom.hidden = false;
      console.warn('[automations] tasks_list failed', e);
    });
    projSel.onchange = () => { projCustom.hidden = projSel.value !== '__custom'; };

    // Agent dropdown.
    const agentSel = q('agent');
    agentSel.innerHTML = '<option value="">Loading agents…</option>';
    invoke('agent_list').then((agents) => {
      agents = agents || [];
      agentSel.innerHTML = agents.map((a) =>
        `<option value="${escapeText(a.id)}"${a.available ? '' : ' disabled'}>${escapeText(a.label)}${a.available ? '' : ' (unavailable)'}</option>`).join('');
      const firstAvail = agents.find((a) => a.available);
      agentSel.value = auto.agent_id || (firstAvail ? firstAvail.id : '');
    }).catch((e) => {
      agentSel.innerHTML = '<option value="">No agents</option>';
      console.warn('[automations] agent_list failed', e);
    });

    q('save').onclick = async () => {
      const name = q('name').value.trim();
      if (!name) { toast('Name is required'); return; }
      const schedule = schedSel.value === '__custom' ? schedCustom.value.trim() : schedSel.value;
      if (!schedule) { toast('Schedule is required'); return; }
      const project_path = projSel.value === '__custom' ? projCustom.value.trim() : projSel.value;
      const automation = {
        id: auto.id || '',
        name,
        prompt: q('prompt').value,
        precheck: q('precheck').value.trim() || null,
        precheck_timeout_secs: Number(q('timeout').value),
        project_path,
        workspace: getWorkspace(),
        branch: q('branch').value.trim() || 'development',
        agent_id: agentSel.value,
        session_mode: getSession(),
        schedule,
        grace_hours: Number(q('grace').value),
        enabled: auto.enabled !== false,
        last_fired: auto.last_fired || null,
      };
      try {
        await invoke('automation_save', { automation });
        closeModal();
        toast(isCreate ? `Automation created: ${name}` : `Saved: ${name}`);
        refreshList(entry);
      } catch (e) {
        toast(`Save failed: ${e}`);
      }
    };
  }

  // ---- panel --------------------------------------------------------------
  function createAutomationsPanel(tabId, parentContainer, opts) {
    opts = opts || {};
    injectStyles();
    wireFireListener();
    const label = nextLabel();

    const pane = document.createElement('div');
    pane.className = 'autop-pane';
    pane.dataset.automationsLabel = label;
    pane.innerHTML = `
      <div class="autop-head">${ICONS.clock}<span>Automations</span>
        <span class="autop-sub">Scheduled agent runs with precheck + grace</span></div>
      <div class="autop-scroll">
        <div class="autop-section">Start from a template</div>
        <div class="autop-grid"></div>
        <button class="autop-addnew">${ICONS.plus}<span>Add new</span></button>
        <div class="autop-section">Your automations</div>
        <div class="autop-list"><div class="autop-empty">Loading…</div></div>
      </div>`;
    parentContainer.appendChild(pane);

    const entry = { kind: 'automations', label, pane, listEl: pane.querySelector('.autop-list') };
    panes.set(label, entry);

    const grid = pane.querySelector('.autop-grid');
    for (const t of TEMPLATES) {
      const card = document.createElement('button');
      card.className = 'autop-card';
      card.innerHTML = `
        <span class="autop-tag">${escapeText(t.tag)}</span>
        <span class="autop-card-title">${escapeText(t.title)}</span>
        <span class="autop-card-desc">${escapeText(t.desc)}</span>`;
      card.onclick = () => openModal(entry, Object.assign(blankAutomation(), {
        name: t.title, prompt: t.desc,
      }, t.preset));
      grid.appendChild(card);
    }
    pane.querySelector('.autop-addnew').onclick = () => openModal(entry, blankAutomation());

    refreshList(entry);
    return entry;
  }

  function destroyAutomationsPanel(label) {
    const entry = panes.get(label);
    if (!entry) return;
    closeModal();
    if (entry.pane && entry.pane.parentNode) entry.pane.parentNode.removeChild(entry.pane);
    panes.delete(label);
  }

  // ---- styles -------------------------------------------------------------
  function injectStyles() {
    if (document.getElementById('automations-panel-styles')) return;
    const st = document.createElement('style');
    st.id = 'automations-panel-styles';
    st.textContent = `
.autop-pane { display:flex; flex-direction:column; flex:1 1 0%; width:100%; height:100%; min-width:0; min-height:0; overflow:hidden; background:var(--editor-surface, #17181c); color:var(--text-primary, #d8dade); border-radius:var(--radius-md, 8px); font-size:13px; }
.autop-head { display:flex; align-items:center; gap:8px; padding:10px 14px; border-bottom:1px solid var(--border-color, #2a2c33); font-weight:600; }
.autop-head .autop-sub { font-weight:400; opacity:.55; font-size:12px; }
.autop-scroll { flex:1; overflow-y:auto; padding:14px; }
.autop-section { font-size:11px; text-transform:uppercase; letter-spacing:.08em; opacity:.55; padding:14px 2px 8px; }
.autop-grid { display:grid; grid-template-columns:repeat(auto-fill, minmax(220px, 1fr)); gap:10px; }
.autop-card, .autop-addnew { text-align:left; cursor:pointer; border:1px solid var(--border-color, #2a2c33); border-radius:var(--radius-md, 8px); background:var(--surface-raised, rgba(255,255,255,.03)); color:inherit; padding:12px; font:inherit; }
.autop-card { display:flex; flex-direction:column; gap:6px; }
.autop-card:hover, .autop-addnew:hover { border-color:var(--accent, #5b8af5); }
.autop-tag { font-size:10px; letter-spacing:.08em; font-weight:700; color:var(--accent, #5b8af5); }
.autop-card-title { font-weight:600; }
.autop-card-desc { font-size:12px; opacity:.65; line-height:1.4; }
.autop-addnew { display:flex; align-items:center; justify-content:center; gap:6px; width:100%; margin-top:10px; border-style:dashed; opacity:.8; }
.autop-list { display:flex; flex-direction:column; gap:6px; }
.autop-empty { opacity:.5; padding:8px 2px; font-size:12px; }
.autop-item { display:flex; align-items:center; gap:10px; padding:8px 10px; border:1px solid var(--border-color, #2a2c33); border-radius:var(--radius-md, 8px); background:var(--surface-raised, rgba(255,255,255,.02)); }
.autop-item-name { font-weight:600; flex:1; min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.autop-chip { font-size:11px; font-family:var(--mono-font, ui-monospace, monospace); padding:2px 8px; border-radius:99px; background:var(--chip-bg, rgba(255,255,255,.07)); white-space:nowrap; }
.autop-fired { font-size:11px; opacity:.55; white-space:nowrap; }
.autop-toggle { accent-color:var(--accent, #5b8af5); cursor:pointer; }
.autop-mini { display:inline-flex; align-items:center; gap:5px; cursor:pointer; border:1px solid var(--border-color, #2a2c33); background:transparent; color:inherit; border-radius:6px; padding:4px 8px; font-size:11px; }
.autop-mini:hover { border-color:var(--accent, #5b8af5); }
.autop-danger:hover { border-color:var(--danger, #e5534b); color:var(--danger, #e5534b); }
.autop-overlay { position:fixed; inset:0; z-index:1000; display:flex; align-items:center; justify-content:center; background:rgba(0,0,0,.55); }
.autop-modal { display:flex; flex-direction:column; width:min(560px, calc(100vw - 48px)); max-height:calc(100vh - 64px); background:var(--editor-surface, #1c1d22); color:var(--text-primary, #d8dade); border:1px solid var(--border-color, #2a2c33); border-radius:var(--radius-md, 10px); box-shadow:0 16px 48px rgba(0,0,0,.5); font-size:13px; }
.autop-modal-head { display:flex; align-items:center; justify-content:space-between; padding:12px 16px; border-bottom:1px solid var(--border-color, #2a2c33); font-weight:600; }
.autop-modal-body { flex:1; overflow-y:auto; display:flex; flex-direction:column; gap:12px; padding:16px; }
.autop-modal-foot { display:flex; justify-content:flex-end; gap:8px; padding:12px 16px; border-top:1px solid var(--border-color, #2a2c33); }
.autop-field { display:flex; flex-direction:column; gap:5px; min-width:0; }
.autop-frow { display:flex; gap:10px; align-items:flex-end; }
.autop-grow { flex:1; }
.autop-label { font-size:11px; text-transform:uppercase; letter-spacing:.06em; opacity:.6; }
.autop-hint { font-size:11px; opacity:.45; }
.autop-input { font:inherit; color:inherit; background:var(--input-bg, rgba(255,255,255,.05)); border:1px solid var(--border-color, #2a2c33); border-radius:6px; padding:7px 9px; min-width:0; }
.autop-input:focus { outline:none; border-color:var(--accent, #5b8af5); }
.autop-mono { font-family:var(--mono-font, ui-monospace, monospace); font-size:12px; }
textarea.autop-input { resize:vertical; }
.autop-seg { display:inline-flex; border:1px solid var(--border-color, #2a2c33); border-radius:6px; overflow:hidden; }
.autop-seg button { font:inherit; font-size:12px; color:inherit; background:transparent; border:none; padding:7px 12px; cursor:pointer; opacity:.65; }
.autop-seg button.autop-seg-on { background:var(--accent, #5b8af5); color:var(--accent-foreground,#fff); opacity:1; }
.autop-btn { font:inherit; color:inherit; background:transparent; border:1px solid var(--border-color, #2a2c33); border-radius:6px; padding:7px 14px; cursor:pointer; }
.autop-btn:hover { border-color:var(--accent, #5b8af5); }
.autop-primary { background:var(--accent, #5b8af5); border-color:var(--accent, #5b8af5); color:var(--accent-foreground,#fff); font-weight:600; }
.autop-toasts { position:fixed; right:16px; bottom:16px; z-index:1100; display:flex; flex-direction:column; gap:8px; align-items:flex-end; }
.autop-toast { background:var(--editor-surface, #23242a); color:var(--text-primary, #e2e4e8); border:1px solid var(--border-color, #34363e); border-radius:8px; padding:9px 14px; font-size:12px; max-width:360px; box-shadow:0 6px 20px rgba(0,0,0,.4); }
[hidden] { display:none !important; }
`;
    document.head.appendChild(st);
  }

  // Public API hooks (mirror the markdown-pane shape).
  window.xnautCreateAutomationsPanel = createAutomationsPanel;
  window.xnautDestroyAutomationsPanel = destroyAutomationsPanel;
})();
