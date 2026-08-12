// NautFlow tracks (XNAUT-17) — a feature is not a business case.
//
// Same lifting trick as slice-ports-smoke.cjs: the tables and the selector are
// pulled out of the real source rather than copied, so this cannot keep passing
// after they change.
const { readFileSync } = require('node:fs');
const src = readFileSync(`${__dirname}/../src/js/project-management-panel.js`, 'utf8');

const lift = (name, re) => {
  const m = src.match(re);
  if (!m) throw new Error(`${name} not found in project-management-panel.js — did it get renamed?`);
  return m[0];
};
const tables = ['STANDARD_STAGES', 'INCIDENT_STAGES', 'FEATURE_STAGES']
  .map((n) => lift(n, new RegExp(`const ${n} = \\[[\\s\\S]*?\\n {2}\\];`)))
  .join('\n');
const stagesFor = lift('stagesFor', /function stagesFor\(project\) \{[\s\S]*?\n {4}\}/);
const flowTypes = lift('FLOW_TYPES', /const FLOW_TYPES = \[[\s\S]*?\n {2}\];/);
const flowLabel = lift('FLOW_LABEL', /const FLOW_LABEL = \{.*?\};/);
// eslint-disable-next-line no-eval
const ctx = eval(`(() => { ${tables}\n${flowTypes}\n${flowLabel}\n${stagesFor}\n return { STANDARD_STAGES, INCIDENT_STAGES, FEATURE_STAGES, FLOW_TYPES, FLOW_LABEL, stagesFor }; })()`);

const keys = (rows) => rows.map((r) => r[0]);
const ok = (cond, label) => { if (!cond) throw new Error(label); };

// The selector is the whole contract: an unknown or missing flow type must land
// on the standard track, never on an empty one.
ok(ctx.stagesFor({ flow_type: 'incident' }) === ctx.INCIDENT_STAGES, 'incident picks the incident track');
ok(ctx.stagesFor({ flow_type: 'feature' }) === ctx.FEATURE_STAGES, 'feature picks the feature track');
ok(ctx.stagesFor({ flow_type: 'standard' }) === ctx.STANDARD_STAGES, 'standard picks the standard track');
ok(ctx.stagesFor({}) === ctx.STANDARD_STAGES, 'a project with no flow type falls back to standard');
ok(ctx.stagesFor({ flow_type: 'nonsense' }) === ctx.STANDARD_STAGES, 'an unknown flow type falls back to standard');

// A feature reuses the standard stage keys — the documents, personas and Rust
// stage validation are all shared. A key only the feature track knows would be
// rejected by pm_project_update's allow-list.
const std = keys(ctx.STANDARD_STAGES);
const foreign = keys(ctx.FEATURE_STAGES).filter((k) => !std.includes(k));
ok(!foreign.length, `feature stages must exist on the standard track, found ${foreign.join(', ')}`);

// It is a trimmed track, not a copy, and it drops exactly what a feature
// already has answers for.
ok(ctx.FEATURE_STAGES.length < ctx.STANDARD_STAGES.length, 'the feature track is shorter than standard');
const dropped = std.filter((k) => !keys(ctx.FEATURE_STAGES).includes(k));
ok(JSON.stringify(dropped) === JSON.stringify(['concept', 'business_case', 'data_model', 'sprint_stories']),
  `unexpected drops: ${dropped.join(', ')}`);

// Every track has to start where the Rust side starts it: "idea" for
// standard/feature (default_project_stage), "intake" for incident.
ok(ctx.FEATURE_STAGES[0][0] === 'idea', 'the feature track starts at idea, matching default_project_stage()');
ok(ctx.INCIDENT_STAGES[0][0] === 'intake', 'the incident track starts at intake');

// Every stage row is [key, phase, label, persona] — a short row renders as
// undefined in the rail rather than failing.
for (const [name, rows] of [['standard', ctx.STANDARD_STAGES], ['incident', ctx.INCIDENT_STAGES], ['feature', ctx.FEATURE_STAGES]]) {
  ok(rows.every((r) => r.length === 4 && r.every((c) => typeof c === 'string' && c)),
    `every ${name} stage row needs key, phase, label and persona`);
  ok(new Set(keys(rows)).size === rows.length, `${name} stage keys must be unique`);
}

// The picker and the tracks must not drift: every offered flow type has to have
// a track and a label, or the UI offers a choice that renders as standard.
for (const [value, label, blurb] of ctx.FLOW_TYPES) {
  ok(label && blurb, `flow type ${value} needs a label and a blurb`);
  ok(ctx.FLOW_LABEL[value], `flow type ${value} has no short label`);
}
ok(ctx.FLOW_TYPES.some(([v]) => v === 'feature'), 'feature is offered when creating a project');
ok(Object.keys(ctx.FLOW_LABEL).length === ctx.FLOW_TYPES.length, 'FLOW_LABEL and FLOW_TYPES cover the same set');

// The Rust allow-list is the other half of this contract.
const rust = readFileSync(`${__dirname}/../src-tauri/src/project_management.rs`, 'utf8');
for (const [value] of ctx.FLOW_TYPES) {
  ok(rust.includes(`"${value}"`), `flow type ${value} is not accepted by project_management.rs`);
}

console.log('flow tracks: 17 checks passed');
