// Issue intake's one surface (XNAUT-382).
//
// The loop itself is headless. It rides the sweep and files tickets nobody
// asked it to, so the only thing that has no home elsewhere is the part that
// is nobody else's: which projects take issues in, on what trigger, and what
// stops the ones that are switched on. Everything intake PRODUCES is already a
// ticket on the board.
//
// The pane is per PROJECT rather than global because the setting is: a repo
// that already uses `triage` should not have to learn a second word, and one
// workspace routinely has a Linear team per product. The Linear API key is the
// exception and lives in settings, because it is a credential and credentials
// are machine-local.
//
// Rescan is a button and not a hidden behaviour. The cursor is a high-water
// mark, so an issue labelled AFTER it was created sits below it forever; the
// escape hatch has to be something a person can press, or it is a silent
// no-op of exactly the kind this codebase keeps shipping.
(function () {
  'use strict';

  const invoke = (...args) => window.__TAURI__.core.invoke(...args);
  const esc = (value) => String(value == null ? '' : value).replace(/[&<>"']/g, (character) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  }[character]));

  function row(project) {
    const labelled = project.trigger !== 'all';
    return `
      <div class="settings-group" data-ii-project="${esc(project.key)}" style="margin-top:12px;">
        <div style="display:flex; align-items:center; justify-content:space-between; gap:8px;">
          <strong>${esc(project.name || project.key)}</strong>
          <label style="display:flex; align-items:center; gap:6px; font-size:13px;">
            <input type="checkbox" data-ii-enabled ${project.enabled ? 'checked' : ''}>
            <span>Take issues in</span>
          </label>
        </div>
        <div style="font-size:12px; color:var(--text-secondary); margin-top:4px;">
          ${esc(project.linear_team ? `Linear team ${project.linear_team}` : project.forge_remote || 'no remote')}
        </div>
        <div class="settings-field" style="margin-top:8px;">
          <label>Which issues</label>
          <select data-ii-trigger>
            <option value="labelled" ${labelled ? 'selected' : ''}>Only issues with a label</option>
            <option value="all" ${labelled ? '' : 'selected'}>Every issue</option>
          </select>
        </div>
        <div class="settings-field" style="margin-top:8px;">
          <label>The label</label>
          <input type="text" data-ii-label value="${esc(project.label || 'xnaut')}">
        </div>
        <div class="settings-field" style="margin-top:8px;">
          <label>Linear team key. Empty reads the forge remote above</label>
          <input type="text" data-ii-linear value="${esc(project.linear_team)}" placeholder="ENG">
        </div>
        <div style="font-size:12px; color:var(--text-secondary); margin-top:8px;">
          ${project.blocked
            ? `<span data-ii-blocked>Idle: ${esc(project.blocked)}</span>`
            : `${esc(project.tracked)} issue(s) tracked${project.cursor ? ` · seen up to ${esc(project.cursor).replace(/^0+/, '')}` : ' · nothing seen yet'}`}
        </div>
        <div style="display:flex; gap:8px; margin-top:8px;">
          <button class="btn btn-primary" data-ii-save style="flex:1;">Save</button>
          <button class="btn" data-ii-run style="flex:1;">Run now</button>
          <button class="btn" data-ii-rescan style="flex:1;" title="Forget the cursor and re-read every open issue">Rescan</button>
        </div>
        <div data-ii-status style="margin-top:8px; font-size:13px; color:var(--text-secondary);"></div>
      </div>
    `;
  }

  function render(host, status, linear) {
    host.innerHTML = `
      <h3>Issue Intake</h3>
      <p style="color:var(--text-secondary); font-size:13px; margin-bottom:16px;">
        An issue on GitHub, Forgejo or Linear becomes an inbox ticket on this board, and the issue
        gets a comment saying which one. The ticket's status goes back as an <code>xnaut:</code>
        label. Nothing is assigned and nothing is dispatched; an issue is a request.
      </p>
      <div class="settings-group">
        <div class="settings-field">
          <label>Linear API key</label>
          <input type="password" data-ii-key value="${esc(linear.api_key || '')}" placeholder="lin_api_…">
          <small style="color:var(--text-secondary);">Only needed for projects that read a Linear team. Stored on this machine.</small>
        </div>
        <button class="btn btn-primary" data-ii-save-key style="width:100%; margin-top:12px;">Save Linear Key</button>
        <div data-ii-key-status style="margin-top:8px; font-size:13px; color:var(--text-secondary);"></div>
      </div>
      ${status.blocked
        ? `<p style="margin-top:16px;" data-ii-global-blocked>${esc(status.blocked)}</p>`
        : status.projects.length
          ? status.projects.map(row).join('')
          : '<p style="margin-top:16px;">No project has a forge remote yet, so there is nothing to take issues from.</p>'}
    `;
  }

  function said(report) {
    if (report.blocked) return report.blocked;
    const parts = [];
    parts.push(report.created.length ? `Filed ${report.created.join(', ')}` : 'Filed nothing');
    if (report.already_tracked) parts.push(`${report.already_tracked} already tracked`);
    if (report.mirrored) parts.push(`${report.mirrored} label(s) updated`);
    return `${parts.join('; ')}.`;
  }

  window.xnautRenderIssueIntakeSettings = async (host) => {
    if (!host) return;
    let settings;
    try {
      settings = await invoke('settings_get');
    } catch (error) {
      host.innerHTML = `<h3>Issue Intake</h3><p>Could not read settings: ${esc(error)}</p>`;
      return;
    }
    const status = await invoke('issue_intake_status').catch((error) => ({
      projects: [], blocked: String(error),
    }));
    render(host, status, settings.linear || {});

    const say = (scope, text) => {
      const line = scope.querySelector('[data-ii-status]') || host.querySelector('[data-ii-key-status]');
      if (line) line.textContent = text;
    };

    host.querySelector('[data-ii-save-key]')?.addEventListener('click', async () => {
      // Read the WHOLE settings object back and edit one key. Writing a fresh
      // object here drops every field this pane does not know about, which is
      // the same failure the `extra` flatten exists to prevent on the Rust side.
      const current = await invoke('settings_get');
      current.linear = {
        ...(current.linear || {}),
        api_key: host.querySelector('[data-ii-key]')?.value.trim() || '',
      };
      const line = host.querySelector('[data-ii-key-status]');
      try {
        await invoke('settings_set', { settings: current });
        if (line) line.textContent = 'Saved.';
      } catch (error) {
        if (line) line.textContent = `Could not save: ${error}`;
      }
    });

    host.querySelectorAll('[data-ii-project]').forEach((scope) => {
      const key = scope.getAttribute('data-ii-project');
      const read = () => ({
        enabled: !!scope.querySelector('[data-ii-enabled]')?.checked,
        trigger: scope.querySelector('[data-ii-trigger]')?.value || 'labelled',
        label: (scope.querySelector('[data-ii-label]')?.value || '').trim() || 'xnaut',
        linear_team: (scope.querySelector('[data-ii-linear]')?.value || '').trim(),
      });

      scope.querySelector('[data-ii-save]')?.addEventListener('click', async () => {
        try {
          await invoke('issue_intake_configure', { project: key, intake: read() });
          say(scope, 'Saved.');
        } catch (error) {
          say(scope, `Could not save: ${error}`);
        }
      });

      const go = async (button, rescan) => {
        button.disabled = true;
        say(scope, rescan ? 'Re-reading every open issue…' : 'Looking…');
        try {
          // Save first. Pressing Run with an unsaved trigger and watching it
          // use the old one is the kind of surprise that makes a pane
          // untrustworthy.
          await invoke('issue_intake_configure', { project: key, intake: read() });
          const reports = await invoke('issue_intake_run_now', { project: key, rescan });
          say(scope, reports.length ? reports.map(said).join(' ') : 'Intake is off for this project.');
        } catch (error) {
          say(scope, String(error));
        } finally {
          button.disabled = false;
        }
      };
      scope.querySelector('[data-ii-run]')?.addEventListener('click', (event) => go(event.target, false));
      scope.querySelector('[data-ii-rescan]')?.addEventListener('click', (event) => go(event.target, true));
    });
  };
})();
