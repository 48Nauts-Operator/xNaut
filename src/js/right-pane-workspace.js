// Right-pane Workspace view — Phase 1 of the Workspace orchestrator (XNAUT-38).
//
// The shell: a right-pane view with two sub-tabs —
//   • Agentic — capture/preserve artifacts, links, decisions from the session
//     (wired to the transcript in Phase 2).
//   • Loops   — call a loop → GitVM sandbox, self-test/record/iterate
//     (wired to loops_run_start → sandbox.rs in Phase 3).
// State (active sub-tab) persists per project/root. Registers itself as the
// right-pane view 'workspace' (or queues if the host isn't up yet).
(function () {
  'use strict';

  function register(key, view) {
    if (typeof window.xnautRightPaneRegisterView === 'function') {
      window.xnautRightPaneRegisterView(key, view);
    } else {
      (window.__xnautRightPaneQueue = window.__xnautRightPaneQueue || []).push({ key, view });
    }
  }

  const invoke = (...a) => window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke(...a);
  function openTarget(t) { if (typeof window.xnautOpenUrl === 'function') window.xnautOpenUrl(t); else if (window.__TAURI__?.shell?.open) window.__TAURI__.shell.open(t); }
  function openInternalBrowser(url) {
    if (typeof window.xnautAttachBrowserTab === 'function') { window.xnautAttachBrowserTab(url); return true; }
    return false;
  }
  // Read a captured item in place: .md/.txt → in-app markdown viewer; .html and
  // artifact URLs → the internal browser tab; anything else → OS default.
  function openItem(it) {
    const t = it && it.target;
    if (!t) return;
    const isUrl = /^[a-z][a-z0-9+.-]*:\/\//i.test(t);
    if (it.kind !== 'artifact' && /\.(md|markdown|txt)$/i.test(t) && typeof window.xnautOpenMarkdownFile === 'function') {
      window.xnautOpenMarkdownFile(t);
      return;
    }
    if (isUrl || /\.html?$/i.test(t)) {
      const url = isUrl ? t : 'file://' + t;
      if (openInternalBrowser(url)) return;
    }
    openTarget(t);
  }

  function escapeText(s) {
    return String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
  }
  const basename = (p) => String(p || '').replace(/\/+$/, '').split('/').pop() || '';

  const SUBTABS = [
    ['agentic', 'History'],
    ['loops', 'Looms'],
  ];
  // The fixed action-verb set a loom step may use (shown in the editor dropdown).
  const ACTIONS = ['provision', 'sync', 'exec', 'agent', 'test', 'record', 'artifacts', 'teardown', 'spawn'];
  const SUBTAB_KEY = 'xnaut-workspace-subtab';

  // A Weave (*.loom.json) is provider-agnostic — steps carry abstract action verbs
  // (provision/sync/exec/agent/test/record/artifacts/teardown/spawn). A PROVIDER
  // maps each verb to a concrete instruction. composeWeave() renders the selected
  // Weave + the human's goal into one prompt the terminal agent can execute.
  const PROVIDERS = {
    gitvm: {
      provision: () => '`gitvm warm-up` — spin up the sandbox (syncs your claude/codex auth).',
      sync: () => '(code sync is automatic — `gitvm run` rsyncs your uncommitted changes into /workspace).',
      exec: () => '`gitvm run <cmd>` — run commands in the sandbox.',
      agent: (s) => '`gitvm run \'<agent> -p "…"\'` — run the agent on the goal.' + ((s.with && s.with.headed) ? ' Drive any UI HEADED (Playwright headless:false on :0) so ffmpeg captures it.' : ''),
      test: (s) => '`gitvm run <tests>`, iterate until green' + ((s.loop && s.loop.max) ? ` (max ${s.loop.max} attempts).` : '.'),
      record: () => 'Record a video by driving the UI HEADED (headed Playwright / Xfce terminal on :0) — SSH-only runs record a blank screen. Recorder writes /artifacts/session-*.mp4.',
      artifacts: () => '`gitvm artifacts pull` — bring back logs, video, and outputs.',
      teardown: () => '`gitvm stop` — destroy the sandbox when done.',
      spawn: () => 'Spawn specialist sub-agents in their own sandboxes (`gitvm warm-up` + `gitvm run codex exec "<sub-task>"`) and coordinate via the Engram mesh.',
    },
    local: {
      provision: () => 'Work in the local project directory (no sandbox).',
      sync: () => '',
      exec: () => 'Run commands locally.',
      agent: () => 'Run the agent on the goal locally.',
      test: () => 'Run the tests locally; iterate until green.',
      record: () => 'Record a screen capture if a recorder is available.',
      artifacts: () => 'Collect outputs from the working directory.',
      teardown: () => '',
      spawn: () => 'Spawn helper agents locally.',
    },
  };

  function composeWeave(weave, goal) {
    const provider = (weave.runtime && weave.runtime.provider) || 'gitvm';
    const P = PROVIDERS[provider] || PROVIDERS.gitvm;
    if (!(weave.steps || []).length) return goal;   // blank Weave → verbatim
    const lines = [`Run this workload as a NautLoom "${weave.metadata.name}" playbook on the ${provider} provider, and only return once it satisfies the acceptance criteria.`];
    weave.steps.forEach((s, i) => {
      const fn = P[s.action];
      const detail = fn ? fn(s) : `(${s.action})`;
      if (detail) lines.push(`${i + 1}. ${detail}`);
    });
    if ((weave.acceptance || []).length) lines.push(`ACCEPTANCE (done when): ${weave.acceptance.join('; ')}.`);
    if (weave.report && weave.report.include) lines.push(`REPORT back: ${(weave.report.include || []).join(', ')}.`);
    lines.push(`\nWORKLOAD:\n${goal}`);
    return lines.join('\n');
  }

  // Resolve a loom's abstract steps into concrete shell commands for its provider.
  // Returns [{action, show, cmd}] — `show` is human-readable (dry-run/plan), `cmd`
  // is what the executor runs. The run's goal is written to .loom-goal.txt in the
  // project dir (and rsynced into the sandbox by `gitvm run`), so the agent step
  // reads it without CLI-quoting a ticket-sized prompt.
  const GOAL_FILE = '.loom-goal.txt';
  function composeCommands(weave, provider) {
    provider = provider || (weave.runtime && weave.runtime.provider) || 'gitvm';
    // If the loom has an agent step, the AGENT owns the build → test → fix loop
    // (Claude Code iterates natively). So we hand it the whole job and skip the
    // separate exec/test/record steps — those are only for scripted (no-agent)
    // looms, which run as a plain command pipeline.
    const agentic = (weave.steps || []).some((s) => s.action === 'agent');
    const out = [];
    (weave.steps || []).forEach((s) => {
      const w = s.with || {};
      const cmd0 = w.cmd || '';
      if (agentic && (s.action === 'exec' || s.action === 'test' || s.action === 'record')) return; // agent handles these
      if (provider === 'local') {
        if (s.action === 'exec' || s.action === 'test') out.push({ action: s.action, show: cmd0 || (s.action === 'test' ? 'npm test' : 'true'), cmd: cmd0 || (s.action === 'test' ? 'npm test' : 'true') });
        else if (s.action === 'agent') out.push({ action: s.action, show: 'claude — build · test · fix until green', cmd: 'claude -p --verbose "$(cat ' + GOAL_FILE + ')"' });
        else if (s.action === 'artifacts') out.push({ action: s.action, show: 'collect outputs (local)', cmd: 'true' });
        return;
      }
      switch (s.action) {
        case 'provision': out.push({ action: s.action, show: 'gitvm warm-up', cmd: 'gitvm warm-up' }); break;
        case 'sync': break; // implicit in `gitvm run`
        case 'exec': out.push({ action: s.action, show: 'gitvm run ' + (cmd0 || 'true'), cmd: "gitvm run 'cd /workspace && " + (cmd0 || 'true') + "'" }); break;
        case 'agent': out.push({ action: s.action, show: 'agent — build · test · fix until green (tmux/desktop)', cmd: "gitvm run 'bash /workspace/.loom-agent.sh'" }); break;
        case 'test': out.push({ action: s.action, show: 'gitvm run ' + (cmd0 || 'npm test'), cmd: "gitvm run 'cd /workspace && " + (cmd0 || 'npm test') + "'" }); break;
        case 'record': out.push({ action: s.action, show: 'ffmpeg auto-captures :0 (headed run)', cmd: 'true' }); break;
        case 'artifacts': out.push({ action: s.action, show: 'gitvm artifacts pull', cmd: 'gitvm artifacts pull' }); break;
        case 'teardown': out.push({ action: s.action, show: 'gitvm stop', cmd: 'gitvm stop' }); break;
        case 'spawn': out.push({ action: s.action, show: 'spawn sub-agents (mesh)', cmd: 'true' }); break;
        default: out.push({ action: s.action, show: '(' + s.action + ')', cmd: 'true' });
      }
    });
    return out;
  }

  // The prompt the agent actually gets: the human goal + the loom's acceptance +
  // an explicit iterate-until-green directive. THIS is the loop — the agent runs
  // the build/tests, sees failures, fixes them, and repeats until acceptance
  // passes, then stops. Without this the agent runs once and never self-corrects.
  function enrichGoal(weave, goal) {
    const acc = (weave.acceptance || []).filter(Boolean);
    const parts = [String(goal || '').trim()];
    parts.push('\nWork iteratively until it actually works: implement, then RUN the build and tests. If anything fails or errors, FIX it and re-run — repeat until everything passes. Do not stop while tests are red.');
    if (acc.length) parts.push('\nAcceptance — every item must pass before you are done:\n' + acc.map((a) => '- ' + a).join('\n'));
    parts.push('\nWhen all acceptance criteria pass, stop and report what you changed + the test results.');
    return parts.join('\n');
  }

  // Structural check — the `terraform validate` of a loom.
  function verifyWeave(weave) {
    const issues = [];
    if (!weave || typeof weave !== 'object') return { ok: false, issues: ['not a loom object'], provider: '', steps: 0, actions: 0 };
    if (weave.spec !== 'nautloom/v1') issues.push('spec must be "nautloom/v1" (got ' + JSON.stringify(weave.spec) + ')');
    if (weave.kind !== 'Weave') issues.push('kind must be "Weave"');
    if (!weave.metadata || !String(weave.metadata.name || '').trim()) issues.push('metadata.name is required');
    const provider = (weave.runtime && weave.runtime.provider) || '';
    if (!PROVIDERS[provider]) issues.push('unknown provider ' + JSON.stringify(provider) + ' (known: ' + Object.keys(PROVIDERS).join(', ') + ')');
    const steps = (weave.steps || []);
    let known = 0;
    steps.forEach((s, i) => {
      if (!s || !s.action) issues.push('step ' + (i + 1) + ' has no action');
      else if (ACTIONS.indexOf(s.action) < 0) issues.push('step ' + (i + 1) + ': unknown action ' + JSON.stringify(s.action));
      else known++;
    });
    return { ok: issues.length === 0, issues: issues, provider: provider, steps: steps.length, actions: known };
  }

  const ICON = {
    // artifact / link — Agentic empty state
    artifact: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.4" width="26" height="26"><path d="M8 12h8M8 8h8M8 16h5"/><rect x="4" y="3" width="16" height="18" rx="2"/></svg>',
    // sandbox / play — Loops empty state
    loop: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.4" width="26" height="26"><path d="M21 12a9 9 0 1 1-3-6.7"/><path d="M21 4v4h-4"/><path d="M10.5 9.5l4 2.5-4 2.5z" fill="currentColor" stroke="none"/></svg>',
  };

  function injectStyles() {
    if (document.getElementById('right-pane-workspace-styles')) return;
    const s = document.createElement('style');
    s.id = 'right-pane-workspace-styles';
    s.textContent = `
.rpws { display:flex; flex-direction:column; height:100%; min-height:0; }
.rpws-head { flex:0 0 auto; padding:9px 12px 0; }
.rpws-scope { font-size:11px; color:var(--muted-foreground); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
.rpws-scope strong { color:var(--foreground); font-weight:650; }
.rpws-nav { display:flex; gap:18px; padding:8px 12px 0; border-bottom:1px solid var(--border); flex:0 0 auto; }
.rpws-nav button { padding:6px 0 8px; border:0; border-bottom:2px solid transparent; background:transparent; color:var(--muted-foreground); font:inherit; font-size:12px; cursor:pointer; }
.rpws-nav button:hover { color:var(--foreground); }
.rpws-nav button.active { border-bottom-color:var(--xnaut-yellow); color:var(--foreground); font-weight:650; }
.rpws-body { flex:1 1 auto; min-height:0; overflow-y:auto; }
.rpws-page { display:none; flex-direction:column; height:100%; min-height:0; }
.rpws-page.active { display:flex; }
.rpws-empty { flex:1 1 auto; display:flex; flex-direction:column; align-items:center; justify-content:center; gap:10px; padding:28px 20px; text-align:center; color:var(--muted-foreground); }
.rpws-empty .rpws-ico { color:var(--muted-foreground); opacity:.55; }
.rpws-empty h3 { margin:2px 0 0; font-size:13px; color:var(--foreground); font-weight:650; }
.rpws-empty p { margin:0; font-size:12px; line-height:1.5; max-width:34ch; }
.rpws-phase { font-size:10px; letter-spacing:.04em; text-transform:uppercase; color:var(--muted-foreground); border:1px solid var(--border); border-radius:999px; padding:2px 8px; margin-top:4px; }
.rpws-actions { flex:0 0 auto; display:flex; gap:8px; padding:10px 12px; border-top:1px solid var(--border); }
.rpws-btn { flex:1 1 auto; min-height:32px; padding:6px 12px; border:1px solid var(--border); border-radius:var(--radius-md,8px); background:transparent; color:var(--foreground); font:inherit; font-size:12px; cursor:pointer; }
.rpws-btn:hover:not(:disabled) { background:var(--accent,#2a2a2f); }
.rpws-btn:disabled { opacity:.4; cursor:default; }
.rpws-btn-primary { border-color:var(--xnaut-yellow); background:var(--xnaut-yellow); color:var(--primary-foreground); font-weight:650; }
.rpws-feed-head { display:flex; align-items:center; gap:8px; padding:8px 12px 4px; flex:0 0 auto; }
.rpws-feed-head .rpws-count { font-size:10px; letter-spacing:.04em; text-transform:uppercase; color:var(--muted-foreground); white-space:nowrap; }
.rpws-session { flex:1 1 auto; min-width:0; height:26px; padding:2px 6px; border:1px solid var(--border); border-radius:var(--radius-md,6px); background:var(--secondary,#262626); color:var(--foreground); font:inherit; font-size:11px; outline:none; }
.rpws-session:focus { border-color:var(--xnaut-yellow); }
.rpws-icon-btn { width:26px; height:26px; display:flex; align-items:center; justify-content:center; border:1px solid transparent; border-radius:6px; background:transparent; color:var(--muted-foreground); cursor:pointer; }
.rpws-icon-btn:hover { background:var(--accent,#2a2a2f); color:var(--foreground); }
.rpws-icon-btn svg { width:14px; height:14px; }
.rpws-list { display:flex; flex-direction:column; gap:1px; padding:2px 8px 10px; }
.rpws-item { display:flex; align-items:center; gap:10px; padding:7px 9px; border-radius:var(--radius-md,8px); cursor:pointer; text-align:left; appearance:none; -webkit-appearance:none; background:transparent; border:0; border-left:2px solid transparent; color:inherit; }
.rpws-item:hover { background:var(--xnaut-yellow); background:color-mix(in srgb, var(--xnaut-yellow) 80%, #fff); }
.rpws-item:hover .rpws-item-label, .rpws-item:hover .rpws-item-sub { color:var(--primary-foreground,#171717); }
.rpws-item:hover .rpws-item-sub { opacity:.72; }
.rpws-item:hover .rpws-item-mark { background:rgba(0,0,0,.14); color:var(--primary-foreground,#171717); }
.rpws-item-mark { flex:0 0 auto; width:26px; height:26px; display:flex; align-items:center; justify-content:center; border-radius:6px; background:var(--bg-tertiary); color:var(--muted-foreground); }
.rpws-item-mark svg { width:15px; height:15px; }
.rpws-item-kind-artifact { border-left-color:var(--xnaut-yellow); }
.rpws-item-kind-artifact .rpws-item-mark { color:var(--xnaut-yellow); }
.rpws-item-copy { min-width:0; flex:1 1 auto; display:flex; flex-direction:column; gap:1px; }
.rpws-item-label { font-size:12.5px; font-weight:550; color:var(--foreground); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
.rpws-item-sub { font-size:10px; color:var(--muted-foreground); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
.rpws-pb-chips { display:flex; flex-wrap:wrap; gap:6px; padding:10px 12px 4px; flex:0 0 auto; }
.rpws-pb-chip { appearance:none; -webkit-appearance:none; background:transparent; color:var(--muted-foreground); border:1px solid var(--border); border-radius:999px; padding:4px 10px; font:inherit; font-size:11px; cursor:pointer; }
.rpws-pb-chip:hover { color:var(--foreground); }
.rpws-pb-chip.active { background:var(--xnaut-yellow); border-color:var(--xnaut-yellow); color:var(--primary-foreground,#171717); font-weight:650; }
.rpws-pb-desc { padding:4px 12px 8px; font-size:11px; color:var(--muted-foreground); line-height:1.45; flex:0 0 auto; }
.rpws-pb-workload { flex:1 1 auto; min-height:110px; margin:0 12px; padding:8px 10px; border:1px solid var(--border); border-radius:var(--radius-md,8px); background:var(--secondary,#262626); color:var(--foreground); font:inherit; font-size:12px; line-height:1.5; resize:none; outline:none; }
.rpws-pb-workload:focus { border-color:var(--xnaut-yellow); }
.rpws-pb-status { flex:0 0 auto; padding:0 12px 10px; font-size:11px; color:var(--muted-foreground); min-height:14px; }
.rpws-pb-overlay { position:fixed; inset:0; z-index:960; display:flex; align-items:center; justify-content:center; background:rgba(0,0,0,.5); }
.rpws-pb-dialog { width:min(360px,90vw); background:var(--card,#171717); border:1px solid var(--border); border-radius:10px; padding:16px; display:flex; flex-direction:column; gap:10px; box-shadow:0 20px 60px rgba(0,0,0,.5); }
.rpws-pb-dialog-head { font-size:12px; font-weight:650; color:var(--foreground); }
.rpws-pb-dialog-wide { width:min(560px,94vw); }
.rpws-pb-json { width:100%; min-height:320px; padding:10px 11px; border:1px solid var(--border); border-radius:var(--radius-md,8px); background:var(--secondary,#262626); color:var(--foreground); font-family:ui-monospace,SFMono-Regular,Menlo,monospace; font-size:11.5px; line-height:1.5; resize:vertical; outline:none; white-space:pre; }
.rpws-pb-json:focus { border-color:var(--xnaut-yellow); }
.rpws-runs { display:flex; flex-direction:column; gap:2px; padding:8px 10px 2px; flex:0 0 auto; }
.rpws-runs:empty { display:none; }
.rpws-run { display:flex; align-items:center; gap:7px; padding:4px 8px; border-radius:var(--radius-md,6px); background:var(--secondary,#262626); font-size:11px; color:var(--foreground); }
.rpws-run-dot { flex:0 0 auto; width:6px; height:6px; border-radius:50%; background:var(--xnaut-yellow); box-shadow:0 0 0 3px color-mix(in srgb, var(--xnaut-yellow) 22%, transparent); }
.rpws-run.done { opacity:.5; }
.rpws-run.done .rpws-run-dot { background:var(--muted-foreground); box-shadow:none; }
.rpws-run-name { flex:0 0 auto; font-weight:600; }
.rpws-run-goal { flex:1 1 auto; min-width:0; color:var(--muted-foreground); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
.rpws-run-time { flex:0 0 auto; color:var(--muted-foreground); font-size:10px; }
.rpws-run-x { flex:0 0 auto; appearance:none; -webkit-appearance:none; width:18px; height:18px; border:0; border-radius:4px; background:transparent; color:var(--muted-foreground); font-size:11px; line-height:1; cursor:pointer; }
.rpws-run-x:hover { background:var(--xnaut-yellow); color:var(--primary-foreground,#171717); }
.rpws-pb-tools { display:flex; gap:14px; padding:0 12px 6px; flex:0 0 auto; }
.rpws-linkbtn { appearance:none; -webkit-appearance:none; background:transparent; border:0; color:var(--muted-foreground); font:inherit; font-size:11px; cursor:pointer; padding:0; }
.rpws-linkbtn:hover { color:var(--xnaut-yellow); }

/* ── Looms runner ─────────────────────────────────────────────── */
.rpwl-label { display:flex; align-items:center; justify-content:space-between; padding:13px 12px 6px; flex:0 0 auto; }
.rpwl-label .k { font-size:10px; letter-spacing:.09em; text-transform:uppercase; color:var(--muted-foreground); font-weight:650; }
.rpwl-loomlist { display:flex; flex-direction:column; gap:2px; padding:0 8px; flex:0 0 auto; }
.rpwl-loom { display:flex; align-items:center; gap:10px; padding:8px; border-radius:var(--radius-md,7px); border-left:2px solid transparent; cursor:pointer; appearance:none; -webkit-appearance:none; background:transparent; border-top:0; border-right:0; border-bottom:0; color:inherit; text-align:left; width:100%; font:inherit; }
.rpwl-loom:hover { background:var(--secondary,#262626); }
.rpwl-loom.active { border-left-color:var(--xnaut-yellow); background:var(--card,#171717); }
.rpwl-loom .glyph { flex:0 0 auto; width:15px; height:15px; display:flex; flex-direction:column; justify-content:center; gap:2.5px; }
.rpwl-loom .glyph i { height:1.5px; border-radius:2px; background:var(--muted-foreground); display:block; }
.rpwl-loom.active .glyph i { background:var(--xnaut-yellow); }
.rpwl-loom .nm { flex:1 1 auto; font-family:ui-monospace,SFMono-Regular,Menlo,monospace; font-size:12.5px; color:var(--muted-foreground); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
.rpwl-loom.active .nm { color:var(--foreground); font-weight:550; }
.rpwl-loom .ct { flex:0 0 auto; font-size:10px; color:var(--muted-foreground); }
.rpwl-sel { display:flex; flex-direction:column; gap:9px; padding:15px 12px 12px; border-top:1px solid var(--border); flex:0 0 auto; }
.rpwl-sel .name { font-family:ui-monospace,SFMono-Regular,Menlo,monospace; font-size:15px; color:var(--foreground); font-weight:650; }
.rpwl-sel .desc { font-size:12px; line-height:1.5; color:var(--muted-foreground); }
.rpwl-chips { display:flex; gap:6px; flex-wrap:wrap; }
.rpwl-chip { font-size:10px; color:var(--foreground); border:1px solid var(--border); padding:3px 8px; border-radius:999px; }
.rpwl-chip.prov { background:var(--xnaut-yellow); color:var(--primary-foreground,#171717); font-weight:650; border-color:var(--xnaut-yellow); font-family:ui-monospace,Menlo,monospace; }
.rpwl-tools { display:flex; gap:14px; }
.rpwl-goal-h { display:flex; align-items:center; gap:7px; }
.rpwl-goal-h .k { font-size:10px; letter-spacing:.08em; text-transform:uppercase; color:var(--muted-foreground); font-weight:650; }
.rpwl-goal-h .hint { font-size:9px; letter-spacing:.04em; text-transform:uppercase; color:var(--xnaut-yellow); }
.rpwl-goal-h .load { margin-left:auto; }
.rpwl-origin { display:inline-flex; align-items:center; gap:5px; align-self:flex-start; font-family:ui-monospace,Menlo,monospace; font-size:10px; color:var(--xnaut-yellow); border:1px solid color-mix(in srgb,var(--xnaut-yellow) 45%,transparent); border-radius:999px; padding:2px 8px; }
.rpwl-origin b { cursor:pointer; opacity:.7; }
.rpwl-goal { min-height:58px; padding:10px 11px; border:1px solid var(--border); border-left:2px solid var(--xnaut-yellow); border-radius:var(--radius-md,8px) var(--radius-md,8px) 0 0; background:var(--secondary,#262626); color:var(--foreground); font:inherit; font-size:12px; line-height:1.5; resize:none; outline:none; display:block; }
.rpwl-goal:focus { border-color:var(--xnaut-yellow); }
.rpwl-goal-resize { height:9px; margin-top:-1px; border:1px solid var(--border); border-top:0; border-left:2px solid var(--xnaut-yellow); border-radius:0 0 var(--radius-md,8px) var(--radius-md,8px); background:var(--secondary,#262626); cursor:ns-resize; display:flex; align-items:center; justify-content:center; }
.rpwl-goal-resize:hover { background:var(--accent,#2a2a2f); }
.rpwl-goal-resize span { width:24px; height:2px; border-radius:2px; background:var(--muted-foreground); opacity:.5; }
.rpwl-goal-resize:hover span { opacity:.9; }
.rpwl-runbar { display:flex; gap:7px; padding:0 12px 12px; flex:0 0 auto; }
.rpwl-b { display:flex; align-items:center; justify-content:center; gap:5px; height:34px; border:1px solid var(--border); border-radius:var(--radius-md,8px); background:transparent; color:var(--foreground); font:inherit; font-size:12px; cursor:pointer; }
.rpwl-b.grow { flex:1 1 auto; }
.rpwl-b:hover:not(:disabled) { background:var(--accent,#2a2a2f); }
.rpwl-b:disabled { opacity:.4; cursor:default; }
.rpwl-b.run { flex:1 1 auto; border-color:var(--xnaut-yellow); background:var(--xnaut-yellow); color:var(--primary-foreground,#171717); font-weight:650; }
.rpwl-b.stop { flex:1 1 auto; border-color:rgba(220,110,100,.55); background:transparent; color:#e98b83; font-weight:650; }
.rpwl-out { flex:1 1 auto; min-height:150px; display:flex; flex-direction:column; border-top:1px solid var(--border); background:var(--bg-deep,#0a0b0e); }
.rpwl-out-h { display:flex; align-items:center; justify-content:space-between; padding:8px 12px; border-bottom:1px solid var(--border); flex:0 0 auto; }
.rpwl-out-h .k { font-size:10px; letter-spacing:.09em; text-transform:uppercase; color:var(--muted-foreground); font-weight:650; }
.rpwl-out-h .state { font-size:9.5px; letter-spacing:.06em; text-transform:uppercase; border:1px solid var(--border); border-radius:999px; padding:2px 8px; font-family:ui-monospace,Menlo,monospace; color:var(--muted-foreground); }
.rpwl-out-h .state.run { color:var(--xnaut-yellow); border-color:color-mix(in srgb,var(--xnaut-yellow) 45%,transparent); }
.rpwl-sb { margin-left:auto; margin-right:10px; font-family:ui-monospace,Menlo,monospace; font-size:10px; color:var(--muted-foreground); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
.rpwl-sb:empty { display:none; }
.rpwl-sb a { color:var(--xnaut-yellow); cursor:pointer; text-decoration:none; }
.rpwl-sb a:hover { text-decoration:underline; }
.rpwl-log { flex:1 1 auto; overflow-y:auto; padding:11px 12px 16px; margin:0; font-family:ui-monospace,SFMono-Regular,Menlo,monospace; font-size:11px; line-height:1.55; color:#aeb4bd; white-space:pre-wrap; word-break:break-word; }
.rpwl-log .ok { color:#7ec98f; } .rpwl-log .warn { color:#e98b83; } .rpwl-log .step { color:var(--xnaut-yellow); } .rpwl-log .dim { color:var(--muted-foreground); }
.rpwl-ed-body { display:flex; flex-direction:column; gap:12px; max-height:56vh; overflow-y:auto; padding-right:2px; }
.rpwl-ed-f { display:flex; flex-direction:column; gap:6px; }
.rpwl-ed-f > span { font-size:10px; letter-spacing:.08em; text-transform:uppercase; color:var(--muted-foreground); font-weight:650; }
.rpwl-ed-in { background:var(--secondary,#262626); border:1px solid var(--border); border-radius:var(--radius-md,8px); color:var(--foreground); font:inherit; font-size:12px; padding:8px 10px; outline:none; }
.rpwl-ed-in.mono { font-family:ui-monospace,SFMono-Regular,Menlo,monospace; }
.rpwl-ed-in:focus { border-color:var(--xnaut-yellow); }
textarea.rpwl-ed-in { resize:vertical; line-height:1.5; }
.rpwl-ed-steps { display:flex; flex-direction:column; gap:6px; }
.rpwl-ed-step { display:flex; gap:6px; align-items:center; }
.rpwl-ed-step .rpwl-ed-in[data-sk="id"] { flex:0 0 84px; }
.rpwl-ed-step select.rpwl-ed-in { flex:0 0 108px; }
.rpwl-ed-step .rpwl-ed-in[data-sk="cmd"] { flex:1 1 auto; min-width:0; }
.rpwl-ed-x { flex:0 0 auto; width:24px; height:24px; border:0; border-radius:6px; background:transparent; color:var(--muted-foreground); cursor:pointer; font-size:12px; }
.rpwl-ed-x:hover { background:var(--accent,#2a2a2f); color:var(--foreground); }
`;
    document.head.appendChild(s);
  }

  function createWorkspaceView() {
    let container = null;
    let root = null;
    let active = localStorage.getItem(SUBTAB_KEY) || 'agentic';
    let sessionId = '';        // '' = newest; else a specific session to backscan
    let sessions = [];
    let loomSelected = '';   // path of the active loom (*.loom.json)
    let looms = [];          // WeaveMeta[] from the library
    let loomWeave = null;    // full read Weave of the selected loom
    let goalText = '';       // intent.goal for this run (the human's prompt)
    let origin = null;       // {id:'XNAUT-39'} when this run came from a ticket
    let running = false;     // a loom is executing
    let runHandle = null;    // {pid, log} of the active run
    let pending = null;      // {goal, origin} from a ticket, applied on next load
    let goalHeight = parseInt(localStorage.getItem('xnaut-loom-goal-h'), 10) || 0; // px, user-chosen
    let sandboxId = '';      // parsed from the run log (gitvm warm-up output)
    let desktopUrl = '';     // noVNC desktop URL to watch the sandbox live

    function scopeKey() { return root ? `${SUBTAB_KEY}:${root}` : SUBTAB_KEY; }

    const REFRESH_ICON = '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M13.5 8a5.5 5.5 0 1 1-1.6-3.9"/><path d="M13.5 1.5v3h-3"/></svg>';
    const FILE_ICON = '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3"><path d="M4 1.5h5l3 3V14a.5.5 0 0 1-.5.5h-7.5A.5.5 0 0 1 3.5 14V2a.5.5 0 0 1 .5-.5z"/><path d="M9 1.5v3h3"/></svg>';
    const LINK_ICON = '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3"><path d="M6.5 9.5a2.5 2.5 0 0 0 3.5 0l2-2a2.5 2.5 0 0 0-3.5-3.5l-1 1"/><path d="M9.5 6.5a2.5 2.5 0 0 0-3.5 0l-2 2a2.5 2.5 0 0 0 3.5 3.5l1-1"/></svg>';

    function agenticPage() {
      // Live feed of clean captures: artifacts (links) + created documents.
      return `<div class="rpws-page" data-page="agentic">
        <div class="rpws-feed-head">
          <select class="rpws-session" data-session title="Session to read (backscan any past session)"></select>
          <span class="rpws-count" data-count></span>
          <button class="rpws-icon-btn" data-act="refresh" title="Rescan">${REFRESH_ICON}</button>
        </div>
        <div class="rpws-list" data-list>
          <div class="rpws-empty"><div class="rpws-ico">${ICON.artifact}</div><h3>Loading…</h3></div>
        </div>
      </div>`;
    }

    function relTime(ms) {
      if (!ms) return '';
      const s = Math.max(0, (Date.now() - ms) / 1000);
      if (s < 90) return 'just now';
      if (s < 5400) return `${Math.round(s / 60)}m ago`;
      if (s < 129600) return `${Math.round(s / 3600)}h ago`;
      return `${Math.round(s / 86400)}d ago`;
    }

    async function loadSessions() {
      const sel = container && container.querySelector('[data-session]');
      if (!sel || !root) return;
      try { sessions = (await invoke('workspace_sessions', { projectPath: root })) || []; }
      catch (_) { sessions = []; }
      const opts = ['<option value="">Newest session (auto)</option>'].concat(
        sessions.map((s) => {
          const glyph = s.agent === 'codex' ? '⬡' : '✳';
          return `<option value="${escapeText(s.id)}">${glyph} ${escapeText(s.title)} · ${relTime(s.updated_ms)}</option>`;
        }),
      );
      sel.innerHTML = opts.join('');
      sel.value = sessionId;
      sel.onchange = () => { sessionId = sel.value; loadItems(); };
    }

    async function loadAgentic() { await loadSessions(); await loadItems(); }

    async function loadItems() {
      const page = container && container.querySelector('.rpws-page[data-page="agentic"]');
      if (!page || !root) return;
      const list = page.querySelector('[data-list]');
      const count = page.querySelector('[data-count]');
      let items = [];
      const args = { projectPath: root };
      if (sessionId) args.session = sessionId;
      try { items = (await invoke('workspace_agentic_items', args)) || []; }
      catch (e) { console.error('[workspace] workspace_agentic_items failed', e); list.innerHTML = `<div class="rpws-empty"><p>${escapeText(String((e && e.message) || e))}</p></div>`; return; }
      count.textContent = items.length ? `${items.length} captured` : '';
      if (!items.length) {
        list.innerHTML = `<div class="rpws-empty">
          <div class="rpws-ico">${ICON.artifact}</div>
          <h3>Nothing captured yet</h3>
          <p>Artifacts and documents this session produces show up here — so a served HTML page or a report link survives scrollback. Brainstorm/idea capture lands next.</p>
          <span class="rpws-phase">Capture · artifacts + documents</span></div>`;
        return;
      }
      // Newest first.
      items = items.slice().reverse();
      list.innerHTML = items.map((it, i) => {
        const isArt = it.kind === 'artifact';
        const sub = isArt ? it.target : it.target.replace(/^.*\/(?=[^/]+\/[^/]+$)/, '…/');
        return `<button class="rpws-item rpws-item-kind-${escapeText(it.kind)}" data-i="${i}">
          <span class="rpws-item-mark">${isArt ? LINK_ICON : FILE_ICON}</span>
          <span class="rpws-item-copy"><span class="rpws-item-label">${escapeText(it.label)}</span><span class="rpws-item-sub">${escapeText(sub)}</span></span>
        </button>`;
      }).join('');
      list.querySelectorAll('.rpws-item').forEach((el) => {
        el.onclick = () => openItem(items[+el.dataset.i]);
      });
    }

    function loopsPage() {
      // Loom runner: pick a loom → set the goal (the one thing you write) →
      // Verify (validate) · Dry-run (plan) · Run (execute + live log).
      return `<div class="rpws-page" data-page="loops">
        <div class="rpwl-label"><span class="k">Looms</span><button class="rpws-linkbtn" data-act="loom-new" title="Create a new loom">+ New</button></div>
        <div class="rpwl-loomlist" data-loom-list></div>
        <div class="rpwl-sel" data-loom-sel></div>
        <div class="rpwl-runbar" data-loom-runbar></div>
        <div class="rpwl-out">
          <div class="rpwl-out-h"><span class="k">Output</span><span class="rpwl-sb" data-loom-sb></span><span class="state" data-loom-state>idle</span></div>
          <pre class="rpwl-log" data-loom-log></pre>
        </div>
      </div>`;
    }

    function loomsEls() {
      const page = container && container.querySelector('.rpws-page[data-page="loops"]');
      if (!page) return null;
      return { page, list: page.querySelector('[data-loom-list]'), sel: page.querySelector('[data-loom-sel]'),
        runbar: page.querySelector('[data-loom-runbar]'), state: page.querySelector('[data-loom-state]'), log: page.querySelector('[data-loom-log]') };
    }
    function setState(txt, cls) { const e = loomsEls(); if (e && e.state) { e.state.textContent = txt; e.state.className = 'state' + (cls ? ' ' + cls : ''); } }
    function outReset() { const e = loomsEls(); if (e && e.log) e.log.innerHTML = ''; }
    function outLine(text, cls) {
      const e = loomsEls(); if (!e || !e.log) return;
      const div = document.createElement('div'); if (cls) div.className = cls; div.textContent = text;
      e.log.appendChild(div); e.log.scrollTop = e.log.scrollHeight;
    }
    function metaOf() { return looms.find((l) => l.path === loomSelected); }
    function renderSandbox() {
      const e = loomsEls(); if (!e) return;
      const slot = e.page.querySelector('[data-loom-sb]'); if (!slot) return;
      if (!sandboxId && !desktopUrl) { slot.innerHTML = ''; return; }
      slot.innerHTML = `${escapeText(sandboxId || 'sandbox')}${desktopUrl ? ' · <a data-open-desktop title="Watch the sandbox live">open desktop ↗</a>' : ''}`;
      const a = slot.querySelector('[data-open-desktop]');
      if (a) a.onclick = () => { if (!openInternalBrowser(desktopUrl)) openTarget(desktopUrl); };
    }

    async function loadLooms() {
      const e = loomsEls(); if (!e) return;
      try { await invoke('looms_seed_defaults'); } catch (_) {}
      try { looms = (await invoke('looms_list')) || []; } catch (_) { looms = []; }
      if (pending) { goalText = pending.goal || ''; origin = pending.origin || null; pending = null; }
      const nb = e.page.querySelector('[data-act="loom-new"]'); if (nb) nb.onclick = () => openLoomEditor(null);
      if (!looms.length) {
        e.list.innerHTML = '<div class="rpws-empty" style="padding:22px 16px"><p>No looms yet — <b style="color:var(--xnaut-yellow);cursor:pointer" data-act="loom-new2">create one</b>.</p></div>';
        e.sel.innerHTML = ''; e.runbar.innerHTML = '';
        const n2 = e.list.querySelector('[data-act="loom-new2"]'); if (n2) n2.onclick = () => openLoomEditor(null);
        return;
      }
      if (!looms.find((l) => l.path === loomSelected)) loomSelected = looms[0].path;
      await selectLoom(loomSelected, true);
      maybeResumeRun();
    }

    function renderLoomList() {
      const e = loomsEls(); if (!e) return;
      e.list.innerHTML = looms.map((l) => `<button class="rpwl-loom${l.path === loomSelected ? ' active' : ''}" data-path="${escapeText(l.path)}"><span class="glyph"><i></i><i></i><i></i></span><span class="nm">${escapeText(l.name)}</span></button>`).join('');
      e.list.querySelectorAll('.rpwl-loom').forEach((b) => { b.onclick = () => selectLoom(b.dataset.path, false); });
    }

    async function selectLoom(path, keepGoal) {
      loomSelected = path;
      renderLoomList();
      const meta = looms.find((l) => l.path === path);
      try { loomWeave = await invoke('loom_read', { path }); } catch (_) { loomWeave = null; }
      if (!keepGoal) { origin = origin && keepGoal ? origin : origin; }
      // Seed the goal from the loom's own intent.goal when the box is empty.
      if (!goalText) { const g = loomWeave && loomWeave.intent && loomWeave.intent.goal; if (g) goalText = g; }
      renderSelected(meta); renderRunbar();
    }

    function renderSelected(meta) {
      const e = loomsEls(); if (!e) return;
      const w = loomWeave;
      const provider = (w && w.runtime && w.runtime.provider) || (meta && meta.provider) || '';
      const steps = ((w && w.steps) || []).length;
      const acc = ((w && w.acceptance) || []).length;
      const originChip = origin ? `<span class="rpwl-origin">from ${escapeText(origin.id)}<b data-clear-origin title="clear">✕</b></span>` : '';
      e.sel.innerHTML = `
        <div class="name">${escapeText(meta ? meta.name : '')}</div>
        <div class="desc">${escapeText((meta && meta.description) || '')}</div>
        <div class="rpwl-chips">${provider ? `<span class="rpwl-chip prov">${escapeText(provider)}</span>` : ''}<span class="rpwl-chip">${steps} steps</span><span class="rpwl-chip">acceptance · ${acc}</span></div>
        <div class="rpwl-tools"><button class="rpws-linkbtn" data-act="loom-edit">Edit</button></div>
        ${originChip ? `<div>${originChip}</div>` : ''}
        <div class="rpwl-goal-h"><span class="k">Goal · this run</span><span class="hint">the only thing you write</span><button class="rpws-linkbtn load" data-act="loom-ticket">Load ticket…</button></div>
        <textarea class="rpwl-goal" data-loom-goal placeholder="What should this run achieve?"></textarea>
        <div class="rpwl-goal-resize" data-goal-resize title="Drag to resize"><span></span></div>`;
      const ta = e.sel.querySelector('[data-loom-goal]');
      ta.value = goalText; ta.oninput = () => { goalText = ta.value; };
      if (goalHeight) ta.style.height = goalHeight + 'px';
      const rh = e.sel.querySelector('[data-goal-resize]');
      if (rh) {
        let sy = 0, sh = 0;
        const move = (ev) => { goalHeight = Math.max(48, Math.min(600, sh + (ev.clientY - sy))); ta.style.height = goalHeight + 'px'; };
        const up = () => { document.removeEventListener('pointermove', move); document.removeEventListener('pointerup', up); try { localStorage.setItem('xnaut-loom-goal-h', String(goalHeight)); } catch (_) {} };
        rh.onpointerdown = (ev) => { ev.preventDefault(); sy = ev.clientY; sh = ta.offsetHeight; document.addEventListener('pointermove', move); document.addEventListener('pointerup', up); };
      }
      const ed = e.sel.querySelector('[data-act="loom-edit"]'); if (ed) ed.onclick = () => openLoomEditor(loomWeave);
      const lt = e.sel.querySelector('[data-act="loom-ticket"]'); if (lt) lt.onclick = () => loadTicketInto();
      const co = e.sel.querySelector('[data-clear-origin]'); if (co) co.onclick = () => { origin = null; renderSelected(meta); };
    }

    function renderRunbar() {
      const e = loomsEls(); if (!e) return;
      if (running) {
        e.runbar.innerHTML = `<button class="rpwl-b grow" disabled>✓ Verify</button><button class="rpwl-b grow" disabled>⧉ Dry-run</button><button class="rpwl-b stop" data-act="loom-stop">■ Stop</button>`;
        e.runbar.querySelector('[data-act="loom-stop"]').onclick = () => stopRun(); return;
      }
      e.runbar.innerHTML = `<button class="rpwl-b grow" data-act="loom-verify">✓ Verify</button><button class="rpwl-b grow" data-act="loom-dry">⧉ Dry-run</button><button class="rpwl-b run" data-act="loom-run">▸ Run</button>`;
      e.runbar.querySelector('[data-act="loom-verify"]').onclick = () => doVerify();
      e.runbar.querySelector('[data-act="loom-dry"]').onclick = () => doDryRun();
      e.runbar.querySelector('[data-act="loom-run"]').onclick = () => runLoom();
    }

    function doVerify() {
      if (!loomWeave) return;
      outReset(); setState('verify');
      outLine('$ loom verify ' + (metaOf() ? metaOf().name : ''), 'dim');
      const v = verifyWeave(loomWeave);
      if (v.ok) outLine('✓ schema ok · provider ' + v.provider + ' · ' + v.actions + '/' + v.steps + ' actions known', 'ok');
      else { outLine('✗ ' + v.issues.length + ' issue' + (v.issues.length === 1 ? '' : 's') + ':', 'warn'); v.issues.forEach((i) => outLine('  · ' + i, 'warn')); }
    }

    function doDryRun() {
      if (!loomWeave) return;
      outReset(); setState('dry-run');
      const w = loomWeave, name = metaOf() ? metaOf().name : '';
      outLine('$ loom dry-run ' + name, 'dim');
      const v = verifyWeave(w);
      if (!v.ok) { outLine('✗ verify failed — fix before planning:', 'warn'); v.issues.forEach((i) => outLine('  · ' + i, 'warn')); return; }
      outLine('✓ verify — schema ok · ' + v.provider + ' · ' + v.actions + '/' + v.steps + ' actions', 'ok');
      outLine(''); outLine('PLAN · ' + v.provider + ' provider', 'dim');
      composeCommands(w, v.provider).forEach((c, i) => outLine('  ' + (i + 1) + '  ' + (c.action + '          ').slice(0, 10) + ' ' + c.show, 'step'));
      if ((w.acceptance || []).length) { outLine(''); outLine('ACCEPTANCE', 'dim'); w.acceptance.forEach((a) => outLine('  • ' + a)); }
      outLine(''); outLine('— nothing executed · press Run to apply —', 'dim');
    }

    async function runLoom() {
      if (running || !loomWeave) return;
      const goal = (goalText || '').trim();
      const v = verifyWeave(loomWeave);
      if (!v.ok) { doVerify(); return; }
      if (!goal && (loomWeave.steps || []).some((s) => s.action === 'agent')) {
        const e = loomsEls(); const ta = e && e.sel.querySelector('[data-loom-goal]'); if (ta) ta.focus();
        outReset(); setState(''); outLine('Set a Goal first — the agent step needs it.', 'warn'); return;
      }
      const name = metaOf() ? metaOf().name : 'loom';
      const script = composeCommands(loomWeave, v.provider).map((c) => 'echo "» ' + c.action + '"; ' + c.cmd).join('\n');
      outReset(); setState('running', 'run');
      sandboxId = ''; desktopUrl = ''; renderSandbox();
      outLine('$ loom run ' + name + '  ·  ' + v.provider, 'dim');
      const runId = 'run-' + Date.now();
      const agentic = (loomWeave.steps || []).some((s) => s.action === 'agent');
      const effGoal = agentic ? enrichGoal(loomWeave, goal) : goal;
      let h;
      try { h = await invoke('loom_run', { runId: runId, script: script, goal: effGoal, cwd: root }); }
      catch (err) { setState('failed'); outLine('could not start: ' + ((err && err.message) || err), 'warn'); return; }
      running = true; runHandle = h; renderRunbar();
      saveActiveRun({ runId: runId, log: h.log, pid: h.pid, name: name, root: root });
      try { await invoke('loom_run_record', { weave: name, goal: goal || '(no goal)', provider: v.provider }); } catch (_) {}
      tailLog(h.log);
    }

    // Persist the live run so a page reload can re-attach to its log (the driver
    // + remote agent survive a webview reload; only the DOM was lost).
    const ACTIVE_KEY = 'xnaut-loom-active';
    function saveActiveRun(a) { try { localStorage.setItem(ACTIVE_KEY, JSON.stringify(a)); } catch (_) {} }
    function clearActiveRun() { try { localStorage.removeItem(ACTIVE_KEY); } catch (_) {} }
    async function maybeResumeRun() {
      if (running) return;
      let a; try { a = JSON.parse(localStorage.getItem(ACTIVE_KEY) || 'null'); } catch (_) { a = null; }
      if (!a || !a.log || a.root !== root) return;
      let txt = ''; try { txt = (await invoke('read_file', { path: a.log })) || ''; } catch (_) { clearActiveRun(); return; }
      if (txt.indexOf('__LOOM_DONE__') >= 0) { clearActiveRun(); return; } // finished while away
      let alive = true; try { alive = await invoke('loom_run_alive', { pid: a.pid }); } catch (_) {}
      if (!alive) { clearActiveRun(); return; } // driver died — run is over
      running = true; runHandle = { pid: a.pid, log: a.log };
      outReset(); setState('running', 'run');
      outLine('↻ re-attached to ' + a.name + ' (still running)', 'dim');
      renderRunbar(); tailLog(a.log);
    }

    async function tailLog(logPath) {
      let seen = 0, stale = 0;
      const step = async () => {
        if (!running) return;
        let txt = '';
        try { txt = (await invoke('read_file', { path: logPath })) || ''; } catch (_) {}
        if (txt.length > seen) {
          stale = 0;
          const chunk = txt.slice(seen); seen = txt.length;
          chunk.split('\n').forEach((ln) => {
            if (ln === '' || ln.indexOf('__LOOM_DONE__') === 0) return;
            const sbm = ln.match(/sandbox:\s*(sb-\S+)/i); if (sbm) { sandboxId = sbm[1]; renderSandbox(); }
            const dm = ln.match(/(https?:\/\/\S*nautbox\.dev\/vnc\.html\S*)/i); if (dm) { desktopUrl = dm[1]; renderSandbox(); }
            let cls = '';
            if (ln.charAt(0) === '»') cls = 'step';
            else if (/error|fail|fatal|panic|✗/i.test(ln)) cls = 'warn';
            else if (/^✓|success|passed|compiled|ready|\bok\b/i.test(ln)) cls = 'ok';
            outLine(ln, cls);
          });
        }
        if (txt.indexOf('__LOOM_DONE__') >= 0) {
          const m = txt.match(/__LOOM_DONE__ (\d+)/); const code = m ? m[1] : '?';
          running = false; runHandle = null; clearActiveRun();
          outLine(''); outLine(code === '0' ? '✓ run finished · exit 0' : '✗ run exited · code ' + code, code === '0' ? 'ok' : 'warn');
          setState(code === '0' ? 'done' : 'failed'); renderRunbar(); return;
        }
        // No new output for a while → check the driver is still alive (agent may
        // just be thinking; only finalize if the process is actually gone).
        stale++;
        if (stale >= 10 && runHandle) {
          stale = 0;
          let alive = true; try { alive = await invoke('loom_run_alive', { pid: runHandle.pid }); } catch (_) {}
          if (!alive) { running = false; runHandle = null; clearActiveRun(); outLine(''); outLine('· driver ended without an exit marker (run detached or killed)', 'warn'); setState('ended'); renderRunbar(); return; }
        }
        setTimeout(step, 800);
      };
      step();
    }

    async function stopRun() {
      if (runHandle) { try { await invoke('loom_run_stop', { pid: runHandle.pid }); } catch (_) {} }
      running = false; runHandle = null; clearActiveRun();
      outLine('■ stopped', 'warn'); setState('stopped'); renderRunbar();
    }

    // Author/edit a loom in a form (not raw JSON) — obvious what a loom needs.
    // Save builds the Weave, validates via loom_write, reloads + selects it.
    const BLANK_LOOM = {
      spec: 'nautloom/v1', kind: 'Weave',
      metadata: { name: '', description: '', author: '48nauts', version: 1 },
      runtime: { provider: 'gitvm', template: 'agent-desktop', tools: [] },
      intent: { goal: '', inputs: [] },
      steps: [
        { id: 'warmup', action: 'provision' },
        { id: 'work', action: 'agent', with: { agent: 'claude', goal: '$intent.goal' } },
        { id: 'verify', action: 'test', with: { cmd: 'npm test' } },
        { id: 'collect', action: 'artifacts' },
      ],
      acceptance: [], report: { to: 'agentic', include: ['summary'] },
    };
    function openLoomEditor(weave) {
      const w = JSON.parse(JSON.stringify(weave || BLANK_LOOM));
      w.metadata = w.metadata || {}; w.runtime = w.runtime || {}; w.steps = w.steps || []; w.acceptance = w.acceptance || [];
      const ov = document.createElement('div'); ov.className = 'rpws-pb-overlay';
      ov.innerHTML = `<div class="rpws-pb-dialog rpws-pb-dialog-wide">
        <div class="rpws-pb-dialog-head">${weave ? 'Edit loom' : 'New loom'} <span style="font-weight:400;color:var(--muted-foreground);font-size:10px;font-family:ui-monospace,Menlo,monospace"> · ~/.config/xnaut/looms/</span></div>
        <div class="rpwl-ed-body">
          <label class="rpwl-ed-f"><span>Name</span><input class="rpwl-ed-in mono" data-f="name" value="${escapeText(w.metadata.name || '')}" placeholder="my-loom"></label>
          <label class="rpwl-ed-f"><span>Description</span><textarea class="rpwl-ed-in" data-f="desc" rows="2">${escapeText(w.metadata.description || '')}</textarea></label>
          <label class="rpwl-ed-f"><span>Provider</span><select class="rpwl-ed-in mono" data-f="provider">${Object.keys(PROVIDERS).map((p) => `<option${w.runtime.provider === p ? ' selected' : ''}>${p}</option>`).join('')}</select></label>
          <div class="rpwl-ed-f"><span>Steps</span><div class="rpwl-ed-steps" data-steps></div><button class="rpws-linkbtn" data-add-step>+ add step</button></div>
          <label class="rpwl-ed-f"><span>Acceptance · one per line</span><textarea class="rpwl-ed-in mono" data-f="acceptance" rows="3">${escapeText((w.acceptance || []).join('\n'))}</textarea></label>
        </div>
        <div class="rpws-pb-status" data-ed-status></div>
        <div class="rpws-actions"><button class="rpws-btn" data-x>Cancel</button><button class="rpws-btn rpws-btn-primary" data-ok>Save loom</button></div></div>`;
      container.appendChild(ov);
      const est = ov.querySelector('[data-ed-status]');
      const stepsHost = ov.querySelector('[data-steps]');
      let steps = (w.steps || []).map((s) => ({ id: s.id || '', action: s.action || 'exec', cmd: (s.with && s.with.cmd) || '' }));
      function paintSteps() {
        stepsHost.innerHTML = steps.map((s, i) => {
          const hasCmd = s.action === 'exec' || s.action === 'test';
          return `<div class="rpwl-ed-step">
            <input class="rpwl-ed-in mono" data-si="${i}" data-sk="id" value="${escapeText(s.id)}" placeholder="id">
            <select class="rpwl-ed-in mono" data-si="${i}" data-sk="action">${ACTIONS.map((a) => `<option${s.action === a ? ' selected' : ''}>${a}</option>`).join('')}</select>
            <input class="rpwl-ed-in mono" data-si="${i}" data-sk="cmd" value="${escapeText(s.cmd)}" placeholder="cmd"${hasCmd ? '' : ' style="visibility:hidden"'}>
            <button class="rpwl-ed-x" data-rm="${i}" title="remove">✕</button></div>`;
        }).join('');
        stepsHost.querySelectorAll('[data-si]').forEach((el) => { el.oninput = el.onchange = () => { const i = +el.dataset.si; steps[i][el.dataset.sk] = el.value; if (el.dataset.sk === 'action') paintSteps(); }; });
        stepsHost.querySelectorAll('[data-rm]').forEach((b) => { b.onclick = () => { steps.splice(+b.dataset.rm, 1); paintSteps(); }; });
      }
      paintSteps();
      ov.querySelector('[data-add-step]').onclick = () => { steps.push({ id: '', action: 'exec', cmd: '' }); paintSteps(); };
      const close = () => ov.remove();
      ov.querySelector('[data-x]').onclick = close;
      ov.onclick = (e) => { if (e.target === ov) close(); };
      ov.querySelector('[data-ok]').onclick = async () => {
        const name = ov.querySelector('[data-f="name"]').value.trim();
        if (!name) { est.textContent = 'Name is required.'; return; }
        const provider = ov.querySelector('[data-f="provider"]').value;
        const desc = ov.querySelector('[data-f="desc"]').value.trim();
        const acceptance = ov.querySelector('[data-f="acceptance"]').value.split('\n').map((x) => x.trim()).filter(Boolean);
        const outSteps = steps.filter((s) => s.id || s.action).map((s) => {
          const o = { id: s.id || s.action, action: s.action };
          if ((s.action === 'exec' || s.action === 'test') && s.cmd) o.with = { cmd: s.cmd };
          else if (s.action === 'agent') o.with = { agent: 'claude', goal: '$intent.goal' };
          return o;
        });
        const built = Object.assign({}, w, {
          spec: 'nautloom/v1', kind: 'Weave',
          metadata: Object.assign({}, w.metadata, { name: name, description: desc, author: w.metadata.author || '48nauts', version: w.metadata.version || 1 }),
          runtime: Object.assign({}, w.runtime, { provider: provider }),
          steps: outSteps, acceptance: acceptance,
        });
        try { const savedPath = await invoke('loom_write', { weave: built }); loomSelected = savedPath; goalText = ''; close(); await loadLooms(); }
        catch (e) { est.textContent = String((e && e.message) || e); }
      };
    }

    // ---- ticket ↔ loom bridge (one mechanism, two entry points) ----
    function ticketToGoal(t) {
      const docs = (t.documentation || []).filter(Boolean);
      let g = `${t.id}: ${t.title}`.trim();
      if (t.body && t.body.trim()) g += `\n\n${t.body.trim()}`;
      if (docs.length) g += `\n\nFollow the design doc${docs.length > 1 ? 's' : ''}: ${docs.join(', ')}.`;
      return g;
    }
    function applyTicket(t) { goalText = ticketToGoal(t); origin = { id: t.id }; renderSelected(metaOf()); }

    // Pull a ticket into the Goal — the mirror of the PM 3-dot "Create Loom".
    async function loadTicketInto() {
      let tickets = [];
      try { tickets = (await invoke('pm_ticket_list', { project: null })) || []; }
      catch (e) { outLine('Could not load tickets: ' + ((e && e.message) || e), 'warn'); return; }
      if (!tickets.length) { outLine('No tickets found.', 'warn'); return; }
      const ordered = tickets.filter((t) => t.status !== 'done').concat(tickets.filter((t) => t.status === 'done'));
      const ov = document.createElement('div'); ov.className = 'rpws-pb-overlay';
      ov.innerHTML = `<div class="rpws-pb-dialog"><div class="rpws-pb-dialog-head">Load a ticket into the Goal</div>
        <select class="rpws-session" data-pick>${ordered.map((t) => `<option value="${escapeText(t.id)}">${escapeText(t.id)} · ${escapeText(t.title)} · ${escapeText(t.status)}</option>`).join('')}</select>
        <div class="rpws-actions"><button class="rpws-btn" data-x>Cancel</button><button class="rpws-btn rpws-btn-primary" data-ok>Load</button></div></div>`;
      container.appendChild(ov);
      const close = () => ov.remove();
      ov.querySelector('[data-x]').onclick = close;
      ov.onclick = (e) => { if (e.target === ov) close(); };
      ov.querySelector('[data-ok]').onclick = () => { const id = ov.querySelector('[data-pick]').value; const t = tickets.find((x) => x.id === id); if (t) applyTicket(t); close(); };
    }

    function render() {
      if (!container) return;
      injectStyles();
      active = localStorage.getItem(scopeKey()) || localStorage.getItem(SUBTAB_KEY) || 'agentic';
      if (!root) {
        container.innerHTML = `<div class="rpws"><div class="rpws-body"><div class="rpws-empty">
          <div class="rpws-ico">${ICON.artifact}</div><h3>No project open</h3>
          <p>Open a project (or focus a terminal in one) to use its Workspace.</p></div></div></div>`;
        return;
      }
      container.innerHTML = `<div class="rpws">
        <div class="rpws-head"><div class="rpws-scope">Workspace · <strong>${escapeText(basename(root))}</strong></div></div>
        <div class="rpws-nav">${SUBTABS.map(([k, label]) => `<button data-sub="${k}"${k === active ? ' class="active"' : ''}>${label}</button>`).join('')}</div>
        <div class="rpws-body">${agenticPage()}${loopsPage()}</div>
      </div>`;
      applyActive();
      if (active === 'agentic') loadAgentic();
      if (active === 'loops') loadLooms();
      const refresh = container.querySelector('[data-act="refresh"]');
      if (refresh) refresh.onclick = () => loadAgentic();
      container.querySelectorAll('.rpws-nav button').forEach((b) => {
        b.onclick = () => {
          active = b.dataset.sub;
          localStorage.setItem(SUBTAB_KEY, active);
          localStorage.setItem(scopeKey(), active);
          container.querySelectorAll('.rpws-nav button').forEach((x) => x.classList.toggle('active', x.dataset.sub === active));
          applyActive();
          if (active === 'agentic') loadAgentic();
          if (active === 'loops') loadLooms();
        };
      });
    }

    function applyActive() {
      container.querySelectorAll('.rpws-page').forEach((p) => p.classList.toggle('active', p.dataset.page === active));
    }

    // Flow A: PM ticket 3-dot → "Create Loom" → open the Looms tab with the
    // ticket as the pre-filled goal (+ origin chip). One mechanism, two entries.
    window.xnautCreateLoomFromTicket = function (ticket) {
      if (!ticket) return;
      pending = { goal: ticketToGoal(ticket), origin: { id: ticket.id } };
      active = 'loops';
      try { localStorage.setItem(SUBTAB_KEY, 'loops'); localStorage.setItem(scopeKey(), 'loops'); } catch (_) {}
      if (typeof window.xnautRightPaneShow === 'function') window.xnautRightPaneShow('workspace');
      if (container) render();
    };

    return {
      mount(el, initialRoot) { container = el; root = initialRoot; render(); },
      setRoot(newRoot) { if (newRoot !== root) { root = newRoot; render(); } },
      destroy() { container = null; },
    };
  }

  register('workspace', createWorkspaceView());
})();
