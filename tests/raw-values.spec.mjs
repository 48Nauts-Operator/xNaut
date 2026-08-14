// "Nothing in the interface shows a raw object or an error string."
//
// The scenario from tests/features/smoke.feature that keeps not getting run
// headless, because it reads like it needs the whole app. It does not: it needs
// every surface open at once and a sweep of the visible leaves.
//
// The failures it is aimed at all shipped at some point: a canvas showing
// [object Object], a field rendering "undefined", a spinner nobody resolves.
// They are cheap to detect and expensive to notice by eye.
import { test, expect } from '@playwright/test';

// Same list the surface walk uses, and for the same reason: these are the
// surfaces reachable without starting work.
const SURFACES = [
  'Toggle projects sidebar', 'Toggle project pane', 'Command snippets',
  'Open new browser tab', 'Open new markdown tab', 'Open new diff tab',
  'Open Projects (tasks & plan)', 'Open worktree manager', 'More actions',
  'Help and keyboard shortcuts',
];

// Leaves only. A parent's innerText contains its children's, so sweeping every
// element reports one bad string once per ancestor and buries the location.
const LEAVES = () => [...document.querySelectorAll('body *')]
  .filter((el) => !el.children.length)
  .filter((el) => {
    const r = el.getBoundingClientRect();
    return r.width > 0 && r.height > 0 && getComputedStyle(el).visibility !== 'hidden';
  })
  .map((el) => ({
    text: (el.textContent || '').trim(),
    where: `${el.nodeName.toLowerCase()}${el.id ? '#' + el.id : ''}${el.className && typeof el.className === 'string' ? '.' + el.className.split(/\s+/).join('.') : ''}`,
  }))
  .filter((l) => l.text);

// "undefined" and "NaN" as whole words. Substring matching would flag the word
// inside ordinary prose, and the help overlay is full of prose.
const BAD = [
  { label: '[object Object]', re: /\[object \w+\]/ },
  { label: 'undefined', re: /(^|[\s:>(])undefined([\s.,)<]|$)/ },
  { label: 'NaN', re: /(^|[\s:>(])NaN([\s.,)<]|$)/ },
];

// A fresh page per surface, and not only for tron's specificity reason. Opening
// them in sequence on one page cannot work at all: the worktree manager is a
// real modal, and its backdrop intercepts the click for every surface after it.
// The first version of this file failed on exactly that and it was the harness,
// not the app.
async function surface(page, name) {
  page.on('dialog', (d) => d.dismiss().catch(() => {}));
  await page.addInitScript(() => localStorage.setItem('xnaut-sidebar-visible', '0'));
  await page.goto('/?stub=1');
  await page.waitForSelector('#btn-help');
  await page.waitForTimeout(4000);
  const btn = page.getByRole('button', { name, exact: true });
  expect(await btn.count(), `no control named "${name}"`).toBeGreaterThan(0);
  await btn.click();
  await page.waitForTimeout(600);
}

for (const name of SURFACES) {
  test(`${name} shows no raw object or error string`, async ({ page }) => {
    await surface(page, name);

    const found = [];
    for (const leaf of await page.evaluate(LEAVES)) {
      for (const b of BAD) {
        if (b.re.test(leaf.text)) found.push(`${b.label} :: ${leaf.where} :: ${leaf.text.slice(0, 120)}`);
      }
    }

    expect(found, `raw values on "${name}":\n  ${found.join('\n  ')}`).toEqual([]);
  });
}

for (const name of SURFACES) {
  test(`${name} leaves no spinner running`, async ({ page }) => {
    await surface(page, name);

    // Long enough that anything still animating is not merely slow. The stub
    // answers every command immediately, so nothing is waiting on a network.
    await page.waitForTimeout(5000);

    const spinning = await page.evaluate(() => [...document.querySelectorAll('body *')]
      .filter((el) => {
        const r = el.getBoundingClientRect();
        if (!r.width || !r.height) return false;
        if (getComputedStyle(el).visibility === 'hidden') return false;
        const cls = typeof el.className === 'string' ? el.className : '';
        return /spinner|spin\b/i.test(cls) || /^Loading[.…]*$/.test((el.textContent || '').trim());
      })
      .map((el) => `${el.nodeName.toLowerCase()}.${typeof el.className === 'string' ? el.className : ''}`));

    expect(spinning, `still spinning on "${name}": ${spinning.join(', ')}`).toEqual([]);
  });
}
