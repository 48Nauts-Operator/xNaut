#!/usr/bin/env node
// One check for the Delivery report's joins. A wrong grouping here does not
// throw; it silently files every commit under "other" and the dashboard still
// renders, which is why this exists. Run: node scripts/delivery-report-check.cjs
const assert = require('node:assert');

globalThis.window = {};
globalThis.document = { getElementById: () => ({}), head: { appendChild() {} } };
require('../src/js/delivery-panel.js');
const { typeOf, isTestFile, duration } = window.xnautDeliveryInternals;

assert.equal(typeOf('feat(scripts): proof of work (XNAUT-207)'), 'feat');
assert.equal(typeOf('fix: surface NautGate routing receipts'), 'fix');
assert.equal(typeOf('chore(release): 1.18.1'), 'chore');
assert.equal(typeOf('feat!: breaking'), 'feat');
assert.equal(typeOf('Merge branch main'), 'other');
assert.equal(typeOf('wip: something'), 'other');      // not a conventional type
assert.equal(typeOf(''), 'other');

assert.ok(isTestFile('src-tauri/tests/pty.rs'));
assert.ok(isTestFile('src/js/foo.test.js'));
assert.ok(isTestFile('pkg/thing_test.go'));
assert.ok(!isTestFile('src/js/latest.js'));           // "test" inside a word is not a test
assert.ok(!isTestFile('src-tauri/src/protest.rs'));

assert.equal(duration('2026-08-19T10:00:00Z', '2026-08-19T10:00:42Z'), '42s');
assert.equal(duration('2026-08-19T10:00:00Z', '2026-08-19T10:03:05Z'), '3m 5s');
assert.equal(duration('2026-08-19T10:00:00Z', 'not a date'), '');

console.log('delivery report joins ok');
