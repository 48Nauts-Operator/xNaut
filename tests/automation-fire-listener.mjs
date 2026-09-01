// The automation://fire listener has to exist before anyone opens the
// Automations panel.
//
// The tron rig, 2026-09-01: the scheduler's cadence was exact and every fire
// was emitted, but `listen('automation://fire')` was called from inside
// createAutomationsPanel. On a fresh app that had never opened the panel there
// was no listener, so a fire produced no tab and no toast; opening it once
// armed it for the app's life. Nothing errored, which is why it survived.
//
//   node tests/automation-fire-listener.mjs
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const panelSrc = readFileSync(new URL('../src/js/automations-panel.js', import.meta.url), 'utf8');
const appSrc = readFileSync(new URL('../src/js/app.js', import.meta.url), 'utf8');

/// Loads the panel IIFE against a throwaway window and records every Tauri
/// event subscription it makes.
function load() {
  const listened = [];
  const win = {
    __TAURI__: {
      core: { invoke: async () => ({}) },
      event: {
        listen: (name, cb) => {
          listened.push({ name, cb });
          return Promise.resolve(() => {});
        },
      },
    },
  };
  // Enough DOM for the toast path a fire takes; nothing is rendered.
  const node = () => ({
    style: {}, dataset: {}, classList: { add() {}, remove() {} },
    appendChild() {}, remove() {}, querySelector: () => null,
    set innerHTML(_) {}, set textContent(_) {},
  });
  const doc = {
    getElementById: () => null,
    querySelector: () => null,
    createElement: node,
    head: node(),
    body: node(),
  };
  const timers = [];
  new Function('window', 'document', 'setTimeout', panelSrc)(win, doc, (fn) => timers.push(fn));
  return { win, listened };
}

{
  // 1. The wiring is reachable without constructing a panel.
  const { win, listened } = load();
  assert.equal(typeof win.xnautWireAutomationFire, 'function',
    'the panel must export its fire wiring so app start can call it');
  assert.deepEqual(listened, [], 'merely loading the file subscribes to nothing');

  win.xnautWireAutomationFire();
  assert.deepEqual(listened.map((l) => l.name), ['automation://fire'],
    'calling it subscribes exactly once, to the fire event');

  // Idempotent: app start and a later panel open must not double-subscribe.
  win.xnautWireAutomationFire();
  assert.equal(listened.length, 1, 'the listener is wired at most once');
}

{
  // 2. A successful fire attaches the tab the backend already opened.
  const { win, listened } = load();
  const attached = [];
  win.xnautAutomationFired = (payload) => attached.push(payload);
  win.xnautWireAutomationFire();
  listened[0].cb({ payload: { automation: { name: 'Audit' }, session_id: 'abc123', error: null } });
  assert.deepEqual(attached, [{ automation: { name: 'Audit' }, session_id: 'abc123', error: null }]);
}

{
  // 3. A failed fire is NOT handed on as a run; the panel toasts the reason.
  // "unknown agent id: rigtwo" reached one console.error and nothing else.
  const { win, listened } = load();
  const attached = [];
  win.xnautAutomationFired = (payload) => attached.push(payload);
  win.xnautWireAutomationFire();
  listened[0].cb({
    payload: { automation: { name: 'Audit' }, session_id: null, error: 'no agent called "rigtwo"' },
  });
  assert.deepEqual(attached, [], 'a failure is not an opened session');
}

{
  // 4. And app start actually calls it. The export is worthless if the only
  // caller is still the panel constructor; that is the original bug.
  assert.match(appSrc, /xnautWireAutomationFire\(\)/,
    'app.js must wire the automation fire listener during init');
}

console.log('✓ automation fire listener is wired at app start, not per-panel');
