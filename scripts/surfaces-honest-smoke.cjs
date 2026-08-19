// A surface may not claim more than the code does (XNAUT-202).
//
// Three overclaims shipped together, all of them invisible to every other check
// because a sentence in the UI and the code it describes are never compared:
//
//   - the plugin library and the agent editor both said a chat turn does not use
//     plugins, while agent_tools::run_turn opens every plugin the agent holds;
//   - the Collaborators tab said "Enforced at dispatch" for a list no Rust file
//     read at all;
//   - the work-log panel rendered session.generate_summary, a Rust METHOD name
//     that is not a serialised field, so the fallback one-liner always won.
//
// Each check reads BOTH sides. A claim is compared to the code that would make
// it true, so re-introducing either half turns this red.
//
// Run: node scripts/surfaces-honest-smoke.cjs

const { readFileSync, readdirSync } = require('node:fs');
const { join } = require('node:path');

const ROOT = join(__dirname, '..');
const read = (p) => readFileSync(join(ROOT, p), 'utf8');
const JS_DIR = join(ROOT, 'src/js');
const jsFiles = readdirSync(JS_DIR).filter((f) => f.endsWith('.js'));
const surfaces = jsFiles.map((f) => [f, readFileSync(join(JS_DIR, f), 'utf8')]);

const fail = (message) => { throw new Error(message); };

// ---- 1. Chat turns and plugins -----------------------------------------
const agentTools = read('src-tauri/src/agent_tools.rs');
const chatTurnOpensPlugins = /mcp_client::open_for\(capabilities\)/.test(agentTools);
if (!chatTurnOpensPlugins) {
  fail('agent_tools::run_turn no longer opens plugins; the UI copy below is written for the version that does');
}
for (const [name, body] of surfaces) {
  const claim = body.match(/[Cc]hat turns? (?:do not|don't|never) use plugins/);
  if (claim) fail(`${name} says "${claim[0]}" while run_turn opens every plugin the agent holds`);
}

// ---- 2. Collaborators ---------------------------------------------------
// agent-space writes collab:<handle> onto the profile. Something in Rust has to
// read it, or the tab is a setting with no consumer.
const agentSpace = read('src/js/agent-space.js');
if (!agentSpace.includes('`collab:${handle}`')) fail('agent-space no longer writes collab: entries; this check is stale');

const rustDir = join(ROOT, 'src-tauri/src');
const readsCollab = readdirSync(rustDir)
  .filter((f) => f.endsWith('.rs'))
  .some((f) => readFileSync(join(rustDir, f), 'utf8').includes('"collab:"'));
if (!readsCollab) fail('no Rust file reads the collab: prefix, so the Collaborators tab is a setting nobody consumes');

// Reaching the prompt is not enforcement. Nothing blocks a hand-off: there is no
// hand-off tool and no dispatch-time check, so the word must not appear here.
const collabHelp = agentSpace.match(/Which agents this one may[^<]*|Named in this agent's prompt[^<]*/);
if (collabHelp && /enforced/i.test(collabHelp[0])) {
  fail(`the Collaborators help claims enforcement: "${collabHelp[0]}"`);
}

// ---- 3. The work-log panel reads fields that exist -----------------------
const worklog = read('src-tauri/src/worklog.rs');
const struct = worklog.split('pub struct WorkSession {')[1].split('}')[0];
const fields = new Set(
  struct
    .split('\n')
    .map((line) => line.trim().match(/^pub ([a-z_]+):/))
    .filter(Boolean)
    .map((m) => m[1]),
);
if (!fields.has('merkle_root')) fail('could not parse the WorkSession fields; this check is stale');

const app = read('src/js/app.js');
const toggle = app.slice(app.indexOf('async function toggleWorkLog()'));
const body = toggle.slice(0, toggle.indexOf('\n}\n') + 1);
if (!body.includes('worklog_stop')) fail('could not find the toggleWorkLog body; this check is stale');
for (const match of body.matchAll(/(?<!`)session\.([a-zA-Z_]+)/g)) {
  if (!fields.has(match[1])) {
    fail(`app.js reads session.${match[1]}, which is not a serialised WorkSession field (it is undefined at runtime)`);
  }
}
// The summary the panel renders has to be the one the backend produced.
if (!/marked\.parse\(summary\)/.test(body)) {
  fail('the work-log panel renders something other than the summary it fetched');
}

console.log('surfaces honest smoke: ok');
