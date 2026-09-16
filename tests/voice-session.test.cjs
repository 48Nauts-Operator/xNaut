const { test } = require('node:test');
const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const { join } = require('node:path');
const { runInNewContext } = require('node:vm');
const { webcrypto } = require('node:crypto');

function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
function setup(overrides = {}) {
  const window = {};
  runInNewContext(readFileSync(join(__dirname, '../src/js/voice-session.js'), 'utf8'), {
    window, crypto: webcrypto, AbortController, TextEncoder,
  });
  const spoken = [], inserted = [], cancelled = [];
  const provider = {
    capabilities: { recognition: true, synthesis: true },
    start: async () => {}, stop: async () => ({ text: 'final transcript' }),
    cancel: async (id) => cancelled.push(id),
    speak: async (text) => { spoken.push(text); },
    ...overrides,
  };
  const coordinator = window.xnautVoiceSession;
  const begin = (options = {}) => coordinator.begin({
    destination: { id: 'chat-1', label: 'Chat 1' }, provider,
    onTranscript: (text) => inserted.push(text), ...options,
  });
  return { coordinator, begin, provider, spoken, inserted, cancelled };
}
const tick = () => new Promise((resolve) => setImmediate(resolve));

test('only one pinned destination owns capture, including during startup', async () => {
  const start = deferred();
  const f = setup({ start: () => start.promise });
  const beginning = f.begin();
  await assert.rejects(f.begin({ destination: { id: 'other', label: 'Other' } }), /End the current/);
  assert.equal(f.coordinator.snapshot().destination, 'Chat 1');
  start.resolve(); await beginning;
  await Promise.all([f.coordinator.finishInput(), f.coordinator.finishInput()]);
  assert.deepEqual(f.inserted, ['final transcript']);
  await f.coordinator.end();
});

test('cancel during initialization waits for the old provider before restarting', async () => {
  const start = deferred();
  let starts = 0;
  const f = setup({ start: async () => { if (++starts === 1) await start.promise; } });
  const first = f.begin(); await tick();
  const ending = f.coordinator.end();
  const second = f.begin(); await tick();
  assert.equal(starts, 1);
  start.resolve();
  await Promise.all([first, ending, second]);
  assert.equal(starts, 2);
  assert.equal(f.cancelled.length, 1);
  assert.equal(f.coordinator.snapshot().state, 'recording');
});

test('late transcripts cannot enter a new session or a removed composer', async () => {
  const transcription = deferred();
  const f = setup({ stop: () => transcription.promise });
  await f.begin();
  const finishing = f.coordinator.finishInput();
  await f.coordinator.end(); await f.begin();
  transcription.resolve({ text: 'old private utterance' }); await finishing;
  assert.deepEqual(f.inserted, []);
  await f.coordinator.end();
  await f.begin({ destination: { id: 'gone', label: 'Gone', connected: () => false } });
  await f.coordinator.finishInput();
  assert.deepEqual(f.inserted, []);
  assert.equal(f.coordinator.snapshot().state, 'idle');
});

test('Full accepts only explicit assistant answers; summaries and tools stay out', async () => {
  const f = setup(); await f.begin({ mode: 'full' });
  const scope = f.coordinator.beginTurn('turn-1');
  const offer = (id, kind, text) => f.coordinator.offer(scope, { id, kind, text });
  assert.equal(offer('a', 'tool', 'delete file'), 'filtered');
  assert.equal(offer('b', 'reasoning', 'private reasoning'), 'filtered');
  assert.equal(offer('c', 'summary', 'short answer'), 'filtered');
  assert.equal(offer('d', 'answer', 'The full answer.'), 'queued');
  assert.equal(offer('d', 'answer', 'The full answer.'), 'duplicate');
  await tick(); assert.deepEqual(f.spoken, ['The full answer.']);
});

test('Summary needs agent capability and bounded agent text; never invokes a summarizer', async () => {
  const f = setup(); await f.begin({ mode: 'summary' });
  let scope = f.coordinator.beginTurn('no-summary');
  assert.match(f.coordinator.snapshot().error, /does not supply/);
  assert.equal(f.coordinator.offer(scope, { id: 'a', kind: 'summary', text: 'guess' }), 'unsupported');
  scope = f.coordinator.beginTurn('supported', { summary: true });
  assert.equal(f.coordinator.offer(scope, { id: 'b', kind: 'summary', text: 'word '.repeat(81) }), 'invalid');
  assert.equal(f.coordinator.offer(scope, { id: 'c', kind: 'summary', text: '界'.repeat(350) }), 'invalid');
  assert.equal(f.coordinator.offer(scope, { id: 'd', kind: 'summary', text: 'The agent’s short answer.' }), 'queued');
  await tick(); assert.deepEqual(f.spoken, ['The agent’s short answer.']);
});

test('Silent aborts playback without ending input; old generations cannot resume', async () => {
  const playing = deferred(); let signal;
  const f = setup({ speak: (_text, abort) => { signal = abort; return playing.promise; } });
  await f.begin({ mode: 'full' });
  const scope = f.coordinator.beginTurn('turn-1');
  f.coordinator.offer(scope, { id: 'a', kind: 'answer', text: 'first' });
  f.coordinator.offer(scope, { id: 'b', kind: 'answer', text: 'queued' });
  f.coordinator.setMode('silent');
  assert.equal(signal.aborted, true);
  assert.equal(f.coordinator.snapshot().state, 'recording');
  assert.equal(f.coordinator.snapshot().queued, 0);
  assert.equal(f.coordinator.offer(scope, { id: 'c', kind: 'answer', text: 'late' }), 'stale');
  playing.resolve(); await tick();
  assert.equal(f.coordinator.snapshot().speaking, false);
  await f.coordinator.finishInput(); assert.deepEqual(f.inserted, ['final transcript']);
});

test('old turns and old sessions cannot speak to a new destination', async () => {
  const f = setup(); await f.begin({ mode: 'full' });
  const oldTurn = f.coordinator.beginTurn('first');
  f.coordinator.beginTurn('second');
  assert.equal(f.coordinator.offer(oldTurn, { id: 'a', kind: 'answer', text: 'late' }), 'stale');
  const oldSession = f.coordinator.scope();
  await f.coordinator.end(); await f.begin({ mode: 'full' });
  f.coordinator.beginTurn('second');
  assert.equal(f.coordinator.offer(oldSession, { id: 'b', kind: 'answer', text: 'late' }), 'stale');
  assert.deepEqual(f.spoken, []);
});

test('backpressure is bounded and visible, with no automatic resume', async () => {
  const playing = deferred();
  const f = setup({ speak: () => playing.promise });
  await f.begin({ mode: 'full' });
  const scope = f.coordinator.beginTurn('fast-agent');
  assert.equal(f.coordinator.offer(scope, { id: '0', kind: 'answer', text: 'playing' }), 'queued');
  assert.equal(f.coordinator.offer(scope, { id: '1', kind: 'answer', text: 'x'.repeat(32000) }), 'queued');
  assert.equal(f.coordinator.offer(scope, { id: '2', kind: 'answer', text: 'x'.repeat(1000) }), 'overflow');
  assert.match(f.coordinator.snapshot().error, /queue limit/);
  assert.equal(f.coordinator.snapshot().queued, 0);
  assert.equal(f.coordinator.offer(scope, { id: '3', kind: 'answer', text: 'more' }), 'stale');
  playing.resolve(); await tick();
});

test('provider failures preserve text and never fall back to another provider', async () => {
  const f = setup({ speak: async () => { throw new Error('service offline'); } });
  await f.begin({ mode: 'full' });
  const scope = f.coordinator.beginTurn('turn');
  f.coordinator.offer(scope, { id: 'a', kind: 'answer', text: 'answer' });
  await tick(); assert.match(f.coordinator.snapshot().error, /service offline/);
  await f.coordinator.finishInput(); assert.deepEqual(f.inserted, ['final transcript']);
});

test('dictation-only providers reject playback modes instead of pretending to speak', async () => {
  const f = setup({ capabilities: { synthesis: false } });
  await assert.rejects(f.begin({ mode: 'full' }), /does not support/);
  await f.begin();
  assert.throws(() => f.coordinator.setMode('summary'), /does not support/);
});

test('a delayed finish handler cannot end a newer ready session', async () => {
  const f = setup(); await f.begin();
  const epoch = await f.coordinator.finishInput();
  await f.coordinator.end(); await f.begin();
  await f.coordinator.finishInput();
  await f.coordinator.end(epoch);
  assert.equal(f.coordinator.snapshot().state, 'ready');
  assert.notEqual(f.coordinator.snapshot().epoch, epoch);
});

test('a failed cancellation is visible instead of claiming the mic was released', async () => {
  const f = setup({ cancel: async () => { throw new Error('device not responding'); } });
  await f.begin(); await f.coordinator.end();
  assert.equal(f.coordinator.snapshot().state, 'error');
  assert.match(f.coordinator.snapshot().error, /Could not release.*device not responding/);
});
