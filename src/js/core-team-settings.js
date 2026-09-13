// The core team's one surface (XNAUT-357).
//
// Everything the loop PRODUCES already has a home: findings are tickets on the
// CORE board, a PoC run is a run in the Observatory, the Judge's verdict is a
// `council.verdict` line in the Decisions pane, and an approved PoC opens on
// the Plan Canvas. A fifth pane restating all four would only be a place for
// them to disagree.
//
// What has no home is the part that is nobody else's: the switch, the
// threshold, and the sentence saying why the loop is idle. That is this pane,
// and it is deliberately small.
//
// The threshold is the cost knob and the copy says so, because the number is
// the whole decision: everything under it costs one review, everything over it
// costs a full agent run.
(function () {
  'use strict';

  const invoke = (...args) => window.__TAURI__.core.invoke(...args);
  const esc = (value) => String(value == null ? '' : value).replace(/[&<>"']/g, (character) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  }[character]));

  function ago(ms) {
    if (!ms) return 'never';
    const minutes = Math.round((Date.now() - ms) / 60000);
    if (minutes < 60) return `${minutes}m ago`;
    if (minutes < 60 * 24) return `${Math.round(minutes / 60)}h ago`;
    return `${Math.round(minutes / 1440)}d ago`;
  }

  function render(host, status, core) {
    const blocked = (status.blocked || []);
    host.innerHTML = `
      <h3>Core Team</h3>
      <p style="color:var(--text-secondary); font-size:13px; margin-bottom:16px;">
        A Researcher looks outside once a week, a Reviewer weighs what it found, anything over the
        threshold gets a prototype in its own worktree, and a Judge decides in or out. Experimental:
        nothing merges without you.
      </p>
      <div class="settings-group">
        <label style="display:flex; align-items:center; gap:8px;">
          <input type="checkbox" data-ct-enabled ${core.enabled ? 'checked' : ''}>
          <span>Run the core team</span>
        </label>
        <div class="settings-field" style="margin-top:12px;">
          <label>PoC threshold — the cost knob</label>
          <input type="number" min="0" max="100" step="1" data-ct-threshold value="${esc(core.poc_threshold)}">
          <small style="color:var(--text-secondary);">0–100. Under it a finding costs one review. Over it, a full agent run.</small>
        </div>
        <div class="settings-field" style="margin-top:12px;">
          <label>Days between beats</label>
          <input type="number" min="1" max="90" step="1" data-ct-days value="${esc(core.beat_days)}">
        </div>
        <div class="settings-field" style="margin-top:12px;">
          <label>Minutes a PoC run may take</label>
          <input type="number" min="5" max="600" step="5" data-ct-minutes value="${esc(core.poc_minutes)}">
        </div>
        <div class="settings-field" style="margin-top:12px;">
          <label>Board findings are filed on</label>
          <input type="text" data-ct-project value="${esc(core.project)}">
        </div>
        <div class="settings-field" style="margin-top:12px;">
          <label>Topics, one per line</label>
          <textarea data-ct-topics rows="4" style="width:100%;">${esc((core.topics || []).join('\n'))}</textarea>
        </div>
        <button class="btn btn-primary" data-ct-save style="width:100%; margin-top:12px;">Save Core Team Settings</button>
        <div data-ct-status style="margin-top:8px; font-size:13px; color:var(--text-secondary);"></div>
      </div>
      <div class="settings-group" style="margin-top:16px;">
        <div style="font-size:13px; color:var(--text-secondary);">
          <div>Researcher: ${status.researcher ? '@' + esc(status.researcher) : 'none'}</div>
          <div>Reviewer: ${status.reviewer ? '@' + esc(status.reviewer) : 'none'}${status.reviewer_provider ? ' on ' + esc(status.reviewer_provider) : ''}</div>
          <div>Last beat: ${esc(ago(status.last_beat_ms))}</div>
        </div>
        ${blocked.length ? `<ul style="margin-top:8px; color:var(--text-secondary); font-size:13px;">${
          blocked.map((line) => `<li>${esc(line)}</li>`).join('')
        }</ul>` : '<p style="margin-top:8px; font-size:13px;">Nothing is in the way.</p>'}
        <button class="btn" data-ct-scan style="width:100%; margin-top:12px;">Scan now</button>
      </div>
    `;
  }

  window.xnautRenderCoreTeamSettings = async (host) => {
    if (!host) return;
    let settings;
    try {
      settings = await invoke('settings_get');
    } catch (error) {
      host.innerHTML = `<h3>Core Team</h3><p>Could not read settings: ${esc(error)}</p>`;
      return;
    }
    // A missing block is an older settings.json, not a broken one: the Rust
    // side fills its defaults on load, and the pane must render either way.
    const core = settings.core_team || {};
    const status = await invoke('core_team_status').catch(() => ({ blocked: ['status unavailable'] }));
    render(host, status, {
      enabled: !!core.enabled,
      poc_threshold: core.poc_threshold ?? status.threshold ?? 70,
      beat_days: core.beat_days ?? status.beat_days ?? 7,
      poc_minutes: core.poc_minutes ?? 90,
      project: core.project || status.project || 'CORE',
      topics: core.topics || status.topics || [],
    });

    const say = (text) => {
      const line = host.querySelector('[data-ct-status]');
      if (line) line.textContent = text;
    };

    host.querySelector('[data-ct-save]')?.addEventListener('click', async () => {
      const number = (selector, fallback) => {
        const raw = parseInt(host.querySelector(selector)?.value, 10);
        return Number.isFinite(raw) ? raw : fallback;
      };
      // Read the WHOLE settings object back and edit one key. Writing a fresh
      // object here would drop every field this pane does not know about,
      // which is the same failure the `extra` flatten exists to prevent on the
      // Rust side.
      const current = await invoke('settings_get');
      current.core_team = {
        ...(current.core_team || {}),
        enabled: !!host.querySelector('[data-ct-enabled]')?.checked,
        poc_threshold: Math.max(0, Math.min(100, number('[data-ct-threshold]', 70))),
        beat_days: Math.max(1, number('[data-ct-days]', 7)),
        poc_minutes: Math.max(5, number('[data-ct-minutes]', 90)),
        project: (host.querySelector('[data-ct-project]')?.value || 'CORE').trim(),
        topics: String(host.querySelector('[data-ct-topics]')?.value || '')
          .split('\n').map((line) => line.trim()).filter(Boolean),
      };
      try {
        await invoke('settings_set', { settings: current });
        say('Saved.');
      } catch (error) {
        say(`Could not save: ${error}`);
      }
    });

    host.querySelector('[data-ct-scan]')?.addEventListener('click', async (event) => {
      event.target.disabled = true;
      say('Looking…');
      try {
        const result = await invoke('core_team_scan', {});
        if (result.blocked) { say(result.blocked); return; }
        // "Filed 6" and "1 of them earns a PoC run" are different news, and the
        // second is the one that costs an agent run next.
        const weighed = result.weighed || [];
        const poc = weighed.filter((item) => item.verdict === 'poc').length;
        const unweighed = weighed.filter((item) => item.error).length;
        say(
          `Filed ${result.filed.length}; skipped ${result.skipped.length}; ${poc} to PoC`
          + (unweighed ? `; ${unweighed} could not be weighed` : '') + '.',
        );
      } catch (error) {
        say(String(error));
      } finally {
        event.target.disabled = false;
      }
    });
  };
})();
