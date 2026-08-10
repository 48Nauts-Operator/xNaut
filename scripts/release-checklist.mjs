// Build the release-test checklist from what the app actually renders.
//
// A hand-written checklist goes stale the day someone adds a button and nobody
// remembers to add the line. This reads tests/control-inventory.json (produced
// by tests/enumerate-controls.mjs against the real app) and prints every unique
// interactive control, grouped by the surface it first appears on, with the ones
// the GUI smoke test already presses ticked off.
//
// The number that matters is the last line: covered / total. Anything the app
// renders and the smoke test does not press is untested, never passed.
//
//   node tests/static-server.mjs & node tests/enumerate-controls.mjs   # refresh
//   node scripts/release-checklist.mjs > docs/release-checklist.md
//
// ponytail: reads the inventory as-is. If a surface is missing from it, fix the
// enumerator, not this file.

import { readFile } from 'node:fs/promises';

const INVENTORY = new URL('../tests/control-inventory.json', import.meta.url);

// Kept in sync by hand with the loop in scripts/gui-smoke.sh. Deliberately not
// parsed out of the shell: a regex over bash would fail silently the first time
// someone reformats that list, and silently-wrong coverage is worse than none.
// The Settings sections are listed by aria-label, not by their visible text,
// because that is the name axui presses and the name this generator records.
const SMOKED = new Set([
  'Toggle projects sidebar', 'Toggle project pane', 'Command snippets',
  'Open new browser tab', 'Open new markdown tab', 'Open new diff tab',
  'Open Projects (tasks & plan)', 'Open worktree manager', 'More actions',
  'Help and keyboard shortcuts', 'Refresh usage',
  'Settings', 'AI settings', 'Tasks Mode settings', 'Appearance settings',
  'Keyboard Shortcuts settings', 'Mobile settings', 'Nautify settings',
  'Triggers settings',
]);

const inv = JSON.parse(await readFile(INVENTORY, 'utf8'));

// The enumerator's selector deliberately over-collects (it includes [class*=tab]
// and [data-*]) so that nothing is missed. That sweeps in containers: the tab bar
// itself arrives as one "control" labelled with every tab title concatenated.
// A checklist line has to be something a person can press, so keep only the tags
// that actually take a click.
// Tag alone is not enough: the More actions menu items are divs, and so is the
// tab bar. Role separates them.
const ACTIONABLE = new Set(['button', 'a', 'input', 'select', 'textarea']);
const ACTIONABLE_ROLE = new Set(['button', 'menuitem', 'tab', 'link', 'checkbox', 'radio']);
const actionable = (c) => ACTIONABLE.has(c.tag) || ACTIONABLE_ROLE.has(c.role || '');

// An unlabelled control cannot be pressed by identity and cannot be written down
// as a checklist line, so name it by tag and class instead of dropping it.
const name = (c) =>
  c.label || `[${c.tag}${c.cls ? ` .${c.cls.split(' ')[0]}` : ''}] (unlabelled)`;

const seen = new Set();
const groups = [];
let errored = 0;

for (const [surface, controls] of Object.entries(inv.surfaces)) {
  // Two surfaces are {error: "..."} rather than a list: the enumerator could not
  // click them. Those are unenumerated, which is a gap in the checklist and must
  // be said out loud rather than counted as zero controls.
  if (!Array.isArray(controls)) { groups.push({ surface, error: controls.error }); errored++; continue; }
  const fresh = [];
  for (const c of controls) {
    if (!c.visible || !actionable(c)) continue;
    const n = name(c);
    if (seen.has(n)) continue;
    seen.add(n);
    fresh.push(n);
  }
  if (fresh.length) groups.push({ surface, controls: fresh.sort() });
}

const covered = [...seen].filter((n) => SMOKED.has(n)).length;

const out = [];
out.push('# xNAUT release checklist');
out.push('');
out.push(`Generated from \`tests/control-inventory.json\` (captured ${inv.at} against \`${inv.base}\`).`);
out.push('Do not edit by hand. Re-run `node scripts/release-checklist.mjs` after the enumerator.');
out.push('');
out.push('`[x]` means the GUI smoke test presses it on every run. `[ ]` means nobody has.');
out.push('');
out.push('**Depth:** the enumerator opens each top-bar surface, then each *More actions*');
out.push('destination, then each Settings section, and records what every one of those');
out.push('reveals. That is as deep as the app goes. A control listed here is one the app');
out.push('really renders; an unticked one is one nobody presses.');
out.push('');

for (const g of groups) {
  out.push(`## ${g.surface}`);
  out.push('');
  if (g.error) {
    out.push(`> NOT ENUMERATED: ${g.error.split('\n')[0]}`);
    out.push('> Everything behind this surface is untested.');
  } else {
    for (const c of g.controls) out.push(`- [${SMOKED.has(c) ? 'x' : ' '}] ${c}`);
  }
  out.push('');
}

out.push('---');
out.push('');
out.push(`**${covered} of ${seen.size} controls covered by the GUI smoke test.**`);
if (errored) out.push(`**${errored} surface(s) could not be enumerated at all.**`);

console.log(out.join('\n'));

// Names in SMOKED that the app no longer renders mean the smoke test is pressing
// something that moved or was renamed, and its coverage claim is a lie. Fail
// loudly rather than quietly reporting a lower number.
const ghosts = [...SMOKED].filter((n) => !seen.has(n));
if (ghosts.length) {
  console.error(`\nrelease-checklist: smoke test targets no longer in the app: ${ghosts.join(', ')}`);
  process.exit(1);
}
