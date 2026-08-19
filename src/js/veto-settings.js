// Guardrails: the tool-call policy, edited in the app (XNAUT-132).
//
// André, 2026-08-17: first "add this to the settings page of xNaut rather than
// the OS level (shell)", then, on seeing the form, "the form is also very hard
// to read", and: "lets have a simple editor, with a example, a backup when
// someone edit it, and a syntax check when save is clicked. Plus a test on the
// bottom."
//
// A text editor rather than a form, and not only because a grid of fifteen
// rules is unreadable. His file carries COMMENTS explaining why each group of
// rules exists, and a structured editor rebuilds the file from parsed rules,
// which would have deleted every one of them on the first save.
//
// Same file the shell would edit. One writer, one source of truth.
(function () {
  'use strict';

  const invoke = (...args) => window.__TAURI__.core.invoke(...args);
  const esc = (value) => String(value == null ? '' : value).replace(/[&<>"']/g, (character) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  }[character]));

  // Deliberately small, and a starting point rather than a policy to adopt
  // blind: three obvious-looking rules turned out to block the loop's own work
  // (an agent pushing its worktree branch, cleaning a build directory, and
  // xNAUT closing a finished zellij session with --force).
  const EXAMPLE = `# Tool-call policy. Every rule DENIES; there is no allow list.
# A rule matches when every field it names matches. One that names no
# tool/contains/under is ignored rather than matching everything.
#
#   tool      exact tool name, case-insensitive (Bash, Write, Edit)
#   contains  substring of the call's arguments
#   under     path prefix; matches the working directory OR a path in the arguments
#   agent     a handle, to bind one agent
#   reason    what the model is told. Say what to do instead.

# Publishing a website is the line. Scoped to the websites folder on purpose:
# a blanket "git push" rule also stops agents pushing their own worktree
# branches, which is how their work survives a worktree being removed.
[[deny]]
tool = "Bash"
contains = "git push"
under = "/Users/cand0rian/DevHub_Studio/factory/12-Websites"
reason = "Pushing a site publishes it. Hand the ticket back and ask André in the inbox."

# A client's site earns a stricter rule than our own.
[[deny]]
under = "/Users/cand0rian/DevHub_Studio/factory/12-Websites/dat-ag-website"
reason = "The DAT AG site belongs to a client. Prepare the change and ask before anything reaches it."

# Releases are André's call.
[[deny]]
tool = "Bash"
contains = "git tag"
reason = "Tags trigger a release and a cask. Prepare it and ask."

# Destructive git. ~/.claude/hooks/block-dangerous-git.sh already covers these
# for claude and cannot cover codex, which is what @rudi runs.
[[deny]]
tool = "Bash"
contains = "reset --hard"
reason = "Discarding work is not reversible. Commit what you have and say what you were trying to undo."

[[deny]]
tool = "Bash"
contains = "git clean -f"
reason = "That deletes untracked files, which are often the only copy. Ask first."

# Tickets change through the PM tools, never by editing the control repo.
[[deny]]
under = "/Users/cand0rian/.xnaut-control"
reason = "Use create_ticket and update_ticket rather than editing ticket files."
`;

  let host = null;
  let saved = '';

  const el = (selector) => host.querySelector(selector);

  function setStatus(text, kind) {
    const box = el('[data-status]');
    if (!box) return;
    box.className = `vt-status ${kind || ''}`;
    box.textContent = text;
  }

  function markDirty() {
    const dirty = el('[data-editor]').value !== saved;
    el('[data-save]').disabled = !dirty;
    if (dirty) setStatus('Edited. Not saved.', '');
  }

  function render(text, meta) {
    host.innerHTML = `
      <style>
        .vt-note { color: var(--text-secondary, #8a8a94); font-size: 12px; line-height: 1.6; margin: 0 0 12px; }
        .vt-note b { color: var(--text-primary, #e8e8ec); font-weight: 600; }
        .vt-editor { width: 100%; min-height: 380px; resize: vertical; padding: 12px 14px;
          border: 1px solid var(--border-color, #303038); border-radius: 8px;
          background: var(--bg-primary, #0d0e11); color: var(--text-primary, #e8e8ec);
          font-family: var(--font-mono, ui-monospace, Menlo, monospace); font-size: 12.5px; line-height: 1.6;
          tab-size: 2; white-space: pre; overflow-wrap: normal; overflow-x: auto; }
        .vt-editor:focus { outline: none; border-color: var(--accent, #f5b840); }
        .vt-bar { display: flex; gap: 8px; align-items: center; margin-top: 10px; flex-wrap: wrap; }
        .vt-status { font-size: 12px; color: var(--text-secondary, #8a8a94); }
        .vt-status.bad { color: #e5484d; }
        .vt-status.good { color: #4ade80; }
        .vt-path { font-family: var(--font-mono, monospace); font-size: 10.5px; color: var(--faint, #5c626c);
          margin-top: 8px; overflow-wrap: anywhere; }
        .vt-try { margin-top: 22px; padding-top: 14px; border-top: 1px solid var(--border-color, #26262c); }
        .vt-try h4 { margin: 0 0 4px; font-size: 12px; }
        .vt-try p { margin: 0 0 10px; font-size: 11.5px; color: var(--text-secondary, #8a8a94); }
        .vt-row { display: flex; gap: 6px; align-items: center; }
        .vt-in { min-width: 0; padding: 7px 9px; border: 1px solid var(--border-color, #303038); border-radius: 6px;
          background: var(--bg-primary, #0d0e11); color: var(--text-primary, #e8e8ec);
          font-family: var(--font-mono, monospace); font-size: 12px; }
        .vt-verdict { margin-top: 10px; font-size: 12.5px; padding: 9px 11px; border-radius: 6px; display: none; line-height: 1.5; }
        .vt-verdict.deny { display: block; background: rgba(229,72,77,.14); color: #e5484d; }
        .vt-verdict.allow { display: block; background: rgba(74,222,128,.12); color: #4ade80; }
        .vt-log { margin-top: 22px; padding-top: 14px; border-top: 1px solid var(--border-color, #26262c); }
        .vt-log h4 { margin: 0 0 4px; font-size: 12px; }
        .vt-log p { margin: 0 0 10px; font-size: 11.5px; color: var(--text-secondary, #8a8a94); }
        .vt-entry { display: flex; gap: 9px; align-items: baseline; padding: 5px 0;
          border-top: 1px solid var(--border-color, #1f1f25); font-size: 12px; }
        .vt-entry:first-child { border-top: none; }
        .vt-kind { flex: 0 0 62px; font-size: 10px; letter-spacing: .04em; text-transform: uppercase; font-weight: 600; }
        .vt-kind.refused { color: #e5484d; }
        .vt-kind.asked { color: #f5b840; }
        .vt-kind.conflict { color: #f5b840; }
        .vt-who { flex: 0 0 auto; font-family: var(--font-mono, monospace); color: var(--text-secondary, #8a8a94); }
        .vt-detail { flex: 1 1 auto; min-width: 0; overflow-wrap: anywhere; }
        .vt-when { flex: 0 0 auto; color: var(--faint, #5c626c); font-variant-numeric: tabular-nums; font-size: 11px; }
      </style>

      <p class="vt-note">
        Every rule <b>denies</b> a tool call and hands its reason to the agent, which then picks something
        else. Rules apply at the <b>next agent launch</b>, on claude and codex. With no rules, nothing is
        denied. Saving checks the syntax first and keeps a copy of the previous version.
      </p>

      <textarea class="vt-editor" data-editor spellcheck="false">${esc(text)}</textarea>

      <div class="vt-bar">
        <button class="btn btn-primary" data-save disabled>Save</button>
        <button class="btn" data-example>Insert example</button>
        <button class="btn" data-revert>Revert</button>
        <select class="vt-in" data-backups style="display:none"></select>
        <span class="vt-status" data-status></span>
      </div>
      <div class="vt-path">${esc(meta.path)}${meta.exists ? '' : '  (not created yet)'}</div>

      <div class="vt-try">
        <h4>Try a call</h4>
        <p>Checked against the <b>saved</b> file, so it answers what would really happen, not what is in the box above.</p>
        <div class="vt-row">
          <input class="vt-in" data-try-tool value="Bash" style="flex:0 0 90px">
          <input class="vt-in" data-try-input value="git push origin main" placeholder="the command or arguments" style="flex:3">
          <input class="vt-in" data-try-cwd placeholder="working directory (optional)" style="flex:2">
          <button class="btn" data-try>Check</button>
        </div>
        <div class="vt-verdict" data-verdict></div>
      </div>

      <div class="vt-log">
        <h4>What the rules have done</h4>
        <p>Only a <b>refusal</b>, a <b>question</b> and a two-agent <b>conflict</b> are written down. An allowed
        call records nothing, so an empty list here means no rule has fired, not that nothing ran.</p>
        <div data-ledger></div>
      </div>`;

    const editor = el('[data-editor]');
    editor.oninput = markDirty;

    el('[data-example]').onclick = () => {
      const current = editor.value.trim();
      editor.value = current ? `${current}\n\n${EXAMPLE}` : EXAMPLE;
      markDirty();
      setStatus('Example inserted. Read it before saving; some obvious rules block the loop itself.', '');
    };

    el('[data-revert]').onclick = () => {
      editor.value = saved;
      markDirty();
      setStatus('Back to the saved file.', '');
    };

    el('[data-save]').onclick = async () => {
      const text = editor.value;
      // Syntax first. A policy file that does not parse is a machine with no
      // policy at all, and it fails silently, so nothing broken is written.
      const check = await invoke('veto_validate', { text }).catch((error) => ({ ok: false, error: String(error) }));
      if (!check.ok) {
        setStatus(`Not saved. ${check.error}`, 'bad');
        return;
      }
      try {
        const result = await invoke('veto_write', { text });
        saved = text;
        markDirty();
        const inert = check.inert
          ? ` ${check.inert} rule${check.inert === 1 ? ' has' : 's have'} no condition and will be ignored.`
          : '';
        setStatus(
          `Saved. ${result.in_force} rule${result.in_force === 1 ? '' : 's'} in force at the next agent launch.`
          + (result.backup ? ' Previous version kept.' : '') + inert,
          'good',
        );
        await loadBackups();
      } catch (error) {
        setStatus(`Not saved. ${error}`, 'bad');
      }
    };

    el('[data-try]').onclick = async () => {
      const verdict = el('[data-verdict]');
      try {
        const answer = await invoke('veto_check', {
          tool: el('[data-try-tool]').value.trim(),
          input: JSON.stringify({ command: el('[data-try-input]').value }),
          agent: null,
          cwd: el('[data-try-cwd]').value.trim(),
        });
        const denied = answer && answer.decision === 'deny';
        verdict.className = `vt-verdict ${denied ? 'deny' : 'allow'}`;
        verdict.textContent = denied
          ? `Blocked. The agent is told: "${answer.reason}"`
          : 'Allowed by the saved rules.';
      } catch (error) {
        verdict.className = 'vt-verdict deny';
        verdict.textContent = `Could not check: ${error}`;
      }
    };

    el('[data-backups]').onchange = async (event) => {
      const chosen = event.target.value;
      if (!chosen) return;
      const file = await invoke('read_file', { path: chosen }).catch(() => null);
      if (file && typeof file.text === 'string') {
        editor.value = file.text;
        markDirty();
        setStatus('Earlier version loaded into the editor. Save to put it back.', '');
      } else {
        setStatus('Could not read that backup.', 'bad');
      }
      event.target.value = '';
    };
  }

  // Time as a person reads it. An audit line whose only stamp is an RFC3339
  // string makes you do arithmetic to answer "was that just now".
  function ago(iso) {
    const then = Date.parse(iso);
    if (!Number.isFinite(then)) return '';
    const secs = Math.max(0, Math.round((Date.now() - then) / 1000));
    if (secs < 60) return `${secs}s ago`;
    if (secs < 3600) return `${Math.round(secs / 60)}m ago`;
    if (secs < 86400) return `${Math.round(secs / 3600)}h ago`;
    return `${Math.round(secs / 86400)}d ago`;
  }

  // The decision ledger, which had no reader at all: ledger_recent was
  // registered, ACL-allowed and called by nothing, so every refusal an agent hit
  // was written to disk and never shown. It belongs under the editor, where the
  // rules that caused it are.
  async function loadLedger() {
    const list = el('[data-ledger]');
    if (!list) return;
    const entries = await invoke('ledger_recent', { limit: 12 }).catch(() => null);
    if (!entries) { list.innerHTML = '<p>Could not read the ledger.</p>'; return; }
    if (!entries.length) { list.innerHTML = '<p>Nothing yet. No rule has stopped or questioned a call.</p>'; return; }
    list.innerHTML = entries.map((entry) => `
      <div class="vt-entry">
        <span class="vt-kind ${esc(entry.kind)}">${esc(entry.kind)}</span>
        <span class="vt-who">@${esc(entry.agent || 'unknown')}</span>
        <span class="vt-detail">${esc(entry.detail)}</span>
        <span class="vt-when">${esc(ago(entry.at))}</span>
      </div>`).join('');
  }

  async function loadBackups() {
    const select = el('[data-backups]');
    if (!select) return;
    const backups = await invoke('veto_backups').catch(() => []);
    if (!backups.length) { select.style.display = 'none'; return; }
    select.style.display = '';
    select.innerHTML = '<option value="">Earlier versions…</option>'
      + backups.map((item) => `<option value="${esc(item.path)}">${esc(String(item.path).split('/').pop())}</option>`).join('');
  }

  window.xnautRenderVetoSettings = async (element) => {
    if (!element) return;
    host = element;
    const meta = await invoke('veto_read').catch((error) => ({ path: String(error), exists: false, text: '', rules: 0 }));
    saved = meta.text || '';
    render(saved, meta);
    setStatus(
      meta.exists ? `${meta.rules} rule${meta.rules === 1 ? '' : 's'} in force.` : 'No policy file yet. Nothing is denied.',
      '',
    );
    await loadBackups();
    await loadLedger();
  };
})();
