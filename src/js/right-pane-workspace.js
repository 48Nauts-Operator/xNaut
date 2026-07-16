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
    ['plan', 'Plan'],
    ['loops', 'Looms'],
    ['output', 'Output'],
    ['agentic', 'History'],
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
        case 'artifacts': out.push({ action: s.action, show: 'gitvm pull (code) + artifacts pull', cmd: 'gitvm pull . && gitvm artifacts pull' }); break;
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
    if ((weave.steps || []).some((s) => s.action === 'record')) {
      parts.push('\nFULL-CYCLE VERIFICATION + DEMO VIDEO (required, after acceptance passes):\n'
        + '1. Prove it works like a fresh user would get it: clean install from scratch (remove node_modules or equivalent, then npm ci / install), build, and start the product the normal way.\n'
        + '2. Open the running product in a VISIBLE, maximized browser window on DISPLAY=:0 \u2014 the screen at :0 is being recorded and that recording IS the demo (headed playwright with slowMo, or chromium directly). Headless testing does NOT count.\n'
        + '3. On camera, slowly exercise the NEW feature/fix end to end for 60\u201390 seconds, the way a human user would \u2014 from opening the page to the finish-line result \u2014 so anyone watching the video can see the goal being achieved.\n'
        + '4. If the full cycle exposes a problem the earlier tests missed, fix it and redo the cycle. The demo must show the WORKING product.');
    }
    parts.push('\nWhen all acceptance criteria pass, stop and report what you changed + the test results. Write /artifacts/report.md (verdict, acceptance table with evidence, what changed) and /artifacts/status.json ({"done":true|false,"last_action":"..."}).');
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
.rpwl-sessions { display:flex; gap:6px; overflow-x:auto; padding:8px 10px; border-bottom:1px solid var(--border); flex:0 0 auto; scrollbar-width:thin; }
.rpwl-sessions::-webkit-scrollbar { height:5px; }
.rpwl-sess-empty { font-size:11px; color:var(--muted-foreground); padding:3px 4px; }
.rpwl-sess { display:inline-flex; align-items:center; gap:6px; flex:0 0 auto; max-width:180px; border:1px solid var(--border); background:var(--card,#171717); border-radius:999px; padding:4px 10px 4px 8px; cursor:pointer; color:var(--muted-foreground); font-size:11px; }
.rpwl-sess:hover { border-color:color-mix(in srgb,var(--foreground) 30%,transparent); color:var(--foreground); }
.rpwl-sess.on { border-color:var(--xnaut-yellow); color:var(--foreground); background:color-mix(in srgb,var(--xnaut-yellow) 10%,var(--card,#171717)); }
.rpwl-sess .nm { white-space:nowrap; overflow:hidden; text-overflow:ellipsis; font-weight:600; }
.rpwl-sess .ago { opacity:.6; font-family:ui-monospace,Menlo,monospace; font-size:9.5px; flex:0 0 auto; }
.rpwl-sess .dot { width:7px; height:7px; border-radius:50%; flex:0 0 auto; background:var(--muted-foreground); }
.rpwl-sess.active .dot { background:var(--xnaut-yellow); box-shadow:0 0 0 0 color-mix(in srgb,var(--xnaut-yellow) 70%,transparent); animation:rpwlPulse 1.4s ease-out infinite; }
.rpwl-sess.done .dot { background:#7ec98f; }
.rpwl-sess.failed .dot { background:#e98b83; }
.rpwl-sess.cancelled .dot, .rpwl-sess.stale .dot { background:#8a8a8a; }
@keyframes rpwlPulse { 0%{box-shadow:0 0 0 0 color-mix(in srgb,var(--xnaut-yellow) 60%,transparent);} 70%{box-shadow:0 0 0 5px transparent;} 100%{box-shadow:0 0 0 0 transparent;} }
/* ── Run report card ─────────────────────────────────────────── */
.rpwl-report { flex:0 0 auto; max-height:55%; overflow-y:auto; border-bottom:1px solid var(--border); padding:12px 14px; display:flex; flex-direction:column; gap:8px; background:var(--card,#171717); }
.rpwl-out.logfolded .rpwl-log { display:none; }
.rpwl-out.logfolded .rpwl-report { max-height:none; flex:1 1 auto; }
.rpwl-foldbtn { border:1px solid var(--border); background:transparent; color:var(--muted-foreground); font-size:10px; font-family:ui-monospace,Menlo,monospace; border-radius:8px; padding:3px 9px; cursor:pointer; }
.rpwl-foldbtn:hover { color:var(--foreground); background:var(--accent,#2a2a2f); }
.rpwl-rep-row { font-size:11.5px; font-weight:600; }
.rpwl-rep-row.good { color:#7ec98f; }
.rpwl-rep-row.warn { color:var(--xnaut-yellow); }
.rpwl-rep-verdict { font-weight:700; font-size:13px; }
.rpwl-rep-verdict.good { color:#7ec98f; }
.rpwl-rep-verdict.bad { color:#e98b83; }
.rpwl-rep-md { font-size:12px; line-height:1.55; color:var(--foreground); }
.rpwl-rep-md h3, .rpwl-rep-md h4, .rpwl-rep-md h5 { margin:8px 0 3px; font-size:12.5px; color:var(--foreground); }
.rpwl-rep-md h3 { font-size:13.5px; }
.rpwl-rep-md p { margin:2px 0; }
.rpwl-rep-md .li { margin:1px 0 1px 6px; }
.rpwl-rep-md .sp { height:6px; }
.rpwl-rep-md code { font-family:ui-monospace,Menlo,monospace; font-size:10.5px; background:var(--secondary,#262626); border-radius:4px; padding:1px 4px; }
.rpwl-rep-md pre.code, .rpwl-rep-md pre.tbl { margin:4px 0; padding:8px 10px; background:var(--bg-deep,#0a0b0e); border:1px solid var(--border); border-radius:8px; font-family:ui-monospace,Menlo,monospace; font-size:10px; line-height:1.5; overflow-x:auto; }
.rpwl-rep-sec { margin-top:4px; font-size:9.5px; letter-spacing:.08em; text-transform:uppercase; color:var(--muted-foreground); font-weight:650; }
.rpwl-rep-media { display:flex; gap:8px; flex-wrap:wrap; }
.rpwl-rep-media img.shot { width:120px; height:76px; object-fit:cover; border:1px solid var(--border); border-radius:8px; cursor:zoom-in; }
.rpwl-rep-media img.shot:hover { border-color:var(--xnaut-yellow); }
.rpwl-rep-media button.vid { display:inline-flex; align-items:center; gap:7px; border:1px solid var(--border); background:var(--secondary,#262626); color:var(--foreground); border-radius:8px; padding:8px 12px; font:inherit; font-size:11px; cursor:pointer; }
.rpwl-rep-media button.vid span { color:var(--xnaut-yellow); }
.rpwl-rep-media button.vid:hover { border-color:var(--xnaut-yellow); }
.rpwl-media-ov { position:fixed; inset:0; z-index:9999; background:rgba(0,0,0,.75); display:flex; align-items:center; justify-content:center; }
.rpwl-media-ov img, .rpwl-media-ov video { max-width:86vw; max-height:86vh; border-radius:10px; box-shadow:0 18px 60px rgba(0,0,0,.6); }
.rpwl-out-h { display:flex; align-items:center; gap:8px; padding:8px 12px; border-bottom:1px solid var(--border); flex:0 0 auto; }
.rpwl-out-h .k { font-size:10px; letter-spacing:.09em; text-transform:uppercase; color:var(--muted-foreground); font-weight:650; }
.rpwl-out-h .rpwl-sb { margin-left:auto; }
.rpwl-stopbtn { border:1px solid rgba(220,110,100,.55); background:transparent; color:#e98b83; font-weight:650; font-size:11px; border-radius:8px; padding:3px 10px; cursor:pointer; }
.rpwl-stopbtn:hover { background:rgba(220,110,100,.12); }
.rpwl-out-h .state { font-size:9.5px; letter-spacing:.06em; text-transform:uppercase; border:1px solid var(--border); border-radius:999px; padding:2px 8px; font-family:ui-monospace,Menlo,monospace; color:var(--muted-foreground); }
.rpwl-out-h .state.run { color:var(--xnaut-yellow); border-color:color-mix(in srgb,var(--xnaut-yellow) 45%,transparent); }
.rpwl-sb { margin-left:auto; margin-right:10px; font-family:ui-monospace,Menlo,monospace; font-size:10px; color:var(--muted-foreground); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
.rpwl-sb:empty { display:none; }
.rpwl-sb a { color:var(--xnaut-yellow); cursor:pointer; text-decoration:none; }
.rpwl-sb a:hover { text-decoration:underline; }
.rpwl-fs-btn { flex:0 0 auto; margin-left:8px; width:22px; height:22px; border:0; border-radius:5px; background:transparent; color:var(--muted-foreground); cursor:pointer; font-size:13px; line-height:1; }
.rpwl-fs-btn:hover { background:var(--accent,#2a2a2f); color:var(--foreground); }
/* active mode: on Run, only the OUTPUT terminal shows (full pane) */
.rpws-page.rpwl-fs > :not(.rpwl-out) { display:none; }
.rpws-page.rpwl-fs .rpwl-out { flex:1 1 auto; }
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

/* ── Plan tab (Cloud Agent chat) ─────────────────────────────── */
.rpwp-ctx { display:flex; gap:6px; flex-wrap:wrap; padding:10px 14px 2px; flex:0 0 auto; }
.rpwp-ctx:empty { display:none; }
.rpwp-chip { display:inline-flex; align-items:center; gap:5px; font-family:ui-monospace,Menlo,monospace; font-size:10px; color:#c8c8c8; border:1px solid var(--border); border-radius:999px; padding:3px 9px; }
.rpwp-chip.yellow { color:var(--xnaut-yellow); border-color:color-mix(in srgb,var(--xnaut-yellow) 45%,transparent); }
.rpwp-chip b { cursor:pointer; opacity:.6; }
.rpwp-thread { flex:1 1 auto; min-height:0; overflow-y:auto; display:flex; flex-direction:column; gap:12px; padding:12px 14px; }
.rpwp-a { display:flex; gap:9px; }
.rpwp-av { width:24px; height:24px; flex-shrink:0; border-radius:7px; background:color-mix(in srgb,var(--accent,#404040) 55%,transparent); color:var(--foreground); display:flex; align-items:center; justify-content:center; }
.rpwp-av svg { width:13px; height:13px; }
.rpwp-bub-a { background:var(--card,#171717); border:1px solid var(--border); border-radius:11px; border-top-left-radius:3px; padding:9px 11px; font-size:12.5px; line-height:1.5; color:var(--foreground); }
.rpwp-u { display:flex; justify-content:flex-end; }
.rpwp-bub-u { max-width:82%; background:var(--secondary,#262626); border-radius:11px; border-top-right-radius:3px; padding:9px 11px; font-size:12.5px; line-height:1.5; color:var(--foreground); white-space:pre-wrap; }
.rpwp-composer { display:flex; align-items:flex-end; gap:8px; padding:10px 12px 6px; border-top:1px solid var(--border); flex:0 0 auto; }
.rpwp-attach { flex:0 0 auto; width:32px; height:32px; border:1px solid var(--border); border-radius:9px; background:transparent; color:var(--muted-foreground); cursor:pointer; display:flex; align-items:center; justify-content:center; }
.rpwp-attach:hover { color:var(--foreground); background:var(--accent,#2a2a2f); }
.rpwp-attach svg { width:15px; height:15px; }
.rpwp-input { flex:1 1 auto; min-width:0; resize:vertical; background:var(--secondary,#262626); border:1px solid var(--border); border-radius:11px; color:var(--foreground); font:inherit; font-size:12.5px; line-height:1.45; padding:11px 12px; min-height:88px; max-height:280px; outline:none; }
.rpwp-input:focus { border-color:var(--xnaut-yellow); }
.rpwp-actions { display:flex; flex-direction:column; gap:6px; align-items:stretch; flex:0 0 auto; }
.rpwp-model { background:var(--secondary,#262626); border:1px solid var(--border); border-radius:8px; color:var(--muted-foreground); font:inherit; font-size:10.5px; padding:4px 6px; outline:none; cursor:pointer; }
.rpwp-model:focus { border-color:var(--xnaut-yellow); }
.rpwp-run { flex:0 0 auto; display:flex; align-items:center; justify-content:center; gap:6px; height:40px; padding:0 17px; border:0; border-radius:11px; background:var(--xnaut-yellow); color:var(--primary-foreground,#171717); font:inherit; font-weight:700; font-size:13px; cursor:pointer; }
.rpwp-hint { padding:2px 14px 11px; font-family:ui-monospace,Menlo,monospace; font-size:9.5px; color:var(--muted-foreground); flex:0 0 auto; }
.rpwp-run.confirm { background:var(--xnaut-yellow); box-shadow:0 0 0 2px color-mix(in srgb,var(--xnaut-yellow) 35%,transparent); }
/* Plan preflight card */
.rpwp-a:has(.rpwp-plan) .rpwp-bub-a { width:100%; }
.rpwp-plan { display:flex; flex-direction:column; gap:5px; min-width:0; }
.rpwp-plan-h { font-weight:700; font-size:12.5px; color:var(--foreground); margin-bottom:2px; }
.rpwp-chk { font-size:11.5px; line-height:1.4; }
.rpwp-chk.good { color:#7ec98f; }
.rpwp-chk.bad { color:#e98b83; }
.rpwp-chk.dim { color:var(--muted-foreground); }
.rpwp-sec { margin-top:7px; font-size:9.5px; letter-spacing:.08em; text-transform:uppercase; color:var(--muted-foreground); font-weight:650; }
.rpwp-sec .red { color:#e98b83; text-transform:none; letter-spacing:0; font-weight:600; margin-left:4px; }
.rpwp-step { display:flex; gap:8px; font-family:ui-monospace,Menlo,monospace; font-size:11px; color:var(--foreground); }
.rpwp-step b { color:var(--muted-foreground); font-weight:600; min-width:12px; }
.rpwp-red { margin:3px 0 2px; padding:8px 10px; border:1px solid color-mix(in srgb,#e98b83 45%,transparent); border-left:2px solid #e98b83; border-radius:8px; background:color-mix(in srgb,#e98b83 8%,var(--card,#171717)); color:var(--foreground); font-family:ui-monospace,Menlo,monospace; font-size:10.5px; line-height:1.5; white-space:pre-wrap; max-height:220px; overflow-y:auto; }
.rpwp-ready { margin-top:5px; font-size:11.5px; font-weight:600; color:var(--xnaut-yellow); }
.rpwp-ready.bad { color:#e98b83; }
`;
    document.head.appendChild(s);
  }

  function createWorkspaceView() {
    let container = null;
    let root = null;
    let active = localStorage.getItem(SUBTAB_KEY) || 'plan';
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
    let planMessages = [];   // Plan chat thread [{role:'agent'|'user', text}]
    let sandboxId = '';      // parsed from the run log (gitvm warm-up output)
    let desktopUrl = '';     // noVNC desktop URL to watch the sandbox live
    let runStart = 0;        // ms when the current run started (for the live timer)
    let runTimer = null;     // interval id for the running-elapsed ticker

    function scopeKey() { return root ? `${SUBTAB_KEY}:${root}` : SUBTAB_KEY; }

    const REFRESH_ICON = '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M13.5 8a5.5 5.5 0 1 1-1.6-3.9"/><path d="M13.5 1.5v3h-3"/></svg>';
    const TICKET_ICON = '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4"><path d="M2 5.5A1.5 1.5 0 0 1 3.5 4h9A1.5 1.5 0 0 1 14 5.5v1a1.5 1.5 0 0 0 0 3v1a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 2 10.5v-1a1.5 1.5 0 0 0 0-3z"/></svg>';
    const AGENT_ICON = '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="3" y="4" width="10" height="8" rx="2"/><circle cx="6" cy="8" r="1" fill="currentColor" stroke="none"/><circle cx="10" cy="8" r="1" fill="currentColor" stroke="none"/><path d="M8 2v2"/></svg>';
    // Models the Cloud Agent can build with (claude CLI --model). value → label.
    const MODELS = [['claude-opus-4-8', 'Opus 4.8'], ['claude-sonnet-5', 'Sonnet 5'], ['claude-haiku-4-5-20251001', 'Haiku 4.5']];
    let modelSel = localStorage.getItem('xnaut-loom-model') || 'claude-opus-4-8';
    let activeRunId = null;  // id of the live run (to mark done/failed in runs.jsonl)
    let runSessions = [];    // recent runs for the Output sessions bar
    let viewingRun = null;   // id of the run whose log is shown (null = the live one)
    let planReady = false;   // Plan chat: a preflight brief is shown, next Run confirms
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

    // Plan — chat the task with the Cloud Agent, then Run. (v1: a thread + a
    // composer whose Run dispatches the selected loom with the typed goal.)
    function planPage() {
      return `<div class="rpws-page" data-page="plan">
        <div class="rpwp-ctx" data-plan-ctx></div>
        <div class="rpwp-thread" data-plan-thread></div>
        <div class="rpwp-composer">
          <button class="rpwp-attach" data-act="plan-ticket" title="Load a ticket">${TICKET_ICON}</button>
          <textarea class="rpwp-input" data-plan-input rows="1" placeholder="Describe the task, or load a ticket…"></textarea>
          <div class="rpwp-actions">
            <select class="rpwp-model" data-plan-model title="Model to build with">${MODELS.map(([v, l]) => `<option value="${v}"${v === modelSel ? ' selected' : ''}>${l}</option>`).join('')}</select>
            <button class="rpwp-run" data-act="plan-run"><svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor"><path d="M4 3l9 5-9 5z"/></svg>Run</button>
          </div>
        </div>
        <div class="rpwp-hint">↳ Verify runs automatically before dispatch · the Cloud Agent works async → Output</div>
      </div>`;
    }

    // Looms — the workflow library: pick / edit the recipe the agent runs.
    function loopsPage() {
      return `<div class="rpws-page" data-page="loops">
        <div class="rpwl-label"><span class="k">Looms</span><button class="rpws-linkbtn" data-act="loom-new" title="Create a new loom">+ New</button></div>
        <div class="rpwl-loomlist" data-loom-list></div>
        <div class="rpwl-sel" data-loom-sel></div>
        <div class="rpwl-runbar" data-loom-runbar></div>
      </div>`;
    }

    // Output — the live run log + (when done) the structured report.
    function outputPage() {
      return `<div class="rpws-page" data-page="output">
        <div class="rpwl-out">
          <div class="rpwl-sessions" data-loom-sessions></div>
          <div class="rpwl-out-h"><span class="k">Output</span><span class="rpwl-sb" data-loom-sb></span><span class="state" data-loom-state>idle</span><button class="rpwl-stopbtn" data-loom-stop hidden>■ Stop</button><button class="rpwl-foldbtn" data-loom-fold title="Collapse/expand the log">▾ log</button></div>
          <div class="rpwl-report" data-loom-report hidden></div>
          <pre class="rpwl-log" data-loom-log></pre>
        </div>
      </div>`;
    }

    function loomsEls() {
      if (!container) return null;
      const page = container.querySelector('.rpws-page[data-page="loops"]');
      if (!page) return null;
      return { page,
        list: container.querySelector('[data-loom-list]'), sel: container.querySelector('[data-loom-sel]'),
        runbar: container.querySelector('[data-loom-runbar]'),
        state: container.querySelector('[data-loom-state]'), log: container.querySelector('[data-loom-log]') };
    }
    function setState(txt, cls) {
      const e = loomsEls(); if (e && e.state) { e.state.textContent = txt; e.state.className = 'state' + (cls ? ' ' + cls : ''); }
      const sb = container && container.querySelector('[data-loom-stop]'); if (sb) sb.hidden = !running;
    }
    function outReset() { const e = loomsEls(); if (e && e.log) e.log.innerHTML = ''; }
    function outLine(text, cls) {
      const e = loomsEls(); if (!e || !e.log) return;
      const div = document.createElement('div'); if (cls) div.className = cls; div.textContent = text;
      e.log.appendChild(div); e.log.scrollTop = e.log.scrollHeight;
    }
    function metaOf() { return looms.find((l) => l.path === loomSelected); }
    function renderSandbox() {
      if (!container) return;
      const slot = container.querySelector('[data-loom-sb]'); if (!slot) return;
      if (!sandboxId && !desktopUrl) { slot.innerHTML = ''; return; }
      slot.innerHTML = `${escapeText(sandboxId || 'sandbox')}${desktopUrl ? ' · <a data-open-desktop title="Watch the sandbox live">open desktop ↗</a>' : ''}`;
      const a = slot.querySelector('[data-open-desktop]');
      if (a) a.onclick = () => { if (!openInternalBrowser(desktopUrl)) openTarget(desktopUrl); };
    }
    // Switch the active sub-tab programmatically (Run → Output, etc.).
    function loadTab(key) {
      if (key === 'agentic') loadAgentic();
      else if (key === 'loops') loadLooms();
      else if (key === 'plan') loadPlan();
      else if (key === 'output') loadSessions();
    }
    function switchTab(key) {
      active = key;
      try { localStorage.setItem(SUBTAB_KEY, key); localStorage.setItem(scopeKey(), key); } catch (_) {}
      if (!container) return;
      container.querySelectorAll('.rpws-nav button').forEach((x) => x.classList.toggle('active', x.dataset.sub === key));
      applyActive();
      loadTab(key);
    }
    // Warm-up timer: a live elapsed clock in the state chip while running, so
    // it's obviously alive even when provisioning is quiet.
    function mmss(ms) { const s = Math.max(0, Math.floor(ms / 1000)); return Math.floor(s / 60) + ':' + String(s % 60).padStart(2, '0'); }
    function startTimer() {
      stopTimer(); if (!runStart) runStart = Date.now();
      const tick = () => { const e = loomsEls(); if (e && e.state && running) e.state.textContent = 'running · ' + mmss(Date.now() - runStart); };
      tick(); runTimer = setInterval(tick, 1000);
    }
    function stopTimer() { if (runTimer) { clearInterval(runTimer); runTimer = null; } runStart = 0; }

    // ---- Plan tab: chat with the Cloud Agent, then Run ----
    function planEls() {
      const page = container && container.querySelector('.rpws-page[data-page="plan"]');
      if (!page) return null;
      return { page, ctx: page.querySelector('[data-plan-ctx]'), thread: page.querySelector('[data-plan-thread]'), input: page.querySelector('[data-plan-input]') };
    }
    function renderPlanCtx() {
      const p = planEls(); if (!p || !p.ctx) return;
      const chips = [];
      if (origin) chips.push(`<span class="rpwp-chip yellow">from ${escapeText(origin.id)}<b data-plan-clr title="clear">✕</b></span>`);
      const m = metaOf(); if (m) chips.push(`<span class="rpwp-chip">loom · ${escapeText(m.name)}</span>`);
      p.ctx.innerHTML = chips.join('');
      const clr = p.ctx.querySelector('[data-plan-clr]'); if (clr) clr.onclick = () => { origin = null; renderPlanCtx(); };
    }
    // The preflight brief the Plan chat shows before running: what loom, what
    // ticket, the steps, the injected "red prompt", and whether it's ready.
    function planBrief() {
      const m = metaOf();
      const provider = (loomWeave && loomWeave.runtime && loomWeave.runtime.provider) || 'gitvm';
      const v = loomWeave ? verifyWeave(loomWeave) : { ok: false, issues: ['no loom selected'] };
      const steps = loomWeave ? composeCommands(loomWeave, provider).map((c) => c.show) : [];
      const ml = (MODELS.find((x) => x[0] === modelSel) || [null, modelSel])[1];
      return {
        loom: m ? m.name : '(none)', loomOk: !!m, provider: provider,
        ticket: origin ? origin.id : null,
        project: root ? basename(root) : '(no project open)', projectOk: !!root, model: ml,
        steps: steps, redPrompt: enrichGoal(loomWeave || {}, goalText),
        verifyOk: v.ok, verifyIssues: v.issues || [],
      };
    }
    function planCardHtml(pl) {
      const chk = (t, ok) => `<div class="rpwp-chk ${ok ? 'good' : 'bad'}">${ok ? '✓' : '✗'} ${escapeText(t)}</div>`;
      const steps = (pl.steps || []).map((s, i) => `<div class="rpwp-step"><b>${i + 1}</b>${escapeText(s)}</div>`).join('');
      const ready = pl.loomOk && pl.verifyOk && pl.projectOk;
      return `<div class="rpwp-plan">
        <div class="rpwp-plan-h">Plan · ${escapeText(pl.loom)}</div>
        ${chk(pl.loomOk ? 'Loom found · ' + pl.provider + ' · ' + (pl.steps || []).length + ' steps' : 'No loom selected — pick one in Looms', pl.loomOk)}
        ${pl.ticket ? chk('Linked to ticket ' + pl.ticket, true) : '<div class="rpwp-chk dim">○ no ticket linked</div>'}
        ${chk(pl.projectOk ? 'Project ' + pl.project + ' · model ' + pl.model : 'No project open', pl.projectOk)}
        <div class="rpwp-sec">What it will do</div>${steps}
        <div class="rpwp-sec">Brief injected into the agent <span class="red">● red prompt</span></div>
        <pre class="rpwp-red">${escapeText(pl.redPrompt)}</pre>
        ${chk(pl.verifyOk ? 'Preflight — schema valid, system ready' : 'Preflight failed: ' + pl.verifyIssues.join('; '), pl.verifyOk)}
        <div class="rpwp-ready ${ready ? '' : 'bad'}">${ready ? '→ Ready. Hit Run to launch it in a sandbox — or edit your request above.' : 'Not ready — resolve the ✗ above first.'}</div>
      </div>`;
    }
    function syncRunLabel() {
      const p = planEls(); if (!p) return;
      const b = p.page.querySelector('[data-act="plan-run"]');
      if (!b) return;
      b.innerHTML = planReady
        ? '<svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor"><path d="M4 3l9 5-9 5z"/></svg>Run'
        : '<svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor"><path d="M8 4l4 4-4 4M4 4l4 4-4 4"/></svg>Plan';
      b.classList.toggle('confirm', planReady);
    }
    function renderPlanThread() {
      const p = planEls(); if (!p || !p.thread) return;
      const msgs = planMessages.length ? planMessages : [{ role: 'agent', text: "Tell me what to build, or load a ticket. I'll lay out the plan — which loom, the steps, the exact brief the agent gets — and run it in a sandbox once you confirm." }];
      p.thread.innerHTML = msgs.map((mm) => {
        if (mm.role === 'user') return `<div class="rpwp-u"><div class="rpwp-bub-u">${escapeText(mm.text)}</div></div>`;
        const body = mm.kind === 'plan' ? planCardHtml(mm.plan) : escapeText(mm.text);
        return `<div class="rpwp-a"><div class="rpwp-av">${AGENT_ICON}</div><div class="rpwp-bub-a">${body}</div></div>`;
      }).join('');
      p.thread.scrollTop = p.thread.scrollHeight;
      syncRunLabel();
    }
    async function loadPlan() {
      const p = planEls(); if (!p) return;
      try { if (!looms.length) { await invoke('looms_seed_defaults'); looms = (await invoke('looms_list')) || []; } } catch (_) {}
      if (looms.length && !looms.find((l) => l.path === loomSelected)) loomSelected = looms[0].path;
      if (pending) { planMessages.push({ role: 'user', text: pending.goal }); origin = pending.origin || null; goalText = pending.goal || ''; pending = null; }
      renderPlanCtx(); renderPlanThread();
      const runb = p.page.querySelector('[data-act="plan-run"]');
      const tkt = p.page.querySelector('[data-act="plan-ticket"]');
      const msel = p.page.querySelector('[data-plan-model]');
      if (tkt) tkt.onclick = () => loadTicketInto();
      if (runb) runb.onclick = () => dispatchPlan();
      if (msel) msel.onchange = () => { modelSel = msel.value; try { localStorage.setItem('xnaut-loom-model', modelSel); } catch (_) {} if (planReady) { planReady = false; syncRunLabel(); } };
      // Editing the request invalidates a shown plan — back to "Plan" phase.
      if (p.input) p.input.oninput = () => { if (planReady) { planReady = false; syncRunLabel(); } };
      if (p.input) p.input.onkeydown = (ev) => { if (ev.key === 'Enter' && !ev.shiftKey) { ev.preventDefault(); dispatchPlan(); } };
      syncRunLabel();
    }
    async function dispatchPlan() {
      const p = planEls(); if (!p) return;
      // Phase 2 — a plan is on screen and valid → this press launches it.
      if (planReady) { planReady = false; syncRunLabel(); runLoom(); return; }
      // Phase 1 — take the request, resolve the loom, and lay out the plan.
      const text = (p.input.value || '').trim() || goalText;
      if (!text) { p.input.focus(); return; }
      planMessages.push({ role: 'user', text });
      p.input.value = ''; goalText = text;
      if (!loomWeave || (metaOf() && metaOf().path !== loomSelected)) { await selectLoom(loomSelected, false); goalText = text; }
      const brief = planBrief();
      planMessages.push({ role: 'agent', kind: 'plan', plan: brief });
      planReady = brief.loomOk && brief.verifyOk && brief.projectOk;
      renderPlanThread();
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
      e.sel.innerHTML = `
        <div class="name">${escapeText(meta ? meta.name : '')}</div>
        <div class="desc">${escapeText((meta && meta.description) || '')}</div>
        <div class="rpwl-chips">${provider ? `<span class="rpwl-chip prov">${escapeText(provider)}</span>` : ''}<span class="rpwl-chip">${steps} steps</span><span class="rpwl-chip">acceptance · ${acc}</span></div>
        <div class="rpwl-tools"><button class="rpws-linkbtn" data-act="loom-edit">Edit</button></div>`;
      const ed = e.sel.querySelector('[data-act="loom-edit"]'); if (ed) ed.onclick = () => openLoomEditor(loomWeave);
    }

    // Looms is pick/edit-only now — the Goal + Run live in the Plan chat. The one
    // action here hands the selected loom off to Plan.
    function renderRunbar() {
      const e = loomsEls(); if (!e) return;
      e.runbar.innerHTML = `<button class="rpwl-b run" data-act="loom-toplan">Use in Plan →</button>`;
      e.runbar.querySelector('[data-act="loom-toplan"]').onclick = () => switchTab('plan');
    }

    async function runLoom() {
      if (running || !loomWeave) return;
      const goal = (goalText || '').trim();
      const v = verifyWeave(loomWeave);
      if (!v.ok) {
        outReset(); switchTab('output'); setState('failed');
        outLine('✗ verify — ' + v.issues.length + ' issue' + (v.issues.length === 1 ? '' : 's') + ':', 'warn');
        v.issues.forEach((i) => outLine('  · ' + i, 'warn'));
        return;
      }
      if (!goal && (loomWeave.steps || []).some((s) => s.action === 'agent')) {
        switchTab('plan'); const pe = planEls(); if (pe && pe.input) pe.input.focus();
        return;
      }
      const name = metaOf() ? metaOf().name : 'loom';
      const script = composeCommands(loomWeave, v.provider).map((c) => 'echo "» ' + c.action + '"; ' + c.cmd).join('\n');
      outReset(); hideReport(); runCloseNote = ''; switchTab('output'); setState('running', 'run'); startTimer();
      sandboxId = ''; desktopUrl = ''; renderSandbox();
      outLine('$ loom run ' + name + '  ·  ' + v.provider, 'dim');
      const runId = 'run-' + Date.now();
      const agentic = (loomWeave.steps || []).some((s) => s.action === 'agent');
      const effGoal = agentic ? enrichGoal(loomWeave, goal) : goal;
      let h;
      try { h = await invoke('loom_run', { runId: runId, script: script, goal: effGoal, cwd: root, model: modelSel }); }
      catch (err) { setState('failed'); outLine('could not start: ' + ((err && err.message) || err), 'warn'); return; }
      running = true; runHandle = h; activeRunId = runId; renderRunbar();
      saveActiveRun({ runId: runId, log: h.log, pid: h.pid, name: name, root: root, ts: runStart });
      try { await invoke('loom_run_record', { runId: runId, weave: name, goal: goal || '(no goal)', provider: v.provider, pid: h.pid }); } catch (_) {}
      loadSessions();
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
      running = true; runHandle = { pid: a.pid, log: a.log }; activeRunId = a.runId || null;
      outReset(); switchTab('output'); setState('running', 'run');
      runStart = a.ts || Date.now(); startTimer();
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
          const live = !viewingRun || viewingRun === activeRunId; // else: user is reading history
          chunk.split('\n').forEach((ln) => {
            if (ln === '' || ln.indexOf('__LOOM_DONE__') === 0) return;
            const sbm = ln.match(/sandbox:\s*(sb-\S+)/i); if (sbm) { sandboxId = sbm[1]; renderSandbox(); }
            const dm = ln.match(/(https?:\/\/\S*nautbox\.dev\/vnc\.html\S*)/i); if (dm) { desktopUrl = dm[1]; renderSandbox(); }
            if (!live) return;
            let cls = '';
            if (ln.charAt(0) === '»') cls = 'step';
            else if (/error|fail|fatal|panic|✗/i.test(ln)) cls = 'warn';
            else if (/^✓|success|passed|compiled|ready|\bok\b/i.test(ln)) cls = 'ok';
            outLine(ln, cls);
          });
        }
        if (txt.indexOf('__LOOM_DONE__') >= 0) {
          const m = txt.match(/__LOOM_DONE__ (\d+)/); const code = m ? m[1] : '?';
          const sinceMs = runStart; // capture before stopTimer zeroes it
          running = false; runHandle = null; clearActiveRun(); stopTimer();
          if (activeRunId) { try { await invoke('loom_run_mark', { id: activeRunId, status: code === '0' ? 'done' : 'failed' }); } catch (_) {} }
          activeRunId = null;
          outLine(''); outLine(code === '0' ? '✓ run finished · exit 0' : '✗ run exited · code ' + code, code === '0' ? 'ok' : 'warn');
          setState(code === '0' ? 'done' : 'failed'); renderRunbar(); loadSessions();
          runCloseNote = await closeLoopTicket(code === '0');
          renderReport(sinceMs); return;
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
      if (activeRunId) { try { await invoke('loom_run_mark', { id: activeRunId, status: 'cancelled' }); } catch (_) {} }
      running = false; runHandle = null; activeRunId = null; clearActiveRun(); stopTimer();
      outLine('■ stopped', 'warn'); setState('stopped'); renderRunbar(); loadSessions();
    }

    // ---- Sessions bar: every run + its live status (active / done / failed) ----
    function fmtAgo(ms) {
      const s = Math.floor((Date.now() - ms) / 1000);
      if (s < 60) return s + 's'; const m = Math.floor(s / 60);
      if (m < 60) return m + 'm'; const h = Math.floor(m / 60);
      if (h < 24) return h + 'h'; return Math.floor(h / 24) + 'd';
    }
    async function loadSessions() {
      try { runSessions = (await invoke('loom_runs_list', { limit: 24 })) || []; } catch (_) { runSessions = []; }
      await Promise.all(runSessions.map(async (r) => {
        if (r.status === 'started') {
          let alive = false; if (r.pid) { try { alive = await invoke('loom_run_alive', { pid: r.pid }); } catch (_) {} }
          r._st = alive ? 'active' : 'stale';
        } else { r._st = r.status; } // done | failed | cancelled
      }));
      renderSessions();
    }
    function renderSessions() {
      const host = container && container.querySelector('[data-loom-sessions]'); if (!host) return;
      if (!runSessions.length) { host.innerHTML = '<span class="rpwl-sess-empty">No runs yet — start one from Plan.</span>'; return; }
      const cur = viewingRun || activeRunId;
      host.innerHTML = runSessions.map((r) => {
        const st = r._st || r.status;
        return `<button class="rpwl-sess ${st}${r.id === cur ? ' on' : ''}" data-run="${escapeText(r.id)}" title="${escapeText(r.weave)} · ${st}"><span class="dot"></span><span class="nm">${escapeText(r.weave)}</span><span class="ago">${fmtAgo(r.started_ms)}</span></button>`;
      }).join('');
      host.querySelectorAll('[data-run]').forEach((b) => { b.onclick = () => viewSession(b.dataset.run); });
    }
    function dumpLog(txt) {
      outReset();
      txt.split('\n').forEach((ln) => {
        if (ln === '' || ln.indexOf('__LOOM_DONE__') === 0) return;
        let cls = '';
        if (ln.charAt(0) === '»') cls = 'step';
        else if (/error|fail|fatal|panic|✗/i.test(ln)) cls = 'warn';
        else if (/^✓|success|passed|compiled|ready|\bok\b/i.test(ln)) cls = 'ok';
        outLine(ln, cls);
      });
    }
    async function viewSession(id) {
      const r = runSessions.find((x) => x.id === id); if (!r) return;
      if (running && id === activeRunId) { viewingRun = null; } // back to the live tail
      else { viewingRun = id; }
      renderSessions();
      let txt = ''; try { txt = (await invoke('read_file', { path: r.log })) || ''; } catch (_) {}
      dumpLog(txt);
      const st = r._st || r.status;
      setState(st === 'active' ? 'running' : st, st === 'active' ? 'run' : '');
      if (id !== activeRunId) runCloseNote = ''; // ticket note belongs to the live run only
      if (st === 'done' || st === 'failed') renderReport(r.started_ms); else hideReport();
    }

    // ---- Report card: what the run achieved (agent-written report + media) ----
    function hideReport() { const h = container && container.querySelector('[data-loom-report]'); if (h) { h.hidden = true; h.innerHTML = ''; } setLogFolded(false); }
    // Collapse the raw log so the report owns the tab (▾ log toggles it back).
    function setLogFolded(folded) {
      const out = container && container.querySelector('.rpwl-out'); if (!out) return;
      out.classList.toggle('logfolded', !!folded);
      const b = out.querySelector('[data-loom-fold]'); if (b) b.textContent = folded ? '▸ log' : '▾ log';
    }
    // Close the loop on the linked PM ticket: append the run result and move it
    // to review when the run went green. pm_ticket_update does the event+commit.
    let runCloseNote = '';
    async function closeLoopTicket(ok) {
      if (!origin) return '';
      let tickets = [];
      try { tickets = (await invoke('pm_ticket_list', { project: null })) || []; } catch (_) { return ''; }
      const t = tickets.find((x) => x.id === origin.id); if (!t) return '';
      const stamp = new Date().toISOString().slice(0, 16).replace('T', ' ');
      const note = '\n\n> NautLoom run ' + stamp + ' — ' + (ok ? '✓ acceptance green' : '✗ failed') + ' (' + (metaOf() ? metaOf().name : 'loom') + '). Code pulled to the local working tree — review & commit.';
      const req = { id: t.id, expected_revision: t.revision, body: (t.body || '') + note };
      if (ok && ['inbox', 'ready', 'in_progress'].indexOf(t.status) >= 0) req.status = 'review';
      try { await invoke('pm_ticket_update', { request: req }); return t.id + (req.status ? ' → review' : ' — result appended'); }
      catch (_) { return ''; }
    }
    // Minimal markdown → HTML for the agent's report.md (headings, bold, code,
    // bullets, tables kept monospace). Escaped first — never trust file content.
    function mdLite(md) {
      const esc = escapeText(md);
      const lines = esc.split('\n');
      const out = []; let inCode = false, inTable = false;
      const closeTable = () => { if (inTable) { out.push('</pre>'); inTable = false; } };
      for (const ln of lines) {
        if (ln.startsWith('```')) { closeTable(); inCode = !inCode; out.push(inCode ? '<pre class="code">' : '</pre>'); continue; }
        if (inCode) { out.push(ln); continue; }
        if (ln.startsWith('|')) { if (!inTable) { out.push('<pre class="tbl">'); inTable = true; } out.push(ln); continue; }
        closeTable();
        let h = ln.replace(/\*\*([^*]+)\*\*/g, '<b>$1</b>').replace(/`([^`]+)`/g, '<code>$1</code>');
        if (h.startsWith('### ')) out.push('<h5>' + h.slice(4) + '</h5>');
        else if (h.startsWith('## ')) out.push('<h4>' + h.slice(3) + '</h4>');
        else if (h.startsWith('# ')) out.push('<h3>' + h.slice(2) + '</h3>');
        else if (h.startsWith('- ')) out.push('<div class="li">• ' + h.slice(2) + '</div>');
        else if (h.trim() === '') out.push('<div class="sp"></div>');
        else out.push('<p>' + h + '</p>');
      }
      closeTable(); if (inCode) out.push('</pre>');
      return out.join('\n');
    }
    function assetUrl(p) {
      try { return window.__TAURI__.core.convertFileSrc(p); } catch (_) { return ''; }
    }
    // Click a screenshot/video → centered overlay viewer.
    function openMedia(m) {
      const url = assetUrl(m.path); if (!url) return;
      const ov = document.createElement('div'); ov.className = 'rpwl-media-ov';
      ov.innerHTML = m.kind === 'video'
        ? `<video src="${url}" controls autoplay></video>`
        : `<img src="${url}" alt="${escapeText(m.name)}">`;
      document.body.appendChild(ov);
      ov.onclick = (e) => { if (e.target === ov) ov.remove(); };
      const onKey = (e) => { if (e.key === 'Escape') { ov.remove(); document.removeEventListener('keydown', onKey); } };
      document.addEventListener('keydown', onKey);
    }
    async function renderReport(sinceMs) {
      const host = container && container.querySelector('[data-loom-report]'); if (!host) return;
      let rep = null;
      try { rep = await invoke('loom_report', { cwd: root, sinceMs: sinceMs || 0 }); } catch (_) {}
      if (!rep || (!rep.report_md && !rep.media.length)) { hideReport(); return; }
      let verdict = null;
      try { verdict = JSON.parse(rep.status_json || 'null'); } catch (_) {}
      const ok = verdict ? !!verdict.done : /✅|COMPLETE|PASS/i.test(rep.report_md.slice(0, 400));
      const imgs = rep.media.filter((m) => m.kind === 'image');
      const vids = rep.media.filter((m) => m.kind === 'video');
      // Code / PR state: is the agent's work still uncommitted? pushed?
      let codeRow = '';
      try {
        const files = (await invoke('git_uncommitted_files', { repo: root })) || [];
        let ab = null; try { ab = await invoke('git_ahead_behind', { repo: root }); } catch (_) {}
        const pushed = ab && ab.upstream && !ab.ahead;
        codeRow = files.length
          ? `<div class="rpwl-rep-row warn">⚠ ${files.length} file${files.length === 1 ? '' : 's'} changed locally — review &amp; commit</div>`
          : `<div class="rpwl-rep-row ${pushed ? 'good' : 'warn'}">${pushed ? '✓ committed &amp; pushed' : '⚠ committed, not pushed' + (ab && ab.ahead ? ' (' + ab.ahead + ' ahead)' : '')}</div>`;
      } catch (_) {}
      host.innerHTML = `
        <div class="rpwl-rep-verdict ${ok ? 'good' : 'bad'}">${ok ? '✓ Acceptance passed' : '✗ Did not finish green'}${verdict && verdict.last_action ? ' · ' + escapeText(verdict.last_action) : ''}</div>
        ${runCloseNote ? `<div class="rpwl-rep-row good">✓ ticket ${escapeText(runCloseNote)}</div>` : ''}
        ${codeRow}
        ${rep.report_md ? `<div class="rpwl-rep-md">${mdLite(rep.report_md)}</div>` : ''}
        ${vids.length ? `<div class="rpwl-rep-sec">Demo</div><div class="rpwl-rep-media">${vids.map((m, i) => `<button class="vid" data-med="v${i}" title="${escapeText(m.name)}"><span>▶</span>${escapeText(m.name)}</button>`).join('')}</div>` : ''}
        ${imgs.length ? `<div class="rpwl-rep-sec">Screenshots</div><div class="rpwl-rep-media">${imgs.map((m, i) => `<img class="shot" data-med="i${i}" src="${assetUrl(m.path)}" alt="${escapeText(m.name)}" title="${escapeText(m.name)}">`).join('')}</div>` : ''}`;
      host.hidden = false;
      setLogFolded(true); // report owns the tab; ▾ log brings the raw log back
      host.querySelectorAll('[data-med]').forEach((el) => {
        el.onclick = () => {
          const k = el.dataset.med;
          const m = k[0] === 'v' ? vids[+k.slice(1)] : imgs[+k.slice(1)];
          if (m) openMedia(m);
        };
      });
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
    function applyTicket(t) {
      goalText = ticketToGoal(t); origin = { id: t.id };
      renderSelected(metaOf());
      const pe = planEls(); if (pe) { if (pe.input) pe.input.value = goalText; renderPlanCtx(); }
    }

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
      active = localStorage.getItem(scopeKey()) || localStorage.getItem(SUBTAB_KEY) || 'plan';
      if (!root) {
        container.innerHTML = `<div class="rpws"><div class="rpws-body"><div class="rpws-empty">
          <div class="rpws-ico">${ICON.artifact}</div><h3>No project open</h3>
          <p>Open a project (or focus a terminal in one) to use its Workspace.</p></div></div></div>`;
        return;
      }
      container.innerHTML = `<div class="rpws">
        <div class="rpws-head"><div class="rpws-scope">Workspace · <strong>${escapeText(basename(root))}</strong></div></div>
        <div class="rpws-nav">${SUBTABS.map(([k, label]) => `<button data-sub="${k}"${k === active ? ' class="active"' : ''}>${label}</button>`).join('')}</div>
        <div class="rpws-body">${planPage()}${loopsPage()}${outputPage()}${agenticPage()}</div>
      </div>`;
      applyActive();
      loadTab(active);
      const refresh = container.querySelector('[data-act="refresh"]');
      if (refresh) refresh.onclick = () => loadAgentic();
      const stopb = container.querySelector('[data-loom-stop]');
      if (stopb) { stopb.onclick = () => stopRun(); stopb.hidden = !running; }
      const foldb = container.querySelector('[data-loom-fold]');
      if (foldb) foldb.onclick = () => setLogFolded(!container.querySelector('.rpwl-out').classList.contains('logfolded'));
      container.querySelectorAll('.rpws-nav button').forEach((b) => {
        b.onclick = () => switchTab(b.dataset.sub);
      });
    }

    function applyActive() {
      container.querySelectorAll('.rpws-page').forEach((p) => p.classList.toggle('active', p.dataset.page === active));
    }

    // Flow A: PM ticket 3-dot → "Create Loom" → open the Plan chat with the
    // ticket as the pre-filled goal (+ origin chip). One mechanism, two entries.
    window.xnautCreateLoomFromTicket = function (ticket) {
      if (!ticket) return;
      pending = { goal: ticketToGoal(ticket), origin: { id: ticket.id } };
      active = 'plan';
      try { localStorage.setItem(SUBTAB_KEY, 'plan'); localStorage.setItem(scopeKey(), 'plan'); } catch (_) {}
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
