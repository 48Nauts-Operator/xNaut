// A click in the terminal must select the row it landed on, zoomed or not.
//
// It did not. The interface zooms with CSS `zoom` on the root; xterm maps a
// mouse position to a buffer row by dividing by a cell height it measured
// itself, and `zoom` scales what the DOM reports without touching that
// measurement. The two then disagree by exactly the zoom factor, so the error
// grew with distance down the screen: at 1.25, clicking the row showing L20
// selected L25. Reported as "I can't target a line, it highlights 3-4 lines
// below, so I can't copy a block".
//
// The check drives the app's own zoom entry point, not a stand-in, because the
// fix lives in what applyAppZoom does to terminals.

import { test, expect } from '@playwright/test';

const ROW = 'L20';

async function selectRowShowing(page, label) {
  const box = await page.evaluate((want) => {
    const rows = [...document.querySelectorAll('.terminal-output .xterm-rows > div')];
    const el = rows.find((d) => d.textContent.trim() === want);
    if (!el) return null;
    const r = el.getBoundingClientRect();
    return { x: r.left + 2, y: r.top + r.height / 2, h: r.height };
  }, label);
  if (!box) throw new Error(`${label} is not on screen`);
  await page.mouse.move(box.x, box.y);
  await page.mouse.down();
  await page.mouse.move(box.x + 70, box.y);
  await page.mouse.up();
  const text = await page.evaluate(() => window.xnaut.tabs[0].terminals[0].term.getSelection().trim());
  return { text, rowHeight: box.h };
}

test('a terminal click selects the row it landed on, at any interface zoom', async ({ page }) => {
  await page.goto('/?stub=1');
  await page.waitForTimeout(900);

  // The harness has no Tauri shell, so the pane the app built is never mounted.
  // Mount it: everything under test (zoom handling, font sizing, fit) is the
  // app's own code operating on the app's own terminal.
  await page.evaluate(async () => {
    const t = window.xnaut.tabs[0].terminals[0];
    t.pane.style.cssText = 'position:fixed;inset:0;z-index:99999;display:flex;flex-direction:column;';
    document.body.appendChild(t.pane);
    t.fitAddon.fit();
    for (let i = 0; i < 30; i++) t.term.write(`L${String(i).padStart(2, '0')}\r\n`);
    await new Promise((r) => setTimeout(r, 250));
  });

  const plain = await selectRowShowing(page, ROW);
  expect(plain.text).toBe(ROW);

  // 1 -> 1.1 -> 1.25 through the real keyboard path's function.
  await page.evaluate(() => { window.xnautAdjustAppZoom(1); window.xnautAdjustAppZoom(1); });
  await page.waitForTimeout(350);
  expect(await page.evaluate(() => Number(window.xnautUiZoom))).toBeCloseTo(1.25, 2);

  const zoomed = await selectRowShowing(page, ROW);
  expect(zoomed.text).toBe(ROW);

  // And the zoom still has to do its job: bigger rows, not merely correct ones.
  expect(zoomed.rowHeight).toBeGreaterThan(plain.rowHeight * 1.1);
});
