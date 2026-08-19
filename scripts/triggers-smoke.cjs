// Terminal triggers: one path, and it matches what the user typed.
//
// The product shipped two implementations. checkTriggers in app.js was the live
// one; src-tauri/src/triggers.rs was complete, dead and emitting three events
// nothing listened for. XNAUT-199 kept the frontend and deleted the backend, so
// this guards both halves of that decision: the surviving matcher behaves, and
// the deleted one does not grow back.

const { existsSync, readFileSync } = require('node:fs');
const { join } = require('node:path');

const REPO = join(__dirname, '..');
const read = (p) => readFileSync(join(REPO, p), 'utf8');
const fail = (m) => { console.error(`FAIL ${m}`); process.exit(1); };

// ---- the surviving matcher -------------------------------------------------

const app = read('src/js/app.js');

// Lift the shipped function rather than a copy of it: a copy drifts, and a
// drifted copy is what makes a green check meaningless.
const m = /\nfunction checkTriggers\(output, now = Date\.now\(\)\) \{\n([\s\S]*?)\n\}\n/.exec(app);
if (!m) fail('checkTriggers(output, now) not found in app.js. Renamed or re-signed?');

const notifications = [];
const checkTriggers = new Function(
  'output', 'now', 'triggers', 'triggerLastFired', 'TRIGGER_COOLDOWN_MS', 'showNotification', 'console',
  m[1],
);
const quiet = { error: () => {} };
const run = (output, triggers, now = 1000, state = new Map(), log = console) => {
  notifications.length = 0;
  checkTriggers(output, now, triggers, state, 10000,
    (title, body) => notifications.push(`${title}: ${body}`), log);
  return state;
};

const trig = (over) => ({ id: 't1', name: 'Alert', message: 'boom', enabled: true, ...over });

// keyword: comma separated, case insensitive
run('build FAILED after 3s', [trig({ type: 'keyword', pattern: 'error, failed' })]);
if (notifications.length !== 1) fail('a keyword in the output did not fire');

run('all good', [trig({ type: 'keyword', pattern: 'error, failed' })]);
if (notifications.length !== 0) fail('a keyword that is absent fired anyway');

// regex
run('exit status 137', [trig({ type: 'regex', pattern: 'status \\d+' })]);
if (notifications.length !== 1) fail('a regex trigger did not fire');

// a broken regex must not take the loop down with it
run('anything', [trig({ type: 'regex', pattern: '([unclosed' }), trig({ id: 't2', type: 'keyword', pattern: 'any' })], 1000, new Map(), quiet);
if (notifications.length !== 1) fail('an invalid regex stopped the triggers after it from running');

// a trailing comma leaves an empty keyword, and every string contains ''
run('nothing to see', [trig({ type: 'keyword', pattern: 'error,' })]);
if (notifications.length !== 0) fail('a trailing comma in the pattern made the trigger fire on everything');

// disabled
run('failed', [trig({ type: 'keyword', pattern: 'failed', enabled: false })]);
if (notifications.length !== 0) fail('a disabled trigger fired');

// escape sequences: OSC 7 carries the working directory into the same chunk,
// so an unfiltered match turns `cd ~/error-logs` into an error alert.
run('\x1b]7;file://host/Users/a/error-logs\x07$ ls\r\n', [trig({ type: 'keyword', pattern: 'error' })]);
if (notifications.length !== 0) fail('an OSC 7 directory path matched as if it were output');

// ...and a colour code inside the word must not hide it
run('\x1b[31mfai\x1b[0mled\x1b[0m', [trig({ type: 'keyword', pattern: 'failed' })]);
if (notifications.length !== 1) fail('a word split by a colour code was missed');

// cooldown: 60 flushes of a streaming agent are not 60 notifications
const triggers = [trig({ type: 'keyword', pattern: 'error' })];
const state = new Map();
let fired = 0;
for (let i = 0; i < 60; i += 1) {
  run('error: still going', triggers, 1000 + i * 16, state);
  fired += notifications.length;
}
if (fired !== 1) fail(`a streaming agent produced ${fired} notifications for one trigger, expected 1`);

run('error: still going', triggers, 1000 + 60 * 1000, state);
if (notifications.length !== 1) fail('the trigger stayed muted after its cooldown expired');

// ---- both feeds reach it ---------------------------------------------------

if (!/checkTriggers\(data\)/.test(app)) fail('the terminal output handler no longer calls checkTriggers');
if (!/window\.xnautCheckTriggers = checkTriggers/.test(app)) fail('checkTriggers is not exposed to the other panes');

const space = read('src/js/agent-space.js');
if (!/window\.xnautCheckTriggers\(chunk\)/.test(space)) fail('Agent Space output no longer reaches the triggers');

// ---- the second path stays deleted ----------------------------------------

if (existsSync(join(REPO, 'src-tauri/src/triggers.rs'))) fail('src-tauri/src/triggers.rs is back. Two trigger paths again');

const forbidden = [
  ['src-tauri/src/main.rs', /mod triggers;|commands::create_trigger/],
  ['src-tauri/src/commands.rs', /fn create_trigger|fn list_triggers|fn toggle_trigger/],
  ['src-tauri/src/state.rs', /enum TriggerAction|struct Trigger\b/],
  ['src-tauri/permissions/default.toml', /create_trigger|list_triggers|toggle_trigger/],
  ['src/js/app.js', /invoke\('create_trigger'/],
];
for (const [file, pattern] of forbidden) {
  if (pattern.test(read(file))) fail(`${file} carries a backend trigger path again (${pattern})`);
}

// The dead Rust side emitted these three and nothing listened. If one comes
// back it needs a listener, not another silent event.
for (const event of ['trigger-notification', 'trigger-command', 'trigger-ai-assist']) {
  const emitted = read('src-tauri/src/commands.rs').includes(event);
  const heard = app.includes(event) || space.includes(event);
  if (emitted && !heard) fail(`${event} is emitted again with nobody listening`);
}

console.log('PASS triggers-smoke (one path: app.js checkTriggers, fed by the terminal and Agent Space)');
