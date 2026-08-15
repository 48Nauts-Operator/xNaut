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
window.xnautPluginIcons = ${JSON.stringify(icons)};

// Colour for a monogram tile. Deterministic per id so a plugin keeps its
// colour between launches without storing anything.
window.xnautPluginIconFor = function (plugin) {
  const id = String(plugin && plugin.id || "");
  const brand = window.xnautPluginIcons[id];
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
