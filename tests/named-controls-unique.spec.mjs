import { test, expect } from '@playwright/test';
import { SMOKED } from '../scripts/smoked-controls.mjs';

// The sibling of close-buttons-named.spec.mjs, aimed one level up.
//
// That test was written after thirteen close buttons all announced "×", and it
// guards the close buttons and nothing else. The very next release proved the
// gap: 1.13.8 added an "Open Settings" button to the PM empty state, which made
// the More-actions item named "Settings" unpressable, because the matcher works
// by substring and "Open Settings" contains "Settings". The release test
// reported NOT PRESSED and the entire Settings walk was skipped. A fix in one
// release made a control in the next release unaddressable, and nothing caught
// it except a human running the test against the shipped build.
//
// The invariant being guarded: every name gui-smoke.sh presses must name exactly
// one control. Not zero, or the walk presses a ghost; not two, because axui
// exits 3 rather than guess between them.
//
// This used to guard containment instead -- no smoked name may sit inside
// another control's name -- which is what the substring matcher demanded. That
// requirement is gone: the walk now presses with `axui press -x`, so the two
// releases' worth of collisions it was written for ("Settings" inside "Open
// Settings", "AI settings" inside "Save AI Settings") can no longer happen at
// all. Guarding a rule the code no longer follows is worse than not guarding:
// the containment version's first new finding was a false alarm, the snippets
// button against the menu item it opens.
//
// Scoped to SMOKED rather than to every control in the document, because these
// nineteen are the names with a known consequence, and because existence is half
// of what is being checked.
//
// Two limits, and the second one is load-bearing rather than a footnote.
//
// The real "Settings" collision also involved macOS's own menu bar ("System
// Settings…"), which is not in the DOM and cannot be seen from here.
//
// More importantly: this reads the DOM as it is at load, so anything a panel
// renders when it opens is invisible to it. That is not a small gap. On
// 2026-08-10 this test passed green while "AI settings" was unpressable on the
// real build, because the colliding name -- the AI pane's own "Save AI Settings"
// button -- does not exist until Settings is opened. Treat a pass here as "the
// initial DOM is clean", never as "every name gui-smoke.sh presses is safe". The
// macOS AX walk is still the only thing that sees the whole app.

const PRESSABLE = 'button, a[href], [role=menuitem], [role=button], [role=tab], [role=checkbox], [role=radio]';

// With the bridge stub, and not without it. Every name here used to be a top or
// bottom bar button that is in index.html before a single script runs, so a bare
// load was enough. XNAUT-435 added the first one that a module renders: the
// sidebar rail's Projects icon, which is built by sidebar.js and therefore does
// not exist on a page with no Tauri bridge to mount the sidebar against.
//
// This widens what the test sees rather than narrowing it: the stub adds
// controls to the document and never removes any, so a name that was
// unambiguous before is still unambiguous now, and one made ambiguous by the
// sidebar is a collision the real app has and this test could not see.
test.beforeEach(async ({ page }) => {
  page.on('dialog', (d) => d.dismiss().catch(() => {}));
  await page.addInitScript(() => localStorage.setItem('xnaut-sidebar-visible', '1'));
  await page.goto('/index.html?stub=1');
  await page.waitForSelector('#btn-help');
  await page.waitForSelector('.sbar-rail-btn');
});

test('every name the smoke test presses is unambiguous', async ({ page }) => {
  // title= is deliberately excluded: it maps to AXHelp, which is help text and
  // not a name. innerText is the fallback because that is what axui uses when
  // there is no aria-label.
  const labels = await page.$$eval(PRESSABLE, (els) =>
    els
      .map((e) => (e.getAttribute('aria-label') || e.innerText || '').trim())
      .filter((l) => l && l.length < 60));

  expect(labels.length).toBeGreaterThan(20);

  const problems = [];
  for (const name of SMOKED) {
    // Exact and case-sensitive, because that is what gui-smoke.sh does: it
    // presses with `axui press -x`, which is strcmp. Containment is not the test
    // any more. It used to be, and it cried wolf immediately -- the snippets
    // button "Command snippets" and the menu item "Command Snippets" it opens
    // read as a collision under a case-insensitive substring rule and are two
    // unrelated names under the rule axui applies.
    //
    // Counting elements rather than distinct strings matters: axui exits 3 on
    // any count above one, so three controls all named "AI Settings" are just as
    // unpressable as one name nested inside another.
    const matches = labels.filter((l) => l === name);
    if (matches.length === 0) problems.push(`"${name}" matches no control in the app`);
    else if (matches.length > 1) problems.push(`"${name}" names ${matches.length} controls`);
  }

  expect(problems, 'axui will refuse to press these').toEqual([]);
});
