// PM intake modal — v1.7. Create/edit an external (client) project,
// optionally prefilled from a Plow (lead tool) opportunity.
//
// Singleton overlay, modeled on worktree-modal.js: IIFE + 'use strict',
// window.xnaut* export, scoped <style>, escape helper, ESC/outside-click
// close. All handlers bound via element.onclick (CSP kills inline attrs).
(function () {
  'use strict';

  const invoke = (...a) => window.__TAURI__.core.invoke(...a);

  function escapeText(s) {
    return String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
  }

  // de-CH grouping: 5'000.00
  function formatChf(n) {
    const v = Number(n);
    if (!isFinite(v)) return '–';
    const neg = v < 0;
    const [int, dec] = Math.abs(v).toFixed(2).split('.');
    return `${neg ? '-' : ''}${int.replace(/\B(?=(\d{3})+(?!\d))/g, "'")}.${dec}`;
  }

  function injectStyles() {
    if (document.getElementById('pm-intake-styles')) return;
    const st = document.createElement('style');
    st.id = 'pm-intake-styles';
    st.textContent = `
.pmi-overlay { position:fixed; inset:0; z-index:1000; display:flex; align-items:center; justify-content:center; background:rgba(0,0,0,.55); }
.pmi-card { display:flex; flex-direction:column; width:92%; max-width:600px; max-height:88vh; overflow:auto; background:var(--editor-surface, #1b1d23); color:var(--text, #d7dae0); border:1px solid var(--border, rgba(255,255,255,.1)); border-radius:var(--radius-md, 8px); box-shadow:0 18px 50px rgba(0,0,0,.5); font-size:13px; }
.pmi-header { display:flex; align-items:center; justify-content:space-between; padding:14px 16px 8px; }
.pmi-title { font-size:15px; font-weight:600; }
.pmi-close { background:transparent; border:none; color:var(--text-muted, #8a8f98); cursor:pointer; padding:2px; display:flex; }
.pmi-close svg { width:14px; height:14px; }
.pmi-close:hover { color:var(--text, #e8eaf0); }
.pmi-body { display:flex; flex-direction:column; gap:6px; padding:4px 16px 12px; }
.pmi-label { font-size:11px; font-weight:600; text-transform:uppercase; letter-spacing:.05em; color:var(--text-muted, #8a8f98); padding-top:8px; }
.pmi-body select, .pmi-body input, .pmi-body textarea { width:100%; box-sizing:border-box; background:var(--input-bg, rgba(255,255,255,.05)); border:1px solid var(--border, rgba(255,255,255,.12)); border-radius:var(--radius-md, 6px); color:inherit; padding:6px 9px; font:inherit; outline:none; resize:vertical; }
.pmi-body select:focus, .pmi-body input:focus, .pmi-body textarea:focus { border-color:var(--accent, #4f8cff); }
.pmi-btn { background:transparent; border:1px solid var(--border, rgba(255,255,255,.16)); border-radius:var(--radius-md, 6px); color:var(--text, #d7dae0); padding:6px 13px; font:inherit; font-size:12px; cursor:pointer; white-space:nowrap; }
.pmi-btn:hover { border-color:var(--text-muted, #8a8f98); }
.pmi-btn:disabled { opacity:.45; cursor:default; }
.pmi-save { background:var(--accent, #4f8cff); border-color:var(--accent, #4f8cff); color:#fff; font-weight:600; }
.pmi-plow { display:flex; flex-direction:column; gap:6px; align-items:flex-start; }
.pmi-chip { display:inline-block; max-width:100%; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; font-size:12px; padding:3px 10px; border-radius:999px; border:1px solid var(--accent, #4f8cff); color:var(--text, #e8eaf0); background:var(--accent-bg, rgba(79,140,255,.12)); }
.pmi-error { font-size:12px; color:#f85149; white-space:pre-wrap; }
.pmi-note { font-size:12px; color:var(--text-muted, #8a8f98); font-style:italic; }
.pmi-opp-list { width:100%; max-height:180px; overflow-y:auto; border:1px solid var(--border, rgba(255,255,255,.12)); border-radius:var(--radius-md, 6px); }
.pmi-opp-row { display:flex; flex-direction:column; gap:1px; padding:6px 9px; cursor:pointer; border-bottom:1px solid var(--border, rgba(255,255,255,.06)); }
.pmi-opp-row:last-child { border-bottom:none; }
.pmi-opp-row:hover { background:var(--accent-bg, rgba(79,140,255,.12)); }
.pmi-opp-name { font-weight:600; }
.pmi-opp-meta { font-size:11px; color:var(--text-muted, #8a8f98); }
.pmi-contacts { display:flex; flex-direction:column; gap:6px; }
.pmi-opp-empty { padding:6px 9px; }
.pmi-contact-row { display:flex; gap:6px; align-items:center; }
.pmi-contact-row input { flex:1 1 0; min-width:0; }
.pmi-c-remove { flex:0 0 auto; background:transparent; border:none; color:var(--text-muted, #8a8f98); cursor:pointer; padding:2px; display:flex; }
.pmi-c-remove svg { width:13px; height:13px; }
.pmi-c-remove:hover { color:#f85149; }
.pmi-add-contact { align-self:flex-start; padding:3px 9px; }
.pmi-grid { display:flex; gap:10px; }
.pmi-grid > div { flex:1 1 0; min-width:0; display:flex; flex-direction:column; gap:6px; }
.pmi-footer { display:flex; align-items:center; gap:8px; padding:10px 16px 14px; border-top:1px solid var(--border, rgba(255,255,255,.08)); }
.pmi-spacer { flex:1 1 auto; }
`;
    document.head.appendChild(st);
  }

  const ICON_X = '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><line x1="4" y1="4" x2="12" y2="12"/><line x1="12" y1="4" x2="4" y2="12"/></svg>';

  let overlay = null;
  const ui = {};
  const state = { ctx: null, plowOppId: '', saving: false };

  function buildModal() {
    if (overlay) return;
    injectStyles();
    overlay = document.createElement('div');
    overlay.className = 'pmi-overlay';
    overlay.hidden = true;
    overlay.innerHTML = `
      <div class="pmi-card" role="dialog" aria-modal="true" aria-label="Project details">
        <div class="pmi-header">
          <span class="pmi-title">Project details</span>
          <button class="pmi-close" aria-label="Close">${ICON_X}</button>
        </div>
        <div class="pmi-body">
          <label class="pmi-label">Link Plow lead</label>
          <div class="pmi-plow">
            <button class="pmi-btn pmi-load-opps">Load opportunities</button>
            <span class="pmi-chip pmi-plow-chip" hidden></span>
            <div class="pmi-error pmi-plow-error" hidden></div>
            <div class="pmi-opp-list" hidden></div>
          </div>
          <label class="pmi-label">Linked xNaut project</label>
          <select class="pmi-task"></select>
          <label class="pmi-label">Client company</label>
          <input class="pmi-company" type="text" placeholder="Acme AG" spellcheck="false">
          <label class="pmi-label">Contacts</label>
          <div class="pmi-contacts"></div>
          <button class="pmi-btn pmi-add-contact">+ Add contact</button>
          <label class="pmi-label">Scope</label>
          <textarea class="pmi-scope" rows="4" placeholder="What is being delivered…"></textarea>
          <div class="pmi-grid">
            <div><label class="pmi-label">Rate CHF/h</label><input class="pmi-rate" type="number" min="0" step="1"></div>
            <div><label class="pmi-label">Offer CHF</label><input class="pmi-offer" type="number" min="0" step="100" placeholder="optional"></div>
          </div>
          <div class="pmi-grid">
            <div><label class="pmi-label">Expected close</label><input class="pmi-close-date" type="date"></div>
            <div><label class="pmi-label">Lineary project id</label><input class="pmi-lineary" type="text" placeholder="optional" spellcheck="false"></div>
          </div>
        </div>
        <div class="pmi-footer">
          <span class="pmi-error pmi-save-error" hidden></span>
          <span class="pmi-spacer"></span>
          <button class="pmi-btn pmi-cancel">Cancel</button>
          <button class="pmi-btn pmi-save">Save</button>
        </div>
      </div>`;
    document.body.appendChild(overlay);

    const q = (sel) => overlay.querySelector(sel);
    ui.loadOpps = q('.pmi-load-opps');
    ui.plowChip = q('.pmi-plow-chip');
    ui.plowError = q('.pmi-plow-error');
    ui.oppList = q('.pmi-opp-list');
    ui.task = q('.pmi-task');
    ui.company = q('.pmi-company');
    ui.contacts = q('.pmi-contacts');
    ui.scope = q('.pmi-scope');
    ui.rate = q('.pmi-rate');
    ui.offer = q('.pmi-offer');
    ui.closeDate = q('.pmi-close-date');
    ui.lineary = q('.pmi-lineary');
    ui.saveError = q('.pmi-save-error');
    ui.save = q('.pmi-save');

    overlay.addEventListener('mousedown', (e) => { if (e.target === overlay) close(); });
    q('.pmi-close').onclick = close;
    q('.pmi-cancel').onclick = close;
    q('.pmi-add-contact').onclick = () => addContactRow();
    ui.loadOpps.onclick = loadOpportunities;
    ui.save.onclick = save;
  }

  function onKeydown(e) {
    if (e.key === 'Escape') { e.preventDefault(); close(); }
  }

  function addContactRow(c) {
    c = c || {};
    const row = document.createElement('div');
    row.className = 'pmi-contact-row';
    row.innerHTML = `
      <input class="pmi-c-name" type="text" placeholder="Name" value="${escapeText(c.name || '')}" spellcheck="false">
      <input class="pmi-c-email" type="text" placeholder="Email" value="${escapeText(c.email || '')}" spellcheck="false">
      <input class="pmi-c-role" type="text" placeholder="Role" value="${escapeText(c.role || '')}" spellcheck="false">
      <button class="pmi-c-remove" title="Remove contact" aria-label="Remove contact">${ICON_X}</button>`;
    row.querySelector('.pmi-c-remove').onclick = () => row.remove();
    ui.contacts.appendChild(row);
  }

  async function loadOpportunities() {
    ui.plowError.hidden = true;
    ui.loadOpps.disabled = true;
    ui.loadOpps.textContent = 'Loading…';
    try {
      const opps = await invoke('plow_list_opportunities');
      ui.oppList.innerHTML = '';
      ui.oppList.hidden = false;
      if (!opps || !opps.length) {
        ui.oppList.innerHTML = '<div class="pmi-note pmi-opp-empty">no open opportunities</div>';
      } else {
        opps.forEach((o) => {
          const row = document.createElement('div');
          row.className = 'pmi-opp-row';
          row.innerHTML = `
            <span class="pmi-opp-name">${escapeText(o.name)}</span>
            <span class="pmi-opp-meta">${escapeText(o.company || '–')} — ${escapeText(o.stage || '–')} — ${formatChf(o.value_chf)} CHF</span>`;
          row.onclick = () => selectOpportunity(o);
          ui.oppList.appendChild(row);
        });
      }
    } catch (e) {
      // e.g. "Plow database not reachable…" — manual entry keeps working.
      ui.plowError.textContent = String(e);
      ui.plowError.hidden = false;
      ui.oppList.hidden = true;
    }
    ui.loadOpps.disabled = false;
    ui.loadOpps.textContent = 'Load opportunities';
  }

  async function selectOpportunity(o) {
    ui.plowError.hidden = true;
    try {
      const opp = (await invoke('plow_get_opportunity', { id: o.id })) || o;
      state.plowOppId = opp.id;
      if (opp.company) ui.company.value = opp.company;
      if (opp.description) ui.scope.value = opp.description;
      if (opp.value_chf != null) ui.offer.value = opp.value_chf;
      if (opp.expected_close_date) ui.closeDate.value = String(opp.expected_close_date).slice(0, 10);
      if (Array.isArray(opp.contacts) && opp.contacts.length) {
        ui.contacts.innerHTML = '';
        opp.contacts.forEach(addContactRow);
      }
      ui.plowChip.hidden = false;
      ui.plowChip.textContent = `Linked: ${opp.name || opp.id}${opp.stage ? ' · ' + opp.stage : ''}`;
      ui.oppList.hidden = true;
    } catch (e) {
      ui.plowError.textContent = String(e);
      ui.plowError.hidden = false;
    }
  }

  function showSaveError(msg) {
    ui.saveError.textContent = msg;
    ui.saveError.hidden = !msg;
  }

  async function openPmIntake(ctx) {
    buildModal();
    state.ctx = ctx || {};
    const ex = state.ctx.existing || null;
    state.plowOppId = (ex && ex.plow_opportunity_id) || '';
    state.saving = false;

    ui.company.value = (ex && ex.client_company) || '';
    ui.scope.value = (ex && ex.scope) || '';
    ui.rate.value = (ex && ex.rate_chf_per_hour) || 180;
    ui.offer.value = (ex && ex.offer_amount_chf) || '';
    ui.closeDate.value = ex && ex.expected_close ? String(ex.expected_close).slice(0, 10) : '';
    ui.lineary.value = (ex && ex.lineary_project_id) || '';
    ui.contacts.innerHTML = '';
    ((ex && ex.contacts) || []).forEach(addContactRow);
    if (!ui.contacts.children.length) addContactRow();
    ui.oppList.hidden = true;
    ui.oppList.innerHTML = '';
    ui.plowError.hidden = true;
    ui.plowChip.hidden = !state.plowOppId;
    if (state.plowOppId) ui.plowChip.textContent = `Linked: ${state.plowOppId}`;
    showSaveError('');
    ui.save.disabled = false;
    ui.save.textContent = 'Save';

    overlay.hidden = false;
    document.addEventListener('keydown', onKeydown, true);

    ui.task.innerHTML = '<option value="">Loading projects…</option>';
    try {
      const tasks = await invoke('tasks_list');
      const projects = (tasks || []).filter((t) => t.kind === 'project');
      ui.task.innerHTML = ['<option value="">— select project —</option>']
        .concat(projects.map((t) =>
          `<option value="${escapeText(t.id)}">${escapeText(t.name || t.path || t.id)}</option>`))
        .join('');
      const want = (ex && ex.task_id) || (state.ctx.task && state.ctx.task.id) || '';
      if (want) ui.task.value = want;
    } catch (e) {
      ui.task.innerHTML = `<option value="">failed: ${escapeText(String(e))}</option>`;
    }
  }

  function close() {
    if (!overlay) return;
    overlay.hidden = true;
    document.removeEventListener('keydown', onKeydown, true);
  }

  async function save() {
    if (state.saving) return;
    const ex = state.ctx.existing || null;
    const taskId = ui.task.value;
    const company = ui.company.value.trim();
    const rate = parseFloat(ui.rate.value);
    if (!taskId) { showSaveError('Select a linked xNaut project.'); return; }
    if (!company) { showSaveError('Client company is required.'); return; }
    if (!isFinite(rate) || rate <= 0) { showSaveError('Rate CHF/h is required.'); return; }

    const contacts = [];
    ui.contacts.querySelectorAll('.pmi-contact-row').forEach((row) => {
      const name = row.querySelector('.pmi-c-name').value.trim();
      const email = row.querySelector('.pmi-c-email').value.trim();
      const role = row.querySelector('.pmi-c-role').value.trim();
      if (name || email || role) contacts.push({ name, email, role });
    });
    const offer = parseFloat(ui.offer.value);

    const project = {
      id: ex ? ex.id : '',
      task_id: taskId,
      client_company: company,
      contacts,
      scope: ui.scope.value.trim(),
      rate_chf_per_hour: rate,
      offer_amount_chf: isFinite(offer) ? offer : 0,
      expected_close: ui.closeDate.value || '',
      plow_opportunity_id: state.plowOppId || '',
      lineary_project_id: ui.lineary.value.trim(),
      created: (ex && ex.created) || '',
    };

    state.saving = true;
    ui.save.disabled = true;
    ui.save.textContent = 'Saving…';
    showSaveError('');
    try {
      const saved = await invoke('pm_save', { project });
      state.saving = false;
      close();
      if (state.ctx.onSaved) state.ctx.onSaved(saved);
    } catch (e) {
      state.saving = false;
      ui.save.disabled = false;
      ui.save.textContent = 'Save';
      showSaveError(String(e));
    }
  }

  // Public API hook.
  window.xnautOpenPmIntake = openPmIntake;
})();
