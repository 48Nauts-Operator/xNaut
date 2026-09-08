// The four master switches, in the app (XNAUT-231 shipped the switches with no
// surface at all).
//
// André, 2026-09-09, after NautBot spent a run failing every ticket write on
// "the read_only kill-switch is engaged" and neither of them could find where
// it lived: "this needs to be in the settings page of the Agent". It could only
// be flipped by editing ~/Library/Application Support/xnaut/kill-switches.json,
// and nothing in the app said a layer was off, so every agent write failed with
// a message that named a switch the owner could not see.
//
// Deliberately the OWNER's surface only. switches.rs states it: no agent tool
// touches these, and none does; this panel calls the same two commands the
// shell would, and every flip is already audited to kill-switches.log.
(function () {
  'use strict';

  const invoke = (...args) => window.__TAURI__.core.invoke(...args);
  const esc = (v) => String(v == null ? '' : v).replace(/[&<>"']/g, (c) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  }[c]));

  // Each switch says what it STOPS, in the words the agent sees when it hits
  // it. A toggle whose label is its field name teaches nothing.
  const SWITCHES = [
    {
      key: 'read_only',
      label: 'Read only',
      on: 'Every write tool refuses. Agents can list and read; nothing moves.',
      hint: 'Agents fail with "the read_only kill-switch is engaged; no ticket writes until the owner lifts it".',
    },
    {
      key: 'freeze_merges',
      label: 'Freeze merges',
      on: 'No merge lands, anywhere. Unmerge stays live: the safety valve is never frozen.',
      hint: 'The overnight switch.',
    },
    {
      key: 'approve_everything',
      label: 'Approve everything',
      on: 'The risk threshold drops to zero: every merge parks in the Mesh for you, whatever it scored.',
      hint: '',
    },
  ];

  function render(host, state, note) {
    const engaged = SWITCHES.filter((s) => state[s.key]).length + (state.quarantined || []).length;
    host.innerHTML = `
      <h3>Kill switches</h3>
      <p style="color:var(--text-secondary); font-size:13px; margin-bottom:14px;">
        Master switches above the policy below. One flag drops a whole enforcement layer,
        takes effect immediately with no restart, and is written to <code>kill-switches.log</code>.
        ${engaged ? `<strong style="color:var(--warning,#f5b840)">${engaged} engaged.</strong>` : 'All off.'}
      </p>
      <div class="settings-group" id="kill-switch-rows">
        ${SWITCHES.map((s) => `
          <label style="display:flex; gap:10px; align-items:flex-start; padding:10px 0; border-top:1px solid var(--border,#2a2a2f);">
            <input type="checkbox" data-switch="${s.key}" ${state[s.key] ? 'checked' : ''} style="margin-top:3px;">
            <span>
              <span style="font-weight:600;">${esc(s.label)}</span>
              ${state[s.key] ? '<span style="color:var(--warning,#f5b840); font-size:12px; margin-left:8px;">ENGAGED</span>' : ''}
              <br><span style="color:var(--text-secondary); font-size:12px;">${esc(s.on)}</span>
              ${s.hint ? `<br><span style="color:var(--text-secondary); font-size:11px; opacity:.75;">${esc(s.hint)}</span>` : ''}
            </span>
          </label>
        `).join('')}
        <div style="padding:10px 0; border-top:1px solid var(--border,#2a2a2f);">
          <span style="font-weight:600;">Quarantined agents</span>
          <br><span style="color:var(--text-secondary); font-size:12px;">Their nudges are dead, tickets cannot be assigned to them, and their ticket list answers empty.</span>
          <input type="text" id="kill-switch-quarantined" value="${esc((state.quarantined || []).join(', '))}"
                 placeholder="handles, comma separated" style="width:100%; margin-top:8px;">
        </div>
      </div>
      <div id="kill-switch-note" style="font-size:12px; color:var(--text-secondary); margin-top:8px; min-height:16px;">${esc(note || '')}</div>
    `;

    const save = async (next) => {
      const noteEl = document.getElementById('kill-switch-note');
      try {
        const saved = await invoke('kill_switches_set', { switches: next });
        render(host, saved, 'Saved. In force now; no restart needed.');
      } catch (error) {
        // Never silent: a switch that looks flipped but is not is worse than
        // one that refused out loud.
        if (noteEl) noteEl.textContent = `Not saved: ${error}`;
      }
    };

    host.querySelectorAll('[data-switch]').forEach((el) => {
      el.onchange = () => save({ ...state, [el.dataset.switch]: el.checked });
    });
    const q = document.getElementById('kill-switch-quarantined');
    if (q) {
      q.onchange = () => save({
        ...state,
        quarantined: q.value.split(',').map((s) => s.trim()).filter(Boolean),
      });
    }
  }

  async function mount(host) {
    if (!host) return;
    try {
      render(host, await invoke('kill_switches_get'), '');
    } catch (error) {
      host.innerHTML = `<h3>Kill switches</h3><p style="color:var(--error,#ef4444); font-size:13px;">Could not read them: ${esc(error)}</p>`;
    }
  }

  window.xnautRenderKillSwitches = mount;
})();
