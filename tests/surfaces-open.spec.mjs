// Browser-leg baseline: every top-bar surface opens the thing it names.
//
// The browser half of the walk scripts/gui-smoke.sh does natively. The Settings
// sections and the NautFlow views are not here: reaching them means opening
// Settings first, and the macOS AX walk already presses them by exact label.
//
// Two rules, both bought the hard way.
//
// A fresh page per surface. On tron's run 20260813-124712 a shared page let one
// surface's teardown set style.display='none', which beats `.modal.show
// { display:flex }` on specificity, so the next surface could never reopen. It
// was filed as an inert surface and the app was fine.
//
// Each surface names its own container. The first version of this file asserted
// generically that "something appeared", and four probes in a row passed while
// the Command snippets handler was neutered to a no-op:
//
//   ids only            -- the four tab surfaces carry no id, so they failed
//                          rather than passed, but for the wrong reason
//   "some text changed" -- the clock and the usage footer repaint on their own
//   count increased     -- an overlay hides more than it adds; opening Help took
//                          the visible count from 239 down to 225
//   structural path key -- still green under mutation, even after settling
//
// A check that survives the mutation is decoration. Naming the container is
// duller and it actually fails.
import { test, expect } from '@playwright/test';

// Learned by probing the running app rather than read off the handlers, because
// the handler tells you what was intended and the probe tells you what happens.
const SURFACES = [
  { name: 'Toggle projects sidebar', shows: '#xnaut-sidebar-host' },
  { name: 'Toggle project pane', shows: '#xnaut-right-pane-host' },
  { name: 'Command snippets', shows: '#commands-dropdown' },
  { name: 'Open worktree manager', shows: '#worktree-modal' },
  { name: 'More actions', shows: '#more-menu-dropdown' },
  { name: 'Help and keyboard shortcuts', shows: '#help-overlay' },
];

// These four open a tab instead of a panel, and tab content carries no id, so
// the assertion is that the tab bar grew.
const TABS = [
  'Open new browser tab',
  'Open new markdown tab',
  'Open new diff tab',
  'Open Projects (tasks & plan)',
];

// Startup work keeps landing for a few seconds; clicking into it makes the
// before/after comparison measure the app waking up rather than the click.
const SETTLED = 4000;

async function open(page, name) {
  page.on('dialog', (d) => d.dismiss().catch(() => {}));
  await page.addInitScript(() => localStorage.setItem('xnaut-sidebar-visible', '0'));
  await page.goto('/?stub=1');
  await page.waitForSelector('#btn-help');
  await page.waitForTimeout(SETTLED);
}

for (const { name, shows } of SURFACES) {
  test(`${name} opens ${shows}`, async ({ page }) => {
    await open(page, name);

    await expect(page.locator(shows), `${shows} was already visible before the click`).toBeHidden();
    await page.getByRole('button', { name, exact: true }).click();
    await expect(page.locator(shows), `"${name}" did not open ${shows}`).toBeVisible();

    const errors = await page.evaluate(() => window.__xnautErrors || []);
    expect(errors, `"${name}" raised: ${errors.join(' | ')}`).toEqual([]);
  });
}

for (const name of TABS) {
  test(`${name} adds a tab`, async ({ page }) => {
    await open(page, name);

    const tabs = page.locator('#tabs-container > *');
    const before = await tabs.count();
    await page.getByRole('button', { name, exact: true }).click();
    await expect(tabs, `"${name}" did not add a tab (still ${before})`).toHaveCount(before + 1);

    const errors = await page.evaluate(() => window.__xnautErrors || []);
    expect(errors, `"${name}" raised: ${errors.join(' | ')}`).toEqual([]);
  });
}
