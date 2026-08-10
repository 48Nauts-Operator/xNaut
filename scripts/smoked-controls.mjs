// The controls scripts/gui-smoke.sh presses, by accessible name.
//
// Kept in sync by hand with the loop in that script. Deliberately not parsed out
// of the shell: a regex over bash would fail silently the first time someone
// reformats that list, and silently-wrong coverage is worse than none.
//
// This lives in its own module because two things need it and neither should
// import the other: release-checklist.mjs generates the checklist (and does work
// at import time), and tests/named-controls-unique.spec.mjs guards every name
// here against being made ambiguous by a control added later.
//
// The Settings sections are listed by aria-label, not by their visible text,
// because that is the name axui presses and the name the enumerator records.

export const SMOKED = [
  'Toggle projects sidebar', 'Toggle project pane', 'Command snippets',
  'Open new browser tab', 'Open new markdown tab', 'Open new diff tab',
  'Open Projects (tasks & plan)', 'Open worktree manager', 'More actions',
  'Help and keyboard shortcuts', 'Refresh usage',
  'xNAUT settings', 'AI settings', 'Tasks Mode settings', 'Appearance settings',
  'Keyboard Shortcuts settings', 'Mobile settings', 'Nautify settings',
  'Triggers settings',
];
