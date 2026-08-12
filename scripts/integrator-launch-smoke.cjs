// Agent launch verification (XNAUT-93) — a launch is not a start.
//
// Same lifting trick as slice-ports-smoke.cjs: the function is pulled out of the
// real source by name rather than copied, so this cannot pass after the original
// changed. Time is virtual; the probes decide what the world looks like.
const { readFileSync } = require('node:fs');
const src = readFileSync(`${__dirname}/../src/js/project-management-panel.js`, 'utf8');

const m = src.match(/async function awaitAgentSession\(name, cwd, probes, deadlineMs\) \{[\s\S]*?\n {4}\}/);
if (!m) throw new Error('awaitAgentSession not found in project-management-panel.js — did it get renamed?');
const awaitAgentSession = eval(`(${m[0].replace('async function awaitAgentSession', 'async function')})`);

let now = 0;
Date.now = () => now;
const sleep = (ms) => { now += ms; return Promise.resolve(); };

// sessions/agent are functions of the tick count, so a probe can come true late.
const run = (sessions, agent) => {
  let tick = 0;
  return awaitAgentSession('cl-repo', '/repo', {
    liveSessions: () => Promise.resolve(sessions(tick++)),
    agentAlive: () => Promise.resolve(agent(tick)),
    sleep,
  }, 20000);
};
const fails = async (p, needle, label) => {
  try { await p; } catch (e) {
    if (!String(e.message).includes(needle)) throw new Error(`${label}\n  got  ${e.message}\n  want …${needle}…`);
    return;
  }
  throw new Error(`${label}: resolved, expected a throw`);
};

(async () => {
  // The happy path, including a session that takes a few seconds to appear —
  // zellij is not up the instant startShell resolves.
  now = 0;
  await run((t) => (t >= 3 ? ['cl-repo'] : []), () => true);

  // An agent that boots slower than its session must not be called dead.
  now = 0;
  await run(() => ['cl-repo'], (t) => t >= 5);

  // The XNAUT-93 failure itself: the launch was swallowed (or zellij panicked),
  // so no session of this name is ever live. This must be loud, not a toast
  // claiming the Integrator is merging and pushing.
  now = 0;
  await fails(run(() => [], () => true), 'never started',
    'a launch that produced no session must throw');

  // A different name is not this session. `_zj` truncates to 24 chars and so
  // does shellSession; a near-miss here would mean the two drifted apart.
  now = 0;
  await fails(run(() => ['cl-repo-other', 'cl-rep'], () => true), 'never started',
    'a similarly-named session is not the one we launched');

  // Session up, nothing running in it: the bare-shell decay case.
  now = 0;
  await fails(run(() => ['cl-repo'], () => false), 'no agent is running',
    'a session with no agent must name that as the reason');

  // The deadline is real: a probe that never comes true has to end, and roughly
  // when it said it would.
  now = 0;
  await fails(run(() => [], () => true), '20s after launch', 'the deadline is reported');
  if (now < 20000 || now > 22000) throw new Error(`gave up after ${now}ms, expected ~20000`);

  console.log('integrator launch: 6 checks passed');
})();
