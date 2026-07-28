// Designer agent (XNAUT-61) — composes the prompt and runs it inside the
// design's own sandbox. The Designer always produces a REAL project: the first
// turn scaffolds the starter stack for the kind, later turns edit that source.
// Execution + code return live in Rust (designer_agent_run); this file owns
// only the prompt.
(function () {
  'use strict';

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

    const head = 'You are a senior product designer AND the engineer who ships it. You are working in '
      + '/workspace, which is a REAL project served by a live dev server on port 80 — never mocks, never '
      + 'placeholder pages. Everything you write must build and run.\n\n'
      + 'PROJECT: ' + project.name + (project.purpose ? ' — ' + project.purpose : '') + '\n'
      + 'DESIGN: "' + design.name + '" (kind: ' + design.kind + ')\n\n';

    const task = first
      ? 'FIRST TURN — scaffold and build it for real:\n'
        + '1. Scaffold ' + starter + ' directly in /workspace (the folder already contains design.json — keep it).\n'
        + '2. Install dependencies so `npm run dev` serves on 0.0.0.0:80.\n'
        + '3. Design and write the actual pages per the owner brief below: real copy (never lorem ipsum), '
        + 'design tokens as CSS custom properties in :root, an 8px spacing system, responsive.\n'
        + '4. Verify it builds. If the dev server is already running it hot-reloads; do not kill it.\n'
      : 'FOLLOW-UP TURN — edit the existing real source in /workspace:\n'
        + '1. Read what is there first; keep the established tokens, structure and voice unless asked to change them.\n'
        + '2. Make exactly the change asked for. Do not rewrite the project.\n'
        + '3. Keep it building — the dev server hot-reloads, so a syntax error is visible immediately.\n';

    const tail = '\nOWNER BRIEF:\n' + ask + '\n\n'
      + (history ? 'CONVERSATION SO FAR:\n' + history + '\n\n' : '')
      + 'When done, reply with ONE short paragraph describing what you built or changed. '
      + 'No preamble, no file listing — the UI shows changed files itself.';

    return head + doctrine() + '\n\n' + task + tail;
  }

  async function run(project, design, ask) {
    let model = 'claude-fable-5';
    try { model = localStorage.getItem('xnaut-loom-model') || model; } catch (_) {}
    const reply = await invoke('designer_agent_run', {
      project: project.name,
      slug: design.slug,
      prompt: buildPrompt(project, design, ask),
      model,
    });
    return {
      text: (reply && reply.text) || '(no reply)',
      files: (reply && reply.files) || [],
    };
  }

  window.xnautDesignerAgent = { run, buildPrompt };
})();
