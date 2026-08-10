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
  // [role=menuitem] is not optional. The More actions menu is eight plain divs
  // carrying role=menuitem, and Settings is one of them, so without this the
  // entire Settings panel is invisible to the inventory and the checklist
  // silently claims the app has no settings at all.
  // [data-section] for the same reason: the Settings sections are plain divs
  // carrying nothing else the selector recognises, so all seven were invisible
  // to the inventory and Settings looked like a single page.
  const sel = 'button, [role=button], [role=menuitem], a[href], input, select, textarea, [data-act], [data-section], [onclick], [class*=tab]';
  const seen = [];
  for (const el of document.querySelectorAll(sel)) {
    const r = el.getBoundingClientRect();
    const data = Object.fromEntries(
      [...el.attributes].filter((a) => a.name.startsWith('data-')).map((a) => [a.name, a.value]),
    );
    seen.push({
      tag: el.tagName.toLowerCase(),
      // The tag alone cannot tell a control from a container: the selector
      // over-collects on purpose, so the tab bar arrives as a div next to a
      // menu item that is also a div. Record the role so consumers can keep one
      // and drop the other.
      role: el.getAttribute('role') || '',
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
//
// Visible ones only. `button.btn-icon` also matches controls that live inside
// closed modals -- "Close worktree manager" and "Refresh worktree list" are both
// in the worktree dialog -- and the walk dutifully tried to click them and
// reported two errors that were the harness asking for something that was not on
// screen, not the app failing to open it.
const navLabels = await page.$$eval('button.btn-icon', (els) => els
  .filter((e) => e.getBoundingClientRect().width > 0)
  .map((e) => e.getAttribute('aria-label') || e.title || '').filter(Boolean));

// The surfaces that are themselves menus: opening one only reveals more
// navigation, so stopping there leaves Settings, SSH Connections, Knowledge
// Graph, Agents and Loops recorded as a single click and never entered.
//
// Two shapes cover every second level in the app: the More actions menu items,
// and the Settings section list. Start Work Log is excluded for the same reason
// gui-smoke.sh excludes it -- it mutates state, and an inventory run must leave
// nothing for the operator to undo.
const CHILD_SEL = '[role=menuitem][data-action]:not([data-action=worklog]), [data-section]';

// A path is the list of clicks that reaches a surface, and it is replayed from a
// fresh load every time rather than navigated statefully. Clicking a menu item
// closes the menu, so reaching the next sibling means re-opening the parent, and
// the bookkeeping for that is worse than the two seconds a reload costs. Reload
// also means one broken surface cannot poison the ones after it, which was the
// old walk's failure mode.
async function walk(path) {
  await page.goto(BASE, { waitUntil: 'domcontentloaded' });
  await page.waitForTimeout(2000);
  for (const step of path) {
    const el = await page.$(step.sel);
    if (!el) throw new Error(`step vanished: ${step.label}`);
    // force: skip actionability. Playwright reports these buttons visible,
    // enabled and stable and then hangs on the click dispatch itself.
    //
    // Checked before working around it, because "the test needs force" can mean
    // "the button is broken": nothing covers the button
    // (document.elementFromPoint at its centre returns the button), computed
    // pointer-events is auto, and a DOM .click() runs the handler. A real click
    // works. This is a harness artefact, not a product defect.
    await el.click({ timeout: 2000, force: true });
    await page.waitForTimeout(600);
  }
  return page.evaluate(SNIFF);
}

/** The second-level navigation this surface just revealed, as replayable steps. */
async function childSteps() {
  return page.$$eval(CHILD_SEL, (els) => els
    .filter((e) => e.getBoundingClientRect().width > 0)
    .map((e) => {
      const a = e.getAttribute('data-action');
      return {
        sel: a ? `[role=menuitem][data-action="${a}"]` : `[data-section="${e.getAttribute('data-section')}"]`,
        label: (e.innerText || a || e.getAttribute('data-section') || '').trim().slice(0, 40),
      };
    }));
}

const queue = navLabels.map((label) => [{
  sel: `button.btn-icon[aria-label="${label}"], button.btn-icon[title="${label}"]`,
  label,
}]);

// Enumerate each target once, whichever path reached it first. Without this the
// walk multiplies: an open menu is still on screen after one of its items is
// clicked, so every sibling gets queued again under every sibling.
const visited = new Set(queue.map((p) => p[0].sel));

// Depth 3 is where the app runs out: top bar -> More actions -> Settings ->
// section. Nothing goes deeper, and a bound means a mislabelled child cannot
// turn this into an infinite walk.
while (queue.length) {
  const path = queue.shift();
  const name = path.map((s) => s.label).join(' > ');
  try {
    surfaces[name] = await walk(path);
    if (path.length < 3) {
      for (const step of await childSteps()) {
        if (visited.has(step.sel)) continue;
        visited.add(step.sel);
        queue.push([...path, step]);
      }
    }
  } catch (e) {
    surfaces[name] = { error: String(e).split('\n').slice(0, 6).join(' / ') };
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
