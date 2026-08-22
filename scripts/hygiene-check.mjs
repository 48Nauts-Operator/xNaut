#!/usr/bin/env node
// Hygiene — the checks that catch what the suite cannot see about itself.
//
//   node scripts/hygiene-check.mjs
//
// preflight.mjs already runs cargo build/test/clippy, the ACL sweep, JS syntax
// and the live-service probes. This is the other half: three classes of defect
// that a green suite is compatible with, all three found in one session.
//
//   1. THE SUITE POLLUTES THE MACHINE IT RUNS ON.
//      shared_notes' linking test wrote a project folder into the owner's real
//      ~/.xnaut-vault and removed it on its last line, which a failed assertion
//      skips. 487 empty xnaut-notes-test-* folders had accumulated. Every run
//      was green the whole time — the damage was outside the assertions.
//
//   2. A FEATURE IS WIRED AT ONE END ONLY.
//      xnaut-brief.sh read XNAUT_BRIEF_URL; nothing injected it and nothing
//      served /v1/brief. The hook installed, fired on every session, and
//      printed nothing. Each half compiles and tests fine alone.
//
//   3. A KEY THAT CANNOT BE REOPENED.
//      The chat canvas was keyed by request_id, a fresh UUID per turn, so every
//      diagram landed in a file nobody could find. The agent reported success
//      truthfully and the owner saw an empty pane.
//
// Each check names the incident it descends from, because a check whose reason
// is forgotten is the first one deleted when it becomes inconvenient.
import { execSync } from 'node:child_process';
import { readFileSync, existsSync, readdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { homedir } from 'node:os';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const results = [];
const add = (name, status, detail = '') => {
  results.push({ name, status, detail });
  const mark = { pass: '✓', fail: '✗', warn: '!', skip: '–' }[status];
  const colour = { pass: '\x1b[32m', fail: '\x1b[31m', warn: '\x1b[33m', skip: '\x1b[90m' }[status];
  console.log(`${colour}${mark}\x1b[0m ${name}${detail ? ` — ${detail}` : ''}`);
};
const read = (rel) => { try { return readFileSync(join(ROOT, rel), 'utf8'); } catch { return ''; } };

// Whole-token match, not substring.
//
// The first version of this file used String.includes, and a mutation renaming
// XNAUT_BRIEF_URL to XNAUT_BRIEF_URL_OFF passed every check — the broken name
// CONTAINS the one being looked for. A check that survives the mutation it was
// written for is decoration; this is the difference between the two.
const hasToken = (body, token) => {
  const escaped = token.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  return new RegExp(`(?<![A-Za-z0-9_])${escaped}(?![A-Za-z0-9_])`).test(body);
};

// ---- 1. the suite must not write into the owner's vault -------------------
// Snapshot before, run the suite, compare. The only honest way to test this:
// asserting "no test writes to $HOME" by reading source misses the one that
// does it through three layers of helper.
function vaultPollution() {
  const vault = join(homedir(), '.xnaut-vault', 'work');
  if (!existsSync(vault)) { add('Vault pollution', 'skip', 'no vault on this machine'); return; }
  const before = new Set(readdirSync(vault));
  // The suite's own result does not matter here, and skipping on failure was
  // wrong: a FAILING test is exactly the case that leaks, because cleanup on
  // the last line never runs. Verified by mutation — reverting the notes_dir
  // fix made the suite fail AND leak, and the old code reported "skip".
  let suiteFailed = false;
  try {
    execSync('cargo test --manifest-path src-tauri/Cargo.toml', { cwd: ROOT, stdio: 'pipe' });
  } catch {
    suiteFailed = true;
  }
  const created = readdirSync(vault).filter((entry) => !before.has(entry));
  if (created.length) {
    add('Vault pollution', 'fail',
      `the suite created ${created.length} folder(s) in the real vault: ${created.slice(0, 5).join(', ')}` +
      (suiteFailed ? ' (suite also failed — cleanup on the last line never ran)' : ''));
    return;
  }
  if (suiteFailed) { add('Vault pollution', 'pass', 'suite failed but left the vault clean'); return; }
  add('Vault pollution', 'pass', 'suite left the vault untouched');
}

// ---- 2. features wired at both ends ---------------------------------------
// A chain is only real if every link exists. Listed as data so adding a chain
// is one entry, not a new function.
const CHAINS = [
  {
    name: 'SessionStart brief',
    incident: 'the hook installed, fired every session, and printed nothing',
    links: [
      ['script reads the url', 'src-tauri/scripts/hooks/xnaut-brief.sh', 'XNAUT_BRIEF_URL'],
      ['something injects it', 'src-tauri/src/agents.rs', 'XNAUT_BRIEF_URL'],
      ['something serves it', 'src-tauri/src/agent_hooks.rs', '"/v1/brief"'],
      ['the hook is installed', 'src-tauri/src/agent_hook_setup.rs', 'SessionStart'],
    ],
  },
  {
    name: 'Status hook',
    incident: 'XNAUT-183 — two consumers appended to a route, two replaced it',
    links: [
      ['script reads the url', 'src-tauri/scripts/hooks/xnaut-hook.sh', 'XNAUT_HOOK_URL'],
      ['something injects it', 'src-tauri/src/agents.rs', 'XNAUT_HOOK_URL'],
      ['something serves it', 'src-tauri/src/agent_hooks.rs', '"/v1/hook"'],
    ],
  },
  {
    name: 'PreToolUse veto',
    incident: 'XNAUT-132 — a hook pointing at a missing script fails on every call',
    links: [
      ['script reads the url', 'src-tauri/scripts/hooks/xnaut-veto.sh', 'XNAUT_VETO_URL'],
      ['something injects it', 'src-tauri/src/agents.rs', 'XNAUT_VETO_URL'],
      ['something serves it', 'src-tauri/src/agent_hooks.rs', '"/v1/veto"'],
    ],
  },
];

function wiring() {
  for (const chain of CHAINS) {
    const broken = chain.links.filter(([, file, needle]) => !hasToken(read(file), needle));
    if (broken.length === 0) add(`Wired: ${chain.name}`, 'pass', `${chain.links.length} links`);
    else add(`Wired: ${chain.name}`, 'fail',
      `${broken.map(([what]) => what).join('; ')} — ${chain.incident}`);
  }
}

// ---- 3. registered commands that nothing calls ----------------------------
// Not a failure: a command can legitimately land before its UI. But an
// unreported one silently becomes dead code, so it is named rather than
// counted.
function unusedCommands() {
  const main = read('src-tauri/src/main.rs');
  const block = main.split('generate_handler!')[1] || '';
  const cmds = [...block.matchAll(/(?:\w+::)?(\w+),/g)].map((m) => m[1]);
  const js = readdirSync(join(ROOT, 'src/js'))
    .filter((f) => f.endsWith('.js'))
    .map((f) => read(join('src/js', f)))
    .join('\n');
  const orphans = [...new Set(cmds)].filter((c) => !js.includes(`'${c}'`) && !js.includes(`"${c}"`));
  if (orphans.length === 0) add('Commands reachable from the UI', 'pass', `${cmds.length} commands`);
  else add('Commands reachable from the UI', 'warn',
    `${orphans.length} registered but never invoked: ${orphans.slice(0, 8).join(', ')}${orphans.length > 8 ? '…' : ''}`);
}

// ---- 4. canvas keys that can be reopened ----------------------------------
// A canvas under a UUID is a canvas nobody can navigate back to. Reports rather
// than fails: one may legitimately predate the fix.
function canvasKeys() {
  const dir = join(homedir(), 'Library/Application Support/xnaut/canvases');
  if (!existsSync(dir)) { add('Canvas keys reachable', 'skip', 'no canvases'); return; }
  const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\.json$/i;
  const orphans = readdirSync(dir).filter((f) => uuid.test(f) || /^req-\d+-\d+\.json$/.test(f));
  if (orphans.length === 0) add('Canvas keys reachable', 'pass', 'every canvas has a nameable key');
  else add('Canvas keys reachable', 'warn',
    `${orphans.length} keyed by a request id, unreachable from the UI: ${orphans[0]}`);
}

// ---- 5. hook scripts fail silent ------------------------------------------
// SessionStart stdout becomes the agent's context. A script that can print an
// error prints it INTO the session as though the project had said it.
function hooksFailSilent() {
  const bad = [];
  for (const f of ['xnaut-brief.sh', 'xnaut-hook.sh', 'xnaut-veto.sh']) {
    const body = read(join('src-tauri/scripts/hooks', f));
    if (!body) continue;
    if (!/exit 0/.test(body)) bad.push(`${f} has no unconditional exit 0`);
    if (/curl/.test(body) && !/-s|--silent/.test(body)) bad.push(`${f} curls without -s`);
  }
  if (bad.length === 0) add('Hook scripts fail silent', 'pass', 'short timeouts, always exit 0');
  else add('Hook scripts fail silent', 'fail', bad.join('; '));
}

// ---- 6. tests that are not tests -----------------------------------------
// A fn in a #[cfg(test)] mod without #[test] never runs, and nothing reports
// it: the suite counts what it was given. Found by cargo's dead_code warning
// after an edit removed the wrong one of two duplicated attributes, silently
// disabling an idempotency test that had been passing for months.
function disabledTests() {
  const files = readdirSync(join(ROOT, 'src-tauri/src')).filter((f) => f.endsWith('.rs'));
  const orphans = [];
  for (const file of files) {
    const body = read(join('src-tauri/src', file));
    const idx = body.indexOf('#[cfg(test)]');
    if (idx < 0) continue;
    const block = body.slice(idx);
    // A test fn takes no arguments and returns nothing; helpers take args.
    const re = /(^|\n)(\s*)fn (\w+)\(\)\s*\{/g;
    let m;
    while ((m = re.exec(block))) {
      const before = block.slice(Math.max(0, m.index - 220), m.index);
      if (!/#\[(test|tokio::test|rstest)/.test(before)) orphans.push(`${file}::${m[3]}`);
    }
  }
  if (orphans.length === 0) add('Every test fn is attributed', 'pass', `${files.length} modules`);
  else add('Every test fn is attributed', 'fail',
    `${orphans.length} fn(s) in a test module never run: ${orphans.slice(0, 5).join(', ')}`);
}

console.log('\nxNAUT hygiene — what a green suite cannot see about itself\n');
vaultPollution();
wiring();
hooksFailSilent();
unusedCommands();
canvasKeys();
disabledTests();

const failed = results.filter((r) => r.status === 'fail').length;
const warned = results.filter((r) => r.status === 'warn').length;
console.log(`\n${results.length} checks · ${failed} failed · ${warned} warnings\n`);
process.exit(failed ? 1 : 0);
