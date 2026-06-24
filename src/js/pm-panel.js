// PM Space panel — v1.7. Lists external (client) projects with
// worklog-derived financials. List view (cards) + detail view
// (overview / financials / documents / actions).
//
// Same architecture as markdown-pane.js / tasks-panel.js: IIFE +
// 'use strict', window.xnaut* exports, scoped <style>, escape helper.
// All handlers bound via element.onclick (CSP kills inline attributes).
(function () {
  'use strict';

  const invoke = (...a) => window.__TAURI__.core.invoke(...a);

  // label -> { pane }
  const panes = new Map();
  let labelCounter = 0;
  function nextLabel() {
    labelCounter += 1;
    return `pm-${Date.now().toString(36)}-${labelCounter}`;
  }

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

  const ICON_BACK = '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M10 3 5 8l5 5"/></svg>';
  const ICON_REFRESH = '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M13 8a5 5 0 1 1-1.5-3.6"/><path d="M13 2v3h-3"/></svg>';
  const ICON_DOC = '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.2"><path d="M4 1.5h5l3 3v10H4z"/><path d="M9 1.5v3h3"/></svg>';

  function injectStyles() {
    if (document.getElementById('pm-panel-styles')) return;
    const st = document.createElement('style');
    st.id = 'pm-panel-styles';
    st.textContent = `
.pmp-pane { position:relative; display:flex; flex-direction:column; flex:1 1 0%; width:100%; height:100%; min-width:0; min-height:0; overflow:hidden; background:var(--editor-surface, #1b1d23); color:var(--text, #d7dae0); border-radius:var(--radius-md, 8px); font-size:13px; }
.pmp-header { display:flex; align-items:center; justify-content:space-between; gap:8px; padding:12px 16px; border-bottom:1px solid var(--border, rgba(255,255,255,.08)); }
.pmp-title { font-size:15px; font-weight:600; }
.pmp-content { flex:1 1 auto; min-height:0; overflow-y:auto; padding:12px 16px; display:flex; flex-direction:column; gap:10px; }
.pmp-btn { background:transparent; border:1px solid var(--border, rgba(255,255,255,.16)); border-radius:var(--radius-md, 6px); color:var(--text, #d7dae0); padding:5px 11px; font:inherit; font-size:12px; cursor:pointer; white-space:nowrap; }
.pmp-btn:hover { border-color:var(--text-muted, #8a8f98); }
.pmp-btn:disabled { opacity:.45; cursor:default; }
.pmp-btn-primary { background:var(--accent, #4f8cff); border-color:var(--accent, #4f8cff); color:#fff; font-weight:600; }
.pmp-btn-danger { color:#f85149; border-color:rgba(248,81,73,.4); }
.pmp-btn-danger:hover { border-color:#f85149; }
.pmp-icon-btn { background:transparent; border:none; color:var(--text-muted, #8a8f98); cursor:pointer; padding:2px; display:flex; }
.pmp-icon-btn svg { width:14px; height:14px; }
.pmp-icon-btn:hover { color:var(--text, #e8eaf0); }
.pmp-muted { font-size:12px; color:var(--text-muted, #8a8f98); }
.pmp-error { font-size:12px; color:#f85149; white-space:pre-wrap; }
.pmp-card { display:flex; flex-direction:column; gap:6px; padding:10px 12px; border:1px solid var(--border, rgba(255,255,255,.1)); border-radius:var(--radius-md, 8px); cursor:pointer; }
.pmp-card:hover { border-color:var(--accent, #4f8cff); }
.pmp-card-client { font-size:14px; font-weight:600; }
.pmp-card-task { font-size:12px; color:var(--text-muted, #8a8f98); }
.pmp-chips { display:flex; align-items:center; gap:6px; flex-wrap:wrap; }
.pmp-chip { display:inline-block; max-width:100%; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; font-size:11px; padding:2px 9px; border-radius:999px; border:1px solid var(--border, rgba(255,255,255,.16)); color:var(--text-muted, #9aa0aa); }
.pmp-chip-over { color:#f85149; border-color:rgba(248,81,73,.5); }
.pmp-empty { display:flex; flex-direction:column; align-items:flex-start; gap:10px; padding:28px 8px; max-width:440px; }
.pmp-empty-title { font-size:14px; font-weight:600; }
.pmp-back { align-self:flex-start; display:flex; align-items:center; gap:5px; background:transparent; border:none; color:var(--text-muted, #8a8f98); cursor:pointer; padding:2px 4px; font:inherit; font-size:12px; }
.pmp-back svg { width:13px; height:13px; }
.pmp-back:hover { color:var(--text, #e8eaf0); }
.pmp-section { display:flex; flex-direction:column; gap:8px; padding:12px 14px; border:1px solid var(--border, rgba(255,255,255,.1)); border-radius:var(--radius-md, 8px); }
.pmp-section-head { display:flex; align-items:center; justify-content:space-between; gap:8px; }
.pmp-section-title { font-size:11px; font-weight:600; text-transform:uppercase; letter-spacing:.05em; color:var(--text-muted, #8a8f98); }
.pmp-detail-client { font-size:15px; font-weight:600; }
.pmp-contact { font-size:12px; }
.pmp-scope { font-size:12px; line-height:1.5; white-space:pre-wrap; }
.pmp-link { color:var(--accent, #4f8cff); text-decoration:none; }
.pmp-link:hover { text-decoration:underline; }
.pmp-row { display:flex; align-items:center; gap:10px; flex-wrap:wrap; }
.pmp-spacer { flex:1 1 auto; }
.pmp-bignums { display:flex; gap:22px; flex-wrap:wrap; }
.pmp-bignum-val { font-size:18px; font-weight:600; }
.pmp-bignum-label { font-size:10px; text-transform:uppercase; letter-spacing:.05em; color:var(--text-muted, #8a8f98); }
.pmp-pos { color:#3fb950; }
.pmp-neg { color:#f85149; }
.pmp-bar { height:8px; border-radius:999px; background:var(--input-bg, rgba(255,255,255,.08)); overflow:hidden; }
.pmp-bar-fill { height:100%; border-radius:999px; background:var(--accent, #4f8cff); }
.pmp-bar-fill.pmp-neg-bg { background:#f85149; }
.pmp-doc-row { display:flex; align-items:center; gap:8px; padding:5px 6px; border-radius:4px; font-size:12px; }
.pmp-doc-row svg { width:13px; height:13px; flex:0 0 auto; color:var(--text-muted, #8a8f98); }
.pmp-doc-md { cursor:pointer; }
.pmp-doc-md:hover { background:var(--input-bg, rgba(255,255,255,.06)); }
.pmp-toast { position:absolute; bottom:14px; left:50%; transform:translateX(-50%); max-width:80%; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; background:var(--editor-surface, #2a2d35); border:1px solid var(--border, rgba(255,255,255,.16)); border-radius:var(--radius-md, 6px); padding:7px 14px; font-size:12px; box-shadow:0 8px 24px rgba(0,0,0,.45); z-index:10; }
`;
    document.head.appendChild(st);
  }

  function createPmPanel(tabId, parentContainer, opts) {
    opts = opts || {};
    injectStyles();
    const label = nextLabel();

    const pane = document.createElement('div');
    pane.className = 'pmp-pane';
    pane.dataset.pmLabel = label;

    const header = document.createElement('div');
    header.className = 'pmp-header';
    header.innerHTML = `
      <span class="pmp-title">PM — Client Projects</span>
      <button class="pmp-btn pmp-btn-primary pmp-new">+ New external project</button>`;
    pane.appendChild(header);

    const content = document.createElement('div');
    content.className = 'pmp-content';
    pane.appendChild(content);
    parentContainer.appendChild(pane);

    const state = { tasksById: new Map() };

    function toast(msg) {
      const t = document.createElement('div');
      t.className = 'pmp-toast';
      t.textContent = msg;
      pane.appendChild(t);
      setTimeout(() => { if (t.parentNode) t.parentNode.removeChild(t); }, 3500);
    }

    function openIntakeForNew() {
      if (window.xnautOpenPmIntake) window.xnautOpenPmIntake({ onSaved: () => renderList() });
    }
    header.querySelector('.pmp-new').onclick = openIntakeForNew;

    function taskName(taskId) {
      const t = state.tasksById.get(taskId);
      return t ? (t.name || t.path || taskId) : (taskId || '—');
    }

    async function renderList() {
      content.innerHTML = '<div class="pmp-muted">Loading…</div>';
      let projects, tasks;
      try {
        [projects, tasks] = await Promise.all([invoke('pm_list'), invoke('tasks_list')]);
      } catch (e) {
        content.innerHTML = `<div class="pmp-error">${escapeText(String(e))}</div>`;
        return;
      }
      state.tasksById = new Map((tasks || []).map((t) => [t.id, t]));
      content.innerHTML = '';

      if (!projects || !projects.length) {
        content.innerHTML = `
          <div class="pmp-empty">
            <div class="pmp-empty-title">No client projects yet</div>
            <div class="pmp-muted">Link an xNaut project to a client engagement to track hours burned against the offer, generate client documents, and keep contacts in one place.</div>
            <button class="pmp-btn pmp-btn-primary pmp-empty-new">+ New external project</button>
          </div>`;
        content.querySelector('.pmp-empty-new').onclick = openIntakeForNew;
        return;
      }

      projects.forEach((p) => {
        const card = document.createElement('div');
        card.className = 'pmp-card';
        card.innerHTML = `
          <div class="pmp-card-client">${escapeText(p.client_company)}</div>
          <div class="pmp-card-task">${escapeText(taskName(p.task_id))}</div>
          <div class="pmp-chips">
            <span class="pmp-chip pmp-chip-stage">–</span>
            <span class="pmp-chip">offer ${formatChf(p.offer_amount_chf)} CHF</span>
            <span class="pmp-chip pmp-chip-burn">burn –</span>
          </div>`;
        card.onclick = () => renderDetail(p);
        content.appendChild(card);

        const stageChip = card.querySelector('.pmp-chip-stage');
        if (p.plow_opportunity_id) {
          invoke('plow_status', { id: p.plow_opportunity_id })
            .then((s) => { if (s && s.stage) stageChip.textContent = s.stage; })
            .catch(() => {}); // Plow unreachable — leave the gray "–"
        }
        const burnChip = card.querySelector('.pmp-chip-burn');
        invoke('pm_financials', { taskId: p.task_id })
          .then((f) => {
            burnChip.textContent = `burn ${formatChf(f.burn_chf)} CHF`;
            if (Number(f.burn_chf) > Number(f.offer_chf)) burnChip.classList.add('pmp-chip-over');
          })
          .catch(() => {});
      });
    }

    function renderDetail(project) {
      const task = state.tasksById.get(project.task_id);
      const contactsHtml = (project.contacts || []).map((c) => {
        const bits = [escapeText(c.name || '')];
        if (c.role) bits.push(escapeText(c.role));
        if (c.email) bits.push(`<a class="pmp-link" href="mailto:${escapeText(c.email)}">${escapeText(c.email)}</a>`);
        return `<div class="pmp-contact">${bits.join(' — ')}</div>`;
      }).join('') || '<div class="pmp-muted">no contacts</div>';

      content.innerHTML = `
        <button class="pmp-back">${ICON_BACK}<span>Back</span></button>
        <div class="pmp-section">
          <div class="pmp-section-title">Overview</div>
          <div class="pmp-detail-client">${escapeText(project.client_company)}</div>
          ${contactsHtml}
          ${project.scope ? `<div class="pmp-scope">${escapeText(project.scope)}</div>` : ''}
          <div class="pmp-row pmp-muted">
            <span>Rate: ${formatChf(project.rate_chf_per_hour)} CHF/h</span>
            ${project.expected_close ? `<span>Expected close: ${escapeText(project.expected_close)}</span>` : ''}
          </div>
          <div class="pmp-row">
            ${project.plow_opportunity_id ? '<span class="pmp-chip pmp-plow-chip">Plow · –</span>' : ''}
            ${project.lineary_project_id ? `<span class="pmp-chip">Lineary ${escapeText(project.lineary_project_id)}</span>` : ''}
            ${task && task.path ? `<span class="pmp-chip" title="${escapeText(task.path)}">${escapeText(task.path)}</span>` : ''}
          </div>
        </div>
        <div class="pmp-section">
          <div class="pmp-section-head">
            <span class="pmp-section-title">Financials</span>
            <button class="pmp-icon-btn pmp-fin-refresh" title="Refresh financials" aria-label="Refresh financials">${ICON_REFRESH}</button>
          </div>
          <div class="pmp-fin-body pmp-muted">Loading…</div>
        </div>
        <div class="pmp-section">
          <div class="pmp-section-title">Documents</div>
          <div class="pmp-docs pmp-muted">Loading…</div>
          <div class="pmp-row pmp-gen-btns"></div>
          <div class="pmp-error pmp-gen-error" hidden></div>
        </div>
        <div class="pmp-row">
          <button class="pmp-btn pmp-act-chat">Open project chat</button>
          <button class="pmp-btn pmp-act-edit">Edit details</button>
          <span class="pmp-spacer"></span>
          <button class="pmp-btn pmp-btn-danger pmp-act-remove">Remove from PM</button>
        </div>`;

      const q = (sel) => content.querySelector(sel);
      q('.pmp-back').onclick = () => renderList();

      const plowChip = q('.pmp-plow-chip');
      if (plowChip) {
        invoke('plow_status', { id: project.plow_opportunity_id })
          .then((s) => { plowChip.textContent = `Plow · ${(s && s.stage) || '–'}`; })
          .catch(() => { plowChip.textContent = 'Plow · unreachable'; });
      }

      const finBody = q('.pmp-fin-body');
      const bignum = (val, lab, cls) =>
        `<div class="pmp-bignum"><div class="pmp-bignum-val ${cls || ''}">${val}</div><div class="pmp-bignum-label">${lab}</div></div>`;
      async function loadFinancials() {
        finBody.className = 'pmp-fin-body pmp-muted';
        finBody.textContent = 'Loading…';
        try {
          const f = await invoke('pm_financials', { taskId: project.task_id });
          const burn = Number(f.burn_chf) || 0;
          const offer = Number(f.offer_chf) || 0;
          const margin = Number(f.margin_chf) || 0;
          const over = burn > offer;
          finBody.className = 'pmp-fin-body';
          finBody.innerHTML = `
            <div class="pmp-bignums">
              ${bignum((Number(f.hours) || 0).toFixed(1), 'Hours')}
              ${bignum(formatChf(burn), 'Burn CHF', over ? 'pmp-neg' : '')}
              ${bignum(formatChf(offer), 'Offer CHF')}
              ${bignum(formatChf(margin), 'Margin CHF', margin >= 0 ? 'pmp-pos' : 'pmp-neg')}
            </div>
            <div class="pmp-bar"><div class="pmp-bar-fill"></div></div>
            <div class="pmp-muted">burn vs offer · ${f.sessions || 0} sessions · effective ${formatChf(f.effective_rate_chf)} CHF/h</div>`;
          const fill = finBody.querySelector('.pmp-bar-fill');
          const pct = offer > 0 ? Math.min(100, (burn / offer) * 100) : (burn > 0 ? 100 : 0);
          fill.style.width = pct + '%';
          if (over) fill.classList.add('pmp-neg-bg');
        } catch (e) {
          finBody.className = 'pmp-fin-body pmp-error';
          finBody.textContent = String(e);
        }
      }
      q('.pmp-fin-refresh').onclick = loadFinancials;

      const docsEl = q('.pmp-docs');
      const clientDir = task && task.path ? `${task.path}/client` : null;
      async function loadDocs() {
        if (!clientDir) {
          docsEl.className = 'pmp-docs pmp-muted';
          docsEl.textContent = 'no linked project path';
          return;
        }
        let entries = [];
        try {
          const res = await invoke('list_directory', { path: clientDir });
          entries = Array.isArray(res) ? res : ((res && res.entries) || []);
        } catch (e) {
          entries = []; // dir likely missing — treat as empty
        }
        const files = entries.filter((en) => en && !en.is_directory);
        if (!files.length) {
          docsEl.className = 'pmp-docs pmp-muted';
          docsEl.textContent = 'no documents yet';
          return;
        }
        docsEl.className = 'pmp-docs';
        docsEl.innerHTML = '';
        files.forEach((f) => {
          const row = document.createElement('div');
          row.className = 'pmp-doc-row';
          row.innerHTML = `${ICON_DOC}<span>${escapeText(f.name)}</span>`;
          if (/\.md$/i.test(f.name || '')) {
            row.classList.add('pmp-doc-md');
            row.onclick = () => { if (window.xnautOpenMarkdownFile) window.xnautOpenMarkdownFile(f.path); };
          }
          docsEl.appendChild(row);
        });
      }

      const genWrap = q('.pmp-gen-btns');
      const genErr = q('.pmp-gen-error');
      // Preferred button order; unknown templates keep alphabetical tail.
      const ORDER = ['plan', 'architektur', 'meeting-notes', 'offerte', 'sla'];
      const rank = (t) => { const i = ORDER.indexOf(t); return i === -1 ? ORDER.length : i; };
      invoke('docgen_templates').then((templates) => {
        (templates || []).slice().sort((a, b) => rank(a) - rank(b) || a.localeCompare(b)).forEach((t) => {
          const b = document.createElement('button');
          b.className = 'pmp-btn';
          // Plan is interactive: open the Plan Mode workspace (chat + live doc)
          // instead of one-shot generating a file.
          if (t === 'plan') {
            b.textContent = 'Plan Mode';
            b.onclick = () => {
              if (!window.xnautAttachPlanTab) { genErr.textContent = 'Plan Mode unavailable'; genErr.hidden = false; return; }
              window.xnautAttachPlanTab({ projectContext: {
                client_company: project.client_company,
                scope: project.scope,
                contacts: project.contacts,
                path: task ? task.path : null,
              } });
            };
            genWrap.appendChild(b);
            return;
          }
          const lbl = 'Generate ' + t.charAt(0).toUpperCase() + t.slice(1);
          b.textContent = lbl;
          b.onclick = async () => {
            b.disabled = true;
            b.textContent = 'Generating…';
            genErr.hidden = true;
            try {
              const path = await invoke('docgen_generate', { taskId: project.task_id, template: t });
              await loadDocs();
              toast(`Generated ${String(path).split('/').pop()}`);
            } catch (e) {
              genErr.textContent = String(e);
              genErr.hidden = false;
            }
            b.disabled = false;
            b.textContent = lbl;
          };
          genWrap.appendChild(b);
        });
      }).catch((e) => {
        genErr.textContent = String(e);
        genErr.hidden = false;
      });

      q('.pmp-act-chat').onclick = () => {
        if (window.xnautAttachChatTab) {
          window.xnautAttachChatTab({ projectContext: {
            client_company: project.client_company,
            scope: project.scope,
            contacts: project.contacts,
            path: task ? task.path : null,
          } });
        }
      };
      q('.pmp-act-edit').onclick = () => {
        if (window.xnautOpenPmIntake) {
          window.xnautOpenPmIntake({ existing: project, onSaved: (saved) => renderDetail(saved || project) });
        }
      };
      // Two-click confirm (native confirm() is a no-op in Tauri's WKWebView).
      const removeBtn = q('.pmp-act-remove');
      let removeArmed = false;
      let removeTimer = null;
      removeBtn.onclick = async () => {
        if (!removeArmed) {
          removeArmed = true;
          removeBtn.textContent = 'Click again to remove';
          removeTimer = setTimeout(() => {
            removeArmed = false;
            removeBtn.textContent = 'Remove from PM';
          }, 3000);
          return;
        }
        clearTimeout(removeTimer);
        removeArmed = false;
        removeBtn.textContent = 'Remove from PM';
        try {
          await invoke('pm_delete', { id: project.id });
          renderList();
        } catch (e) {
          toast(String(e));
        }
      };

      loadFinancials();
      loadDocs();
    }

    renderList().catch((e) => console.error('[pm-panel] initial render failed:', e));

    const entry = { kind: 'pm', label, pane };
    panes.set(label, entry);
    return entry;
  }

  function destroyPmPanel(label) {
    const entry = panes.get(label);
    if (!entry) return;
    if (entry.pane && entry.pane.parentNode) entry.pane.parentNode.removeChild(entry.pane);
    panes.delete(label);
  }

  // Public API hooks.
  window.xnautCreatePmPanel = createPmPanel;
  window.xnautDestroyPmPanel = destroyPmPanel;
})();
