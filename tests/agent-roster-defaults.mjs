// Guards the two ways agent-roster.js can hand a model id to a CLI that cannot
// run it: a router slug from the OpenRouter half of the merged catalogue, and a
// retired version winning the name-rank. Both hit the NautFlow Designer stage
// on 2026-08-10 (`● Designer starting on anthropic/claude-opus-4` → failed).
//
//   node tests/agent-roster-defaults.mjs
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const src = readFileSync(new URL('../src/js/agent-roster.js', import.meta.url), 'utf8');

/// Loads the IIFE against a throwaway window. `models` becomes the catalogue,
/// `store` the localStorage contents.
function load(models, store = {}) {
  const win = {
    xnautModelCatalog: {
      all: () => models,
      forProvider: (p) => models.filter((m) => m.provider === p),
    },
    dispatchEvent: () => {},
    CustomEvent: class { constructor(t, d) { this.type = t; Object.assign(this, d); } },
  };
  const localStorage = {
    getItem: (k) => (k in store ? store[k] : null),
    setItem: (k, v) => { store[k] = v; },
  };
  new Function('window', 'localStorage', 'CustomEvent', src)(win, localStorage, win.CustomEvent);
  return win.xnautAgentRoster;
}

// The real shape: OpenRouter mirrors Anthropic's line under a slug, and the
// flat catalogue interleaves both providers.
const CATALOGUE = [
  { id: 'anthropic/claude-opus-4', name: 'Claude Opus 4', provider: 'openrouter' },
  { id: 'claude-opus-4', name: 'Claude Opus 4', provider: 'anthropic' },
  { id: 'claude-opus-5', name: 'Claude Opus 5', provider: 'anthropic' },
  { id: 'claude-sonnet-5', name: 'Claude Sonnet 5', provider: 'anthropic' },
];

{
  // Designer wants `reasoning`, whose first rank regex is /opus/i.
  const roster = load(CATALOGUE);
  const b = roster.forRole('Designer', 'XNAUT');
  assert.equal(b.model, 'claude-opus-5', 'default must skip the router slug and the retired version');
  assert.equal(b.provider, 'anthropic');
  assert.equal(b.harness, 'claude');
}

{
  // A slug already pinned in localStorage falls back, and says so.
  const roster = load(CATALOGUE, {
    'xnaut-agent-roster': JSON.stringify({
      Designer: { harness: 'claude', provider: 'openrouter', model: 'anthropic/claude-opus-4' },
    }),
  });
  const b = roster.forRole('Designer', null);
  assert.equal(b.stale, true, 'a router slug must be reported stale');
  assert.equal(b.pinned, 'anthropic/claude-opus-4');
  assert.equal(b.model, 'claude-opus-5');
}

{
  // An empty model is "provider default" and must stay usable — that is how a
  // codex binding is expressed.
  const roster = load(CATALOGUE, {
    'xnaut-agent-roster': JSON.stringify({ Builder: { harness: 'codex', provider: 'openai', model: '' } }),
  });
  const b = roster.forRole('Builder', null);
  assert.equal(b.stale, false);
  assert.equal(b.model, '');
  assert.equal(b.harness, 'codex', 'an empty model must not discard the chosen harness');
  assert.equal(b.provider, 'openai');
}

{
  // No catalogue (fresh install, providers unreachable) still resolves.
  const roster = load([]);
  assert.equal(roster.forRole('Designer', null).model, 'claude-opus-5');
}

console.log('ok — agent-roster defaults are CLI-usable and version-ranked');
