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
// The invariant being guarded: every name gui-smoke.sh presses must not be
// contained in any other control's name. A name inside another name is exactly
// as ambiguous as a duplicate, and axui exits 3 rather than guess.
//
// Scoped to SMOKED rather than to every pair of controls on purpose. A
// document-wide check flags "Save" against "Save workflow", and those two live
// in different modals that are never open at once, so it would fail on ten
// things that cannot actually break while missing the point. These nineteen are
// the names where ambiguity has a known consequence.
//
// One limit worth stating: the real "Settings" collision also involved macOS's
// own menu bar ("System Settings…"), which is not in the DOM and cannot be seen
// from here. The DOM half is enough to have caught it.

const PRESSABLE = 'button, a[href], [role=menuitem], [role=button], [role=tab], [role=checkbox], [role=radio]';

test.beforeEach(async ({ page }) => {
  page.on('dialog', (d) => d.dismiss().catch(() => {}));
  await page.goto('/index.html');
  await page.waitForSelector('#btn-help');
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
    const n = name.toLowerCase();
    // The control itself must exist, or the smoke test is pressing a ghost.
    const matches = labels.filter((l) => l.toLowerCase().includes(n));
    if (matches.length === 0) {
      problems.push(`"${name}" matches no control in the app`);
      continue;
    }
    const others = [...new Set(matches.map((m) => m.toLowerCase()))].filter((m) => m !== n);
    if (others.length) {
      problems.push(`"${name}" is also inside: ${others.map((o) => `"${o}"`).join(', ')}`);
    }
  }

  expect(problems, 'axui will refuse to press these').toEqual([]);
});
