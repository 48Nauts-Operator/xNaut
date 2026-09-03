#!/usr/bin/env node
// The Evidence tab's view model. Run: node scripts/evidence-view-check.cjs
//
// None of this throws when it is wrong. A verdict function that says
// "verified" over an empty report renders a green tick and a confident
// sentence, and looks healthier than the truth; that is the exact bug the
// work report shipped and had to have fixed. Only an assertion catches it.
const assert = require('node:assert');

globalThis.window = {};
globalThis.document = { getElementById: () => ({}), head: { appendChild() {} } };
require('../src/js/delivery-panel.js');
const { dayOf, dayLabel, groupByDay, integrityVerdict, recordLine, clockOf, clockRange } =
  window.xnautDeliveryInternals;

// ── The verdict may never claim more than the report supports ──────────────

// No report at all: the panel has not asked yet.
const none = integrityVerdict(null);
assert.equal(none.tone, 'unknown');
assert.deepEqual(none.detail, []);

// File absent. Not verified, and it names no checks.
const absent = integrityVerdict({
  path: '/x/execution.jsonl', exists: false, records: 0, sessions: 0, ok: false, broken: '', checked: [],
});
assert.equal(absent.tone, 'unknown');
assert.ok(!/verified|intact/i.test(absent.headline), `claimed integrity over nothing: ${absent.headline}`);
assert.equal(absent.path, '/x/execution.jsonl', 'the empty state must name the file');
assert.ok(absent.detail.join(' ').length > 0, 'an empty state must say WHY it is empty');

// File present, zero records. Still not verified, and a different sentence
// from "no file": one means the recorder never ran, the other means it ran and
// wrote nothing, and a reader chases a different bug for each.
const empty = integrityVerdict({
  path: '/x/execution.jsonl', exists: true, records: 0, sessions: 0, ok: false, broken: '', checked: [],
});
assert.equal(empty.tone, 'unknown');
assert.ok(!/verified|intact/i.test(empty.headline), `claimed integrity over an empty file: ${empty.headline}`);
assert.notEqual(empty.headline, absent.headline, 'empty and absent must not read the same');

// Broken. Red, and it says where.
const broken = integrityVerdict({
  path: '/x/execution.jsonl', exists: true, records: 41, sessions: 3, ok: false,
  broken: 'line 42: record does not hash to its own hash', checked: [],
});
assert.equal(broken.tone, 'failed');
assert.ok(broken.detail.some((d) => d.includes('line 42')), 'a break must name its line');
assert.ok(broken.detail.some((d) => d.includes('41')), 'say how much verified before the break');

// Only a real ok goes green, and it carries the checks with it.
const good = integrityVerdict({
  path: '/x/execution.jsonl', exists: true, records: 648, sessions: 26, ok: true, broken: '',
  checked: ['648 records re-hashed', '648 prev_hash links followed'],
});
assert.equal(good.tone, 'passed');
assert.ok(good.headline.includes('648') && good.headline.includes('26'));
assert.equal(good.detail.length, 2, 'a tick must carry what was checked');

// The verdict cannot manufacture ok: `ok` comes from evidence.rs walking the
// chain, and a report that says false with checks attached is still not green.
const lying = integrityVerdict({
  path: '/x', exists: true, records: 648, sessions: 26, ok: false, broken: '',
  checked: ['648 records re-hashed'],
});
assert.notEqual(lying.tone, 'passed', 'tone must follow ok, never the checked list');

// ── A row shows the command, never the hash ────────────────────────────────

assert.equal(
  recordLine({ summary: 'cargo tauri build --release', args_hash: 'sha256:abc', tool: 'Bash' }),
  'cargo tauri build --release',
);
// No summary but a stated reason: show the reason, not an empty row.
const gone = recordLine({ summary: '', args_error: 'session was shredded', tool: 'Bash' });
assert.ok(gone.includes('shredded'), gone);
assert.ok(!gone.startsWith('sha256:'), 'never fall back to the hash');
// A model call has no tool arguments; its model is the line.
assert.equal(recordLine({ summary: '', args_error: '', model: 'claude-opus-5', kind: 'model_call' }), 'claude-opus-5');
assert.equal(recordLine({ summary: '', args_error: '', model: '', tool: 'Bash', kind: 'tool_call' }), 'Bash');

// ── Days ───────────────────────────────────────────────────────────────────

// Local day, not the UTC prefix: 2026-08-31T23:30Z is already September in
// CEST, and filing it under August is how "today" ends up empty.
const local = new Date(2026, 7, 31, 23, 30, 0);
assert.equal(dayOf(local.toISOString()), '2026-08-31');
assert.equal(dayOf('not a date'), '');

const now = new Date(2026, 8, 3, 12, 0, 0).getTime();
assert.equal(dayLabel('2026-09-03', now), 'Today');
assert.equal(dayLabel('2026-09-02', now), 'Yesterday');
assert.ok(!['Today', 'Yesterday'].includes(dayLabel('2026-08-29', now)), 'older days get a real date');
assert.equal(dayLabel('', now), 'Undated');

// Newest day first, and every session lands in exactly one bucket.
const sessions = [
  { session_id: 'a', last_at: new Date(2026, 7, 29, 10, 0).toISOString() },
  { session_id: 'b', last_at: new Date(2026, 7, 31, 10, 0).toISOString() },
  { session_id: 'c', last_at: new Date(2026, 7, 29, 18, 0).toISOString() },
];
const groups = groupByDay(sessions);
assert.deepEqual(groups.map((g) => g[0]), ['2026-08-31', '2026-08-29']);
assert.equal(groups[1][1].length, 2);
assert.equal(groups.reduce((n, g) => n + g[1].length, 0), sessions.length, 'no session may be dropped');
assert.deepEqual(groupByDay([]), []);
assert.deepEqual(groupByDay(undefined), []);

// ── Clocks ─────────────────────────────────────────────────────────────────

assert.equal(clockOf(new Date(2026, 7, 31, 9, 4).toISOString()), '09:04');
assert.equal(clockOf('nope'), '--:--');
assert.equal(clockRange(new Date(2026, 7, 31, 9, 4).toISOString(), new Date(2026, 7, 31, 11, 12).toISOString()), '09:04-11:12');
assert.equal(clockRange('nope', 'nope'), '');
// A session that ran past midnight. The real log has one: nautbot opened on
// 27 August and was still recording on the 31st, which rendered as
// "23:47-13:46" and read as ten hours backwards.
const spanning = clockRange(new Date(2026, 7, 27, 23, 47).toISOString(), new Date(2026, 7, 31, 13, 46).toISOString());
assert.notEqual(spanning, '23:47-13:46', 'a multi-day span must not look like one backwards day');
assert.ok(spanning.includes('23:47') && spanning.includes('13:46'), spanning);
assert.ok(/Aug/i.test(spanning), `the start must carry its date: ${spanning}`);

console.log('evidence view model ok');
