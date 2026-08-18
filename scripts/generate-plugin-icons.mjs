// Regenerates src/js/plugin-icons.js from simple-icons.
//
//   node scripts/generate-plugin-icons.mjs
//
// Brand marks come from simple-icons (CC0-1.0, https://simpleicons.org). They
// are baked into a static file rather than fetched: the app is offline-first,
// and a logo that needs the network is a logo that is missing exactly when
// someone is on a train.
//
// Add a plugin to BRANDS when simple-icons carries its mark. Everything else
// falls back to a monogram tile — including Slack, which simple-icons dropped
// for brand-policy reasons, and the servers that have no logo at all.
import { createRequire } from 'node:module';
import { writeFileSync } from 'node:fs';

const require = createRequire(import.meta.url);
const si = require('simple-icons');

const BRANDS = {
  context7: 'upstash', // Context7 is Upstash's
  'brave-search': 'brave',
  notion: 'notion',
  obsidian: 'obsidian',
  linear: 'linear',
  sentry: 'sentry',
  gmail: 'gmail',
  'google-calendar': 'googlecalendar',
  'google-drive': 'googledrive',
  github: 'github',
  postgres: 'postgresql',
  figma: 'figma',
  excalidraw: 'excalidraw',
  forgejo: 'forgejo',
  kubernetes: 'kubernetes',
  supabase: 'supabase',
  todoist: 'todoist',
  atlassian: 'jira',
  airtable: 'airtable',
  stripe: 'stripe',
};

// Marks simple-icons does not carry, embedded as data URIs (same
// offline-first reasoning). securosys: favicon from securosys.com, used with
// their blessing — the plugin exists because they proposed it.
const IMAGES = {
  'securosys-attest': { i: 'data:image/png;base64,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', t: 'Securosys' },
};

const icons = {};
for (const [id, slug] of Object.entries(BRANDS)) {
  const key = `si${slug.charAt(0).toUpperCase()}${slug.slice(1)}`;
  const icon = si[key];
  if (!icon) throw new Error(`simple-icons has no "${slug}" (for plugin ${id})`);
  icons[id] = { p: icon.path, c: `#${icon.hex}`, t: icon.title };
}

const file = `// Plugin brand marks.
//
// Generated from simple-icons (CC0-1.0, https://simpleicons.org) by
// scripts/generate-plugin-icons.mjs — do not hand-edit; re-run the script.
// Paths are 24x24. A plugin with no brand mark in that set (Slack, which
// simple-icons dropped for brand-policy reasons, and the servers that have no
// logo at all) falls back to a monogram tile, which is why this file only
// carries the ones that exist.
(function () {
"use strict";
window.xnautPluginIcons = ${JSON.stringify({ ...icons, ...IMAGES })};

// Colour for a monogram tile. Deterministic per id so a plugin keeps its
// colour between launches without storing anything.
window.xnautPluginIconFor = function (plugin) {
  const id = String(plugin && plugin.id || "");
  const brand = window.xnautPluginIcons[id];
  if (brand && brand.i) {
    return \`<img src="\${brand.i}" width="18" height="18" alt="" aria-hidden="true">\`;
  }
  if (brand) {
    return \`<svg viewBox="0 0 24 24" width="18" height="18" fill="\${brand.c}" aria-hidden="true"><path d="\${brand.p}"/></svg>\`;
  }
  const name = String(plugin && plugin.name || id || "?");
  let hash = 0;
  for (let i = 0; i < id.length; i += 1) hash = (hash * 31 + id.charCodeAt(i)) % 360;
  const letter = name.trim().charAt(0).toUpperCase() || "?";
  return \`<span class="plg-mono" style="background:hsl(\${hash} 45% 42%)" aria-hidden="true">\${letter}</span>\`;
};
})();
`;

writeFileSync(new URL('../src/js/plugin-icons.js', import.meta.url), file);
console.log(`wrote ${Object.keys(icons).length} brand marks`);
