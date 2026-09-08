// Designer agent (XNAUT-61) — composes the prompt and runs it inside the
// design's own sandbox. The Designer always produces a REAL project: the first
// turn scaffolds the starter stack for the kind, later turns edit that source.
// Execution goes through the app's one agent runner (loom_run +
// xnautDriveRun) — this file owns only the prompt and the command line.
(function () {
  'use strict';

  // XNAUT-107: unattended tasks skip inherited hooks; managed runs keep their veto.
  // One place builds an agent's headless command line (XNAUT-266,
  // `agents::headless_command`); this file used to build its own.
  async function headlessAgentCommand(model, goalFile, opts) {
    return await invoke('agent_headless_command', {
      model: model || '', goalFile: goalFile,
      resume: (opts && opts.resume) || null, isolateMcp: !!(opts && opts.isolateMcp),
    });
  }

  const invoke = (...a) => window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke(...a);

  // Starter stack per kind — mirrors designer.rs::starter_for.
  const STARTER = {
    website: 'an Astro site (`npm create astro@latest . -- --template minimal --no-install --git=false`), one route per page',
    deck: 'a Reveal.js deck (npm init + reveal.js dependency), one <section> per slide, 16:9',
    document: 'an Astro page styled for print/PDF — a single long-form document',
    appui: 'a Next.js app (`npx create-next-app@latest . --ts --app --no-git`), one route per screen',
  };

  // The craft guide is the reason output looks good — reuse the one already
  // shipped for the NAUT-Flow designer rather than a second copy.
  function doctrine() {
    return (window.XNAUT_DESIGN_DOCTRINE || '').trim();
  }

  function buildPrompt(project, design, ask) {
    const first = !(design.messages || []).some((m) => m.role === 'agent');
    const starter = STARTER[design.kind] || STARTER.website;
    const history = (design.messages || [])
      .slice(-8)
      .map((m) => (m.role === 'user' ? 'OWNER: ' : 'YOU: ') + m.text)
      .join('\n');

    // The serving contract differs by runtime. In both cases the port belongs
    // to xNAUT, not the generated project, so a hardcoded script/config port
    // can never be correct for every design run.
    const hosting = design.runtime === 'local'
      ? 'HOSTING — this design runs on the owner’s machine. xNAUT either starts or adopts '
        + '`npm run dev -- --host 127.0.0.1 --port <a free port>`. Do NOT hardcode a port in package.json '
        + 'or the framework config; the command-line port must win. If you start a dev server for visual '
        + 'review, bind it to 127.0.0.1 on any free port and leave it running so xNAUT can adopt it.\n'
        + 'Make sure `npm run dev` accepts the host and port flags — that command serves the canvas.\n\n'
      : 'HOSTING — the site is served from a sandbox behind a proxy on an arbitrary hostname '
        + '(*.nautbox.dev). It is started with `npm run dev -- --host 0.0.0.0 --port <the sandbox port>`, '
        + 'so do NOT hardcode a port in the config (the flag would fight it), but DO accept any Host header:\n'
        + '- Vite / Astro: in the config set `server: { host: true, allowedHosts: true }` — without allowedHosts '
        + 'the proxy gets a 403 "host not allowed" and the canvas stays blank (verified 2026-08-01).\n'
        + '- Next.js: no host check needed.\n'
        + 'Make sure `npm run dev` works with those flags — that command serves the canvas.\n\n';

    const head = 'You are a senior product designer AND the engineer who ships it. Your working directory '
      + 'is a REAL project that gets built and served by a live dev server — never mocks, never '
      + 'placeholder pages. Everything you write must build and run.\n\n'
      + 'PROJECT: ' + project.name + (project.purpose ? ' — ' + project.purpose : '') + '\n'
      + 'DESIGN: "' + design.name + '" (kind: ' + design.kind + ')\n\n';

    const task = first
      ? 'FIRST TURN — scaffold and build it for real:\n'
        + '1. Scaffold ' + starter + ' directly in the working directory (it already contains design.json — keep it).\n'
        + "2. Install dependencies so `npm run dev` starts cleanly.\n"
        + '3. Design and write the actual pages per the owner brief below: real copy (never lorem ipsum), '
        + 'design tokens as CSS custom properties in :root, an 8px spacing system, responsive.\n'
        + '4. Verify it builds. If the dev server is already running it hot-reloads; do not kill it.\n'
      : 'FOLLOW-UP TURN — edit the existing real source in the working directory:\n'
        + '1. Read what is there first; keep the established tokens, structure and voice unless asked to change them.\n'
        + '2. Make exactly the change asked for. Do not rewrite the project.\n'
        + '3. Keep it building — the dev server hot-reloads, so a syntax error is visible immediately.\n';

    const tail = '\nOWNER BRIEF:\n' + ask + '\n\n'
      + (history ? 'CONVERSATION SO FAR:\n' + history + '\n\n' : '')
      + 'When done, reply with ONE short paragraph describing what you built or changed. '
      + 'No preamble, no file listing — the UI shows changed files itself.';

    return head + hosting + doctrine() + '\n\n' + task + tail;
  }

  /// Runs one turn on the SAME machinery as a NautFlow persona: `loom_run`
  /// spawns the detached agent, `loom_run_record` puts it in the Observatory,
  /// and window.xnautDriveRun streams + completes it. Nothing here duplicates
  /// that — this function only composes the command line and the sink.
  ///
  /// `io.dir` is the design folder (the agent's cwd, where loom_run drops
  /// .loom-goal.txt); `io.line(text, cls)` receives every streamed event.
  async function run(project, design, ask, io) {
    if (typeof window.xnautDriveRun !== 'function') throw new Error('agent runner not loaded');
    // Opus 5 is the Designer's default — this is the build agent, not a cheap
    // summariser. localStorage only overrides it if explicitly set.
    let model = 'claude-opus-5';
    try { model = localStorage.getItem('xnaut-designer-model') || model; } catch (_) {}

    // Same command shape as runPersonaHeadless: stream-json so every tool call
    // is visible, no user MCP servers (their teardown stalls the run for
    // minutes), and --resume so a follow-up turn keeps the design's context.
    const PATHX = 'export PATH="/opt/homebrew/bin:/usr/local/bin:$HOME/.local/bin:$PATH"\n';
    const agentLine = await headlessAgentCommand(model, '.loom-goal.txt', { resume: design.session_id, isolateMcp: true });

    // The agent runs LOCALLY in the design folder, not inside the sandbox:
    // Claude Code on macOS keeps credentials in the login Keychain, so there is
    // no .credentials.json for `gitvm warm-up --authSync` to copy and a sandbox
    // agent answers "Not logged in · Please run /login" (verified 2026-08-01).
    // The sandbox builds and serves what the agent wrote — designer_publish
    // rsyncs it in afterwards.
    const runId = 'designer-' + design.slug + '-' + Date.now();
    const goal = buildPrompt(project, design, ask);
    const h = await invoke('loom_run', { runId, script: PATHX + agentLine + ' 2>&1', goal, cwd: io.dir, model });
    try {
      await invoke('loom_run_record', {
        runId, weave: 'Designer · ' + design.name, goal, provider: 'local', pid: h.pid, model, cwd: io.dir,
      });
    } catch (_) {} // → Observatory; a missing record must not fail the turn

    // The driver owns completion. Its last plain-text line is the agent's reply.
    let reply = '';
    return await new Promise((resolve) => {
      window.xnautDriveRun({
        role: 'Designer', stageTitle: design.name, rel: '', h, runId,
        mode: 'local', model, start: Date.now(),
        opts: {
          chatIdle: true, // a pure-text answer finishes on 60s idle
          view: {
            reset() {}, title() {}, status() {}, running() {}, elapsed() {},
            line(text, cls) { if (cls === '#c9cdd6') reply = text; io.line(text, cls); },
          },
          onSession(id) { io.session && io.session(id); },
          onDone(ok) { resolve({ ok, text: reply || (ok ? 'Done.' : 'The run ended without a reply.') }); },
        },
      });
    });
  }

  window.xnautDesignerAgent = { run, buildPrompt };
})();
