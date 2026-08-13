global.window = global;
global.document = { readyState: 'complete', getElementById: () => null };
global.CustomEvent = class CustomEvent {};

require('../src/js/chat-panel.js');

const compile = global.xnautCompileAgentLoop;
const automatic = compile({
  action: 'loop_create',
  name: 'Incident Sweep',
  nodes: [
    { id: 'start', kind: 'trigger', name: 'Start', next: 'review' },
    { id: 'review', kind: 'agent', name: 'Review incident', next: 'gate' },
    { id: 'gate', kind: 'decision', name: 'More incidents?', branches: { yes: 'review', no: 'done' } },
    { id: 'done', kind: 'output', name: 'Done' },
  ],
});
const inserted = automatic.nodes.filter((node) => node.kind === 'retry');
if (inserted.length !== 1 || inserted[0].max_retries !== 10) {
  throw new Error('unbounded cycle was not repaired');
}

const explicit = compile({
  action: 'loop_create',
  name: 'Explicit Retry',
  nodes: [
    { id: 'start', kind: 'trigger', name: 'Start', next: 'work' },
    { id: 'work', kind: 'agent', name: 'Work', next: 'retry' },
    { id: 'retry', kind: 'retry', name: 'Retry', next: 'work' },
  ],
});
if (explicit.nodes.filter((node) => node.kind === 'retry').length !== 1) {
  throw new Error('explicit Retry node was duplicated');
}

// A model writing a loop reuses a name freely. Two nodes landing on one id makes
// the second silently overwrite the first's edges, so ids have to stay distinct.
const collided = compile({
  action: 'loop_create',
  name: 'Same Name Twice',
  nodes: [
    { id: 'start', kind: 'trigger', name: 'Start', next: 'work' },
    { id: 'work', kind: 'agent', name: 'Work' },
    { id: 'work', kind: 'agent', name: 'Work' },
  ],
});
const ids = collided.nodes.map((node) => node.id);
if (new Set(ids).size !== ids.length) {
  throw new Error(`duplicate node ids survived compilation: ${ids.join(', ')}`);
}

