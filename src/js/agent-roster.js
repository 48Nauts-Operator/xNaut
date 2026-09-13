// The agent roster — which harness, provider and model plays each role.
//
// All of this existed already, scattered: roleFrontierModel() was a switch of
// hardcoded model ids, and each stage toolbar carried its own override persisted
// under `xnaut-nf-model:<project>:<stage>`. This is the same idea in one place,
// with four problems fixed.
//
//   1. The defaults were HARDCODED, which contradicts our own rule that model
//      lists are fetched daily and never hardcoded. When a model is retired the
//      switch keeps naming it, and the failure is silent. Defaults now resolve
//      through the live catalogue, with the literal only as a last resort.
//   2. A role binding is a TRIPLE — harness, provider, model — not a bare model
//      id. `Builder: 'codex'` in the old switch was a harness masquerading as a
//      model, which is exactly what that conflation produces.
//   3. There is a GLOBAL default set, with a per-project override, rather than
//      every new project starting from the switch again.
//   4. A pinned model that has vanished from the catalogue is reported STALE and
//      falls back visibly. A silent substitution is how you end up wondering why
//      the Architect got worse.
//
// A different provider for the Reviewer than the Architect is a FEATURE, not an
// accident: different training, different blind spots. Worth saying in the UI,
// because the instinct is to set everything to the strongest model and lose
// exactly the diversity that makes a review worth having.
(function () {
  'use strict';

  const GLOBAL_KEY = 'xnaut-agent-roster';
  const projectKey = (p) => 'xnaut-agent-roster:' + p;
  // The old per-stage override, so existing choices are not silently discarded.
  const legacyKey = (p, stage) => 'xnaut-nf-model:' + p + ':' + stage;

  // Roles, in the order a flow uses them. `want` describes the CAPABILITY the
  // role needs, so a default can be resolved from whatever the catalogue offers
  // today rather than from a model name frozen in source.
  const ROLES = [
    { id: 'Analyst',    label: 'Analyst',    want: 'creative',  note: 'wide-ranging discovery' },
    { id: 'PM',         label: 'PM',         want: 'balanced',  note: '' },
    { id: 'Architect',  label: 'Architect',  want: 'reasoning', note: 'hardest technical reasoning' },
    { id: 'Security',   label: 'Security',   want: 'reasoning', note: '' },
    { id: 'Planner',    label: 'Planner',    want: 'balanced',  note: '' },
    { id: 'Reviewer',   label: 'Reviewer',   want: 'balanced',  note: 'a different provider here is a feature' },
    { id: 'Validator',  label: 'Validator',  want: 'creative',  note: 'release gate' },
    { id: 'Designer',   label: 'Designer',   want: 'reasoning', note: 'HTML mocks' },
    { id: 'Builder',    label: 'Developer',  want: 'coding',    note: 'writes the code' },
    { id: 'Integrator', label: 'Integrator', want: 'balanced',  note: 'merges, verifies, opens the PR' },
  ];

  // Last-resort literals. Only reached when the catalogue is empty — a fresh
  // install before the first fetch, or every provider unreachable. Kept
  // deliberately small: this is a fallback, not a configuration surface.
  const FALLBACK = {
    creative:  { harness: 'claude', provider: 'anthropic', model: 'claude-fable-5' },
    reasoning: { harness: 'claude', provider: 'anthropic', model: 'claude-opus-5' },
    balanced:  { harness: 'claude', provider: 'anthropic', model: 'claude-sonnet-5' },
    coding:    { harness: 'codex',  provider: 'openai',    model: '' },
  };

  // Which harness can run a given provider's models.
  function harnessFor(provider) {
    if (provider === 'openai') return 'codex';
    if (provider === 'anthropic' || provider === 'lmstudio' || provider === 'ollama') return 'claude';
    return 'claude'; // openrouter and friends: reachable through the Claude harness
  }

  const catalogue = () => (window.xnautModelCatalog && window.xnautModelCatalog.all()) || [];
  const modelId = (m) => (typeof m === 'string' ? m : (m && (m.id || m.name)) || '');

  function knows(provider, model) {
    if (!model) return true; // "provider default" is always valid
    const list = (window.xnautModelCatalog && window.xnautModelCatalog.forProvider(provider)) || [];
    return list.some((m) => modelId(m) === model);
  }

  // A router slug (`anthropic/claude-opus-4`) is an OpenRouter id, not a model
  // name the local `claude`/`codex` CLI accepts. The catalogue merges every
  // provider into one flat list, so without this filter a default resolved from
  // the OpenRouter half is handed to the harness and the run dies with "issue
  // with the selected model". Hit for real in the NautFlow Designer stage,
  // 2026-08-10.
  // An empty id is "whatever the harness defaults to" — always valid, same rule
  // as knows() below. A slug never is.
  const cliUsable = (id) => !id || !id.includes('/');

  // Version tuple from an id, so `/opus/i` prefers claude-opus-5 over the
  // retired claude-opus-4. The catalogue carries no release date; the digits in
  // the name are the only ordering signal there is.
  const version = (id) => (id.match(/\d+/g) || []).map(Number);
  function newer(a, b) {
    const x = version(a);
    const y = version(b);
    for (let i = 0; i < Math.max(x.length, y.length); i += 1) {
      const d = (x[i] || 0) - (y[i] || 0);
      if (d) return d > 0;
    }
    return false;
  }

  // Pick from the catalogue by capability. Ranked by name because that is the
  // only signal the catalogue carries — deliberately crude, and only used when
  // the user has not chosen.
  function resolveDefault(want) {
    // Chat models only. The catalogue lists everything a provider serves, and
    // a date-sorted /gpt-5/ match handed every role gpt-5-search-api-2025-10-14
    // (Andre, 2026-09-13), which cannot write code.
    const NOT_CHAT = /search|realtime|audio|transcribe|tts|embed|image|moderation|whisper/i;
    const all = catalogue().filter((m) => cliUsable(modelId(m)) && !NOT_CHAT.test(modelId(m)));
    if (!all.length) return { ...FALLBACK[want] };
    const rank = {
      creative:  [/fable/i, /opus/i, /gpt-5/i],
      reasoning: [/opus/i, /gpt-5/i, /fable/i],
      balanced:  [/sonnet/i, /gpt-5/i, /opus/i],
      coding:    [/codex/i, /gpt-5/i, /sonnet/i],
    }[want] || [];
    for (const re of rank) {
      const hits = all.filter((m) => re.test(modelId(m)));
      if (!hits.length) continue;
      const hit = hits.reduce((best, m) => (newer(modelId(m), modelId(best)) ? m : best));
      const provider = hit.provider || 'anthropic';
      return { harness: harnessFor(provider), provider, model: modelId(hit) };
    }
    return { ...FALLBACK[want] };
  }

  function readStore(key) {
    try { return JSON.parse(localStorage.getItem(key) || '{}') || {}; } catch (_) { return {}; }
  }
  function writeStore(key, value) {
    try { localStorage.setItem(key, JSON.stringify(value)); } catch (_) {}
  }

  /// The binding for a role: per-project override, else global, else resolved
  /// default. `stale` is true when a pinned model is no longer in the catalogue.
  function forRole(roleId, project) {
    const role = ROLES.find((r) => r.id === roleId) || { id: roleId, want: 'balanced' };
    const chosen = (project && readStore(projectKey(project))[roleId]) || readStore(GLOBAL_KEY)[roleId] || null;
    const fallback = resolveDefault(role.want);
    // An empty model is a real choice ("this harness, its own default") — the
    // shape a codex binding takes. Discarding it here threw away the chosen
    // harness and provider along with it.
    if (!chosen) return { ...fallback, source: 'default', stale: false };
    // A stored router slug is stale too: the catalogue still knows it, but the
    // CLI cannot run it. Falling back visibly beats failing at spawn time.
    const stale = !knows(chosen.provider, chosen.model) || !cliUsable(chosen.model);
    return {
      harness: chosen.harness || harnessFor(chosen.provider),
      provider: chosen.provider,
      // A vanished model falls back, but the caller is told — never a silent swap.
      model: stale ? fallback.model : chosen.model,
      source: 'chosen',
      stale,
      pinned: stale ? chosen.model : undefined,
    };
  }

  function setRole(roleId, binding, project) {
    const key = project ? projectKey(project) : GLOBAL_KEY;
    const store = readStore(key);
    if (!binding) delete store[roleId];
    else store[roleId] = { harness: binding.harness, provider: binding.provider, model: binding.model };
    writeStore(key, store);
    window.dispatchEvent(new CustomEvent('xnaut-roster-change', { detail: { roleId, project } }));
  }

  /// One-time migration of the old per-stage model ids. They carried no
  /// provider, so it is inferred from the model name — good enough, and better
  /// than discarding a choice the user made.
  function migrate(project, stageIds) {
    if (!project || !Array.isArray(stageIds)) return 0;
    const store = readStore(projectKey(project));
    let moved = 0;
    for (const { stage, role } of stageIds) {
      if (store[role]) continue;
      let old = null;
      try { old = localStorage.getItem(legacyKey(project, stage)); } catch (_) {}
      if (!old) continue;
      const provider = /^gpt|^o[0-9]|codex/i.test(old) ? 'openai' : 'anthropic';
      store[role] = { harness: harnessFor(provider), provider, model: /codex/i.test(old) ? '' : old };
      moved += 1;
    }
    if (moved) writeStore(projectKey(project), store);
    return moved;
  }

  window.xnautAgentRoster = {
    ROLES,
    forRole,
    setRole,
    migrate,
    /// Every role resolved at once, for the roster panel.
    all: (project) => ROLES.map((r) => ({ ...r, binding: forRole(r.id, project) })),
    /// Back-compat for callers that only want a model id.
    modelFor: (roleId, project) => forRole(roleId, project).model,
  };
})();
