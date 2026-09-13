// The NautFlow Build stage's SANDBOX runtime (XNAUT-354).
//
// This engine used to live in multiagent-pane.js, the Multi-Agent Manager,
// which is gone: NautBot plans and dispatches swarms now, through the run
// registry and the ledger, and a second un-gated way to put agents on a batch
// of TICKETS is exactly what that ticket retired.
//
// This is not that. A Build slice is a piece of a NautFlow SPECIFICATION —
// synthetic, with no PM ticket behind it and nothing for dispatch.rs to read
// — so it could not go through the new tools even in principle. It kept its
// engine, and the engine moved here, to the stage that is its only caller
// (André, 2026-09-13, asked which way to take it).
//
// The Build stage owns everything above: which slices, which model, local or
// sandbox. This file answers one question — "run these worktrees in GitVM
// sandboxes and ship each green one" — and publishes what it is doing as
// window.xnautBuild, which the Build run pane, the build log viewer and the
// build files pane read.
//
// Renamed from window.xnautSwarm, and that is not cosmetic: the global was
// named for a pane that no longer exists, and every remaining writer of it is
// the Build stage. A name that points at a deleted file is how the next reader
// goes looking for a swarm engine that is not there any more.
(function () {
  'use strict';

  // The agent command line is NOT built here (XNAUT-266). This used to pick
  // the CLI from the model string with its own regexes and paste its own
  // flags — a third answer to a question the runtime registry already answers
  // for the fleet and for looms, and therefore a third way for a runtime to be
  // launched wrongly. `agent_headless_command` is the one answer, and it
  // carries XNAUT-107's hook rule (unattended tasks skip inherited hooks;
  // managed runs keep their veto) with it.
  async function headlessAgentCommand(model, goalFile, opts) {
    return await invoke('agent_headless_command', {
      model: model || '', goalFile: goalFile,
      resume: (opts && opts.resume) || null, isolateMcp: !!(opts && opts.isolateMcp),
    });
  }

  const invoke = (...a) => window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke(...a);
  const notify = (t, b) => { if (window.xnautNotify) window.xnautNotify(t, b); };

  // Ceiling on concurrent sandbox slices. A fork-bomb guard, not a tested
  // limit: one worktree and one agent per slot, so the machine is the real
  // constraint. The Build manager already plans 1-3 slices, so this only ever
  // bites a plan that went wrong.
  const MAX_SLICES = 64;

  // ---- build state (published for the build panes) ---------------------------
  const build = {
    queue: [],        // [{id, title, project, root, status: queued|running|done|failed|cancelled, started, runId, pid, wt, pr}]
    project: '',
    model: localStorage.getItem('xnaut-loom-model') || 'claude-fable-5',
    runtime: localStorage.getItem('xnaut-build-runtime') || 'local', // 'local' shell | 'sandbox' gitvm
    loomName: '',
    active: false,
    async stopAll() {
      for (const t of build.queue) {
        if (t.status === 'running' && t.pid) {
          try { await invoke('loom_run_stop', { pid: t.pid }); } catch (_) {}
          try { await invoke('loom_run_mark', { id: t.runId, status: 'cancelled' }); } catch (_) {}
          t.status = 'cancelled';
        } else if (t.status === 'queued') t.status = 'cancelled';
      }
      build.active = false;
      publish();
    },
  };
  window.xnautBuild = build;
  function publish() {
    try { window.dispatchEvent(new CustomEvent('xnaut-build-update')); } catch (_) {}
  }

  // ---- the runner -------------------------------------------------------------
  async function resolveRoot(projectKey) {
    try { return (await window.xnautLoom.resolveProjectRoot(projectKey)) || ''; } catch (_) { return ''; }
  }

  // Pick a build loom and read the FULL weave (with .metadata), which
  // runSlice requires — a bare list item has no .metadata, and passing one
  // makes the launch throw after queueing but before running (the "stuck
  // queued" bug).
  async function pickFullLoom() {
    const looms = (await window.xnautLoom.listLooms()) || [];
    const pick = looms.find((l) => l && /build|dev|code|feature/i.test(l.name || '')) || looms.find((l) => l && l.name !== 'blank') || looms[0];
    if (!pick) throw new Error('No looms available — seed one in the Looms library first.');
    let full; try { full = await invoke('loom_read', { path: pick.path }); } catch (e) { throw new Error('Could not read loom "' + (pick.name || '?') + '": ' + String((e && e.message) || e)); }
    if (!full || !full.metadata) throw new Error('Loom "' + (pick.name || '?') + '" is not a valid weave.');
    return full;
  }

  // A slice named after a real PM ticket gets its outcome written back. Build
  // slices are usually synthetic (a branch name, not a ticket), and then this
  // finds nothing and does nothing, which is the correct answer rather than a
  // reason to drop the call.
  async function closeTicket(id, ok, prUrl) {
    try {
      const tickets = (await invoke('pm_ticket_list', { project: null })) || [];
      const t = tickets.find((x) => x.id === id); if (!t) return;
      const stamp = new Date().toISOString().slice(0, 16).replace('T', ' ');
      const note = '\n\n> NautFlow build run ' + stamp + ' — ' + (ok ? '✓ acceptance green' : '✗ failed') + '.'
        + (prUrl ? ' PR: ' + prUrl : ok ? ' Shipped on branch nautloom/' + id.toLowerCase() + '.' : '');
      const req = { id: t.id, expected_revision: t.revision, body: (t.body || '') + note };
      if (ok && ['inbox', 'ready', 'in_progress'].indexOf(t.status) >= 0) req.status = 'review';
      await invoke('pm_ticket_update', { request: req });
    } catch (_) {}
  }

  async function runSlice(t, chosenLoom) {
    const L = window.xnautLoom;
    t.status = 'running'; t.started = Date.now(); publish();
    try {
      // 1. worktree — own directory → own sandbox
      const branch = 'nautloom/' + t.id.toLowerCase();
      const wt = await invoke('worktree_suggest_path', { repoPath: t.root, branch: branch });
      try {
        await invoke('worktree_add', { repoPath: t.root, worktreePath: wt, opts: { branch: branch, base: null, checkout_existing: false } });
      } catch (_) {
        // branch exists from an earlier run → check it out instead
        await invoke('worktree_add', { repoPath: t.root, worktreePath: wt, opts: { branch: branch, base: null, checkout_existing: true } });
      }
      t.wt = wt;
      // 2. compose the run — LOCAL shell (agent CLI in the worktree, no GitVM) or
      // the SANDBOX loom. Default is local so nothing touches GitVM.
      const ticket = t.full;
      const goal = L.enrichGoal(chosenLoom, L.ticketToGoal(ticket));
      const runId = 'run-' + Date.now() + '-' + t.id.toLowerCase();
      let script, provider;
      if (build.runtime === 'local') {
        // Run the agent headless in the worktree (bash has no TTY, so an
        // interactive runner can't run here). There is no zellij session to
        // attach to either; the honest pointer is the worktree itself. The goal
        // is written to .build-goal.txt and passed to the CLI.
        const agent = await headlessAgentCommand(build.model, '.build-goal.txt');
        const q = "'" + String(wt).replace(/'/g, "'\\''") + "'";
        script = 'cd ' + q + ' || exit 1\n'
          + "cat > .build-goal.txt <<'__GOAL__'\n" + goal + "\n__GOAL__\n"
          + 'echo "» local build in ' + t.id + ' — headless, worktree: ' + wt + '"\n'
          + agent + ' 2>&1\n'
          + 'echo "__LOOM_DONE__ $?"';
        provider = 'local';
      } else {
        const v = L.verifyWeave(chosenLoom);
        if (!v.ok) throw new Error('loom invalid: ' + v.issues.join('; '));
        script = L.composeCommands(chosenLoom, v.provider).map((c) => 'echo "» ' + c.action + '"; ' + c.cmd).join('\n');
        provider = v.provider;
      }
      const h = await invoke('loom_run', { runId: runId, script: script, goal: goal, cwd: wt, model: build.model });
      t.runId = runId; t.pid = h.pid; t.log = h.log; publish();
      try { await invoke('loom_run_record', { runId: runId, weave: chosenLoom.metadata.name, goal: t.id + ': ' + t.title, provider: provider, pid: h.pid, model: build.model, cwd: wt }); } catch (_) {}
      // 3. wait for __LOOM_DONE__ (poll the log; bail if the driver dies)
      const code = await new Promise((resolve) => {
        let stale = 0;
        const step = async () => {
          if (t.status === 'cancelled') return resolve('cancelled');
          let txt = ''; try { txt = (await invoke('read_file', { path: h.log })) || ''; } catch (_) {}
          const m = txt.match(/__LOOM_DONE__ (\d+)/);
          if (m) return resolve(m[1]);
          if (++stale >= 20) {
            stale = 0;
            let alive = true; try { alive = await invoke('loom_run_alive', { pid: h.pid }); } catch (_) {}
            if (!alive) return resolve('dead');
          }
          setTimeout(step, 3000);
        };
        step();
      });
      if (code === 'cancelled') return;
      const ok = code === '0';
      try { await invoke('loom_run_mark', { id: runId, status: ok ? 'done' : 'failed' }); } catch (_) {}
      // 4. ship on green: commit in the worktree (branch already checked out) → push → PR
      let prUrl = '';
      if (ok) {
        try {
          const ship = await invoke('loom_ship', { cwd: wt, branch: branch, message: t.id + ': ' + t.title + '\n\nProduced by a NautFlow sandbox build run — acceptance green.\nRefs ' + t.id + '\n\nCo-Authored-By: NautLoom Cloud Agent <noreply@48nauts.com>' });
          try {
            const pr = await invoke('forge_create_pr', { forgeIndex: 0, repo: ship.org_repo, head: ship.branch, base: 'main', title: t.id + ': ' + t.title, body: 'Automated NautFlow sandbox build run — acceptance green. Report + demo video in the run artifacts.\n\nRefs ' + t.id });
            const u = String(pr || '').match(/https?:\/\/\S+/); prUrl = u ? u[0] : '';
            t.pr = prUrl || true;
          } catch (_) { t.pr = ''; }
        } catch (_) {}
      }
      await closeTicket(t.id, ok, prUrl);
      t.status = ok ? 'done' : 'failed'; publish();
      notify('Build · ' + t.id + (ok ? ' ✓ done' : ' ✗ failed'), ok ? (prUrl ? 'PR created: ' + prUrl : 'Shipped on ' + branch) : 'Run did not finish green — worktree kept for debugging.');
      // 5. Keep the worktree: its artifacts/ holds the report + demo video the
      // Output report card renders. Cleanup is deferred to PR-merge time —
      // removing it here destroyed the reports (learned the hard way).
    } catch (e) {
      t.status = 'failed'; t.err = String((e && e.message) || e); publish();
      notify('Build · ' + t.id + ' ✗ error', t.err.slice(0, 140));
    }
  }

  function runSlices(slices, chosenLoom, addNote) {
    build.queue = slices.map((t) => ({ id: t.id, title: t.title, project: t.project, root: t._root, full: t, status: 'queued' }));
    build.project = slices[0] ? slices[0].project : '';
    build.loomName = chosenLoom.metadata.name;
    build.active = true;
    publish();
    const next = async () => {
      if (!build.active) return;
      const t = build.queue.find((x) => x.status === 'queued');
      if (!t) return;
      await runSlice(t, chosenLoom);
      if (!build.queue.some((x) => x.status === 'queued' || x.status === 'running')) {
        const ok = build.queue.filter((x) => x.status === 'done').length;
        const prs = build.queue.filter((x) => x.pr).length;
        build.active = false; publish();
        notify('Build complete', ok + '/' + build.queue.length + ' green — ' + prs + ' PR' + (prs === 1 ? '' : 's') + '.');
        addNote('Build complete: ' + ok + '/' + build.queue.length + ' green, ' + prs + ' PR(s) opened. Details in the Observatory.');
        return;
      }
      next(); // this slot takes the next queued slice
    };
    // The Build manager decided the parallelism when it planned the slices;
    // the cap above only catches a plan that went wrong.
    const slots = Math.max(1, Math.min(MAX_SLICES, build.queue.length));
    for (let i = 0; i < slots; i++) next();
  }

  // Run an explicit worktree plan in sandboxes. The Build manager decides the
  // parallelism (1-3 worktrees, each with its own branch and scoped goal);
  // each worktree is a slice whose goal is the plan's share of the spec.
  build.launchPlan = async function (projectKey, worktrees, opts) {
    opts = opts || {};
    if (build.active) throw new Error('A build is already running.');
    if (opts.model) build.model = opts.model;
    if (opts.runtime) { build.runtime = opts.runtime; try { localStorage.setItem('xnaut-build-runtime', opts.runtime); } catch (_) {} }
    const chosen = await pickFullLoom();
    const root = await resolveRoot(projectKey);
    if (!root) throw new Error('No local folder for ' + projectKey + '. Set the source path in project Settings.');
    const work = (worktrees || []).map((w, i) => {
      const id = (w.branch || projectKey + '-w' + (i + 1)).replace(/^nautloom\//, '');
      return {
        id: id, title: w.title || w.goal || ('Worktree ' + (i + 1)), project: projectKey, _root: root,
        full: { id: id, title: w.title || '', body: w.goal || '', documentation: [] },
      };
    });
    if (!work.length) throw new Error('Empty build plan.');
    runSlices(work, chosen, opts.addNote || (() => {})); // fire-and-forget; the queue updates via 'xnaut-build-update'
    return { count: work.length };
  };
})();
