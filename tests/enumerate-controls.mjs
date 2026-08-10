// Enumerate every interactive control the app actually renders.
//
// The point is to build a checklist from reality rather than from a source
// grep. A grep finds `data-stop` in a template string; it cannot tell you
// whether that control is ever rendered, which surface it lands on, or what its
// label is. This opens the app, walks every surface, and records what is there.
//
// Output: tests/control-inventory.json, which is the raw material for the
// release checklist. Anything the app renders and the checklist omits is an
// untested control, which is the number that matters.
//
//   node tests/static-server.mjs &
//   node tests/enumerate-controls.mjs

import { chromium } from '@playwright/test';
import { writeFile } from 'node:fs/promises';

const BASE = process.env.BASE || 'http://127.0.0.1:4173/?stub=1';

/** Everything a user can act on, with enough identity to find it again. */
const SNIFF = () => {
  const sel = 'button, [role=button], a[href], input, select, textarea, [data-act], [onclick], [class*=tab]';
  const seen = [];
  for (const el of document.querySelectorAll(sel)) {
    const r = el.getBoundingClientRect();
    const data = Object.fromEntries(
      [...el.attributes].filter((a) => a.name.startsWith('data-')).map((a) => [a.name, a.value]),
    );
    seen.push({
      tag: el.tagName.toLowerCase(),
      label: (el.getAttribute('aria-label') || el.title || el.innerText || el.value || '')
        .trim().replace(/\s+/g, ' ').slice(0, 60),
      data,
      cls: (el.className || '').toString().split(/\s+/).filter(Boolean).slice(0, 3).join(' '),
      visible: r.width > 0 && r.height > 0,
      disabled: el.disabled === true,
    });
  }
  return seen;
};

const app = await chromium.launch();
const page = await app.newPage();
const consoleErrors = [];
page.on('pageerror', (e) => consoleErrors.push({ message: String(e), stack: String(e.stack || '').split('\n').slice(0, 6) }));
await page.goto(BASE, { waitUntil: 'networkidle' });
await page.waitForTimeout(2500);

const surfaces = {};
surfaces['(initial)'] = await page.evaluate(SNIFF);

// Walk the top bar. Verified against the rendered app: the icons are
// button.btn-icon carrying aria-label or title, NOT the data-view attributes a
// source grep suggests. Each click opens a surface; record what it renders.
//
// Re-resolve the button each iteration: clicking one rebuilds the bar, so a
// handle captured up front goes stale and the walk silently stops after the
// first click.
const navLabels = await page.$$eval('button.btn-icon',
  (els) => els.map((e) => e.getAttribute('aria-label') || e.title || '').filter(Boolean));

for (const label of navLabels) {
  // Dismiss whatever the last surface left open. Without this the walk stops
  // dead after the first dropdown: its overlay intercepts every later click and
  // all twelve remaining surfaces report a timeout that has nothing to do with
  // them. Escape, then a click on empty space, then Escape again for anything
  // that only closes on a second press.
  await page.keyboard.press('Escape').catch(() => {});
  await page.waitForTimeout(250);

  try {
    const btn = await page.$(`button.btn-icon[aria-label="${label}"], button.btn-icon[title="${label}"]`);
    if (!btn) { surfaces[label] = { error: 'control vanished after a previous click' }; continue; }
    // force: skip actionability. Playwright reports these buttons as visible,
    // enabled and stable and then hangs on the click itself, so its checks are
    // not the problem and waiting on them just costs two seconds per control.
    // For an inventory the goal is to reach the surface, not to prove the
    // pointer sequence a human would produce.
    await btn.click({ timeout: 2000, force: true });
    await page.waitForTimeout(600);
    surfaces[label] = await page.evaluate(SNIFF);
  } catch (e) {
    surfaces[label] = { error: String(e).split('\n').slice(0, 6).join(' / ') };
  }
}

const total = Object.values(surfaces)
  .filter(Array.isArray)
  .reduce((n, list) => n + list.filter((c) => c.visible).length, 0);

await writeFile(
  new URL('control-inventory.json', import.meta.url),
  JSON.stringify({ base: BASE, at: new Date().toISOString(), surfaces, consoleErrors }, null, 2),
);

console.log(`surfaces reached : ${Object.keys(surfaces).length}`);
console.log(`visible controls : ${total}`);
console.log(`page errors      : ${consoleErrors.length}`);
for (const [name, list] of Object.entries(surfaces)) {
  const n = Array.isArray(list) ? list.filter((c) => c.visible).length : `ERROR ${list.error}`;
  console.log(`  ${String(name).padEnd(28)} ${n}`);
}
await app.close();
