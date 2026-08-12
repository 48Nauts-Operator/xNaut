// Slice output ports (XNAUT-128) — the JS half of the contract.
//
// project-management-panel.js cannot be require()d (it builds DOM at load), so
// the normaliser is lifted out of the real source by name rather than copied.
// A copy would pass forever after the original changed.
const { readFileSync } = require('node:fs');
const src = readFileSync(`${__dirname}/../src/js/project-management-panel.js`, 'utf8');

const m = src.match(/const outputPorts = \(v\) =>[\s\S]*?\.filter\(\(p\) => p\.id\);/);
if (!m) throw new Error('outputPorts not found in project-management-panel.js — did it get renamed?');
const outputPorts = eval(`(${m[0].replace(/^const outputPorts = /, '').replace(/;$/, '')})`);

const eq = (got, want, label) => {
  const a = JSON.stringify(got); const b = JSON.stringify(want);
  if (a !== b) throw new Error(`${label}\n  got  ${a}\n  want ${b}`);
};

// A model writes these, so every shape it plausibly emits has to land somewhere
// sane. Anything that is not a usable port name is dropped, never turned into a
// gate the slice can never pass.
eq(outputPorts(undefined), [], 'no outputs declared is not an error');
eq(outputPorts([]), [], 'an empty list stays empty');
eq(outputPorts('migration'), [], 'a bare string instead of a list is not a port');
eq(outputPorts(['migration']), [{ id: 'migration', data_type: 'any' }],
  'a plain string port defaults to any');
eq(outputPorts([{ name: 'schema', data_type: 'object' }]), [{ id: 'schema', data_type: 'object' }],
  'name is accepted as an alias for id, matching the plan format');
eq(outputPorts([{ id: 'endpoints', data_type: 'array' }]), [{ id: 'endpoints', data_type: 'array' }],
  'id passes through');
eq(outputPorts([{ id: '  spaced  ' }]), [{ id: 'spaced', data_type: 'any' }],
  'a padded name is trimmed and gets the default type');
eq(outputPorts([{ data_type: 'object' }, null, { id: '' }, { name: '   ' }]), [],
  'a port with no usable name is dropped rather than becoming an unpassable gate');
eq(outputPorts([{ id: 'a' }, 'b', { name: 'c', data_type: 'string' }]),
  [{ id: 'a', data_type: 'any' }, { id: 'b', data_type: 'any' }, { id: 'c', data_type: 'string' }],
  'mixed shapes in one list all normalise');

console.log('slice ports: 9 checks passed');
