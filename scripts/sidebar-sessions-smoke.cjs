// A project row must only offer sessions that are actually running.
//
// zellij keeps exited sessions listed ("attach to resurrect"), so the backend
// reports them with exited: true. The sidebar used to match on name alone, and
// a NautGate session killed in zellij kept sitting in the left pane offering to
// be opened (2026-08-18).

const { readFileSync } = require('node:fs');
const { join } = require('node:path');

const src = readFileSync(join(__dirname, '..', 'src/js/sidebar.js'), 'utf8');
const fail = (m) => { console.error(`FAIL ${m}`); process.exit(1); };

// Lift the real filter body out of the file so this tests the shipped code
// rather than a copy that can drift away from it.
const m = /function sessionsFor\(task\) \{([\s\S]*?)\n    \}/.exec(src);
if (!m) fail('sessionsFor not found in sidebar.js — did it get renamed?');
const sessionsFor = new Function('task', 'state', `${m[1].replace(/state\./g, 'state.')}`);

const state = {
  sessions: [
    { name: 'cx-NautGate', exited: true },
    { name: 'cl-NautGate', exited: false },
    { name: 'cx-Bucky', exited: false },
  ],
};

const got = sessionsFor({ name: 'NautGate' }, state).map((s) => s.name);
if (got.includes('cx-NautGate')) fail('an EXITED session is still offered in the sidebar');
if (!got.includes('cl-NautGate')) fail('a live session for the project was dropped');
if (got.includes('cx-Bucky')) fail('another project\'s session leaked into the row');

console.log(`PASS sidebar-sessions-smoke (offered: ${got.join(', ') || 'none'})`);
