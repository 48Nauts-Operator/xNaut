// André, 2026-09-22 on 1.27.1: the browser pane had a gap on the left and its
// page ran off the right. getBoundingClientRect already reports viewport
// points under CSS zoom (probed in WebKit and Chromium), and the pane was
// multiplying by the zoom again (XNAUT-437). The bounds handed to the native
// webview must equal the pane's own rect plus the inset, whatever the zoom.
import { test, expect } from '@playwright/test';

for (const zoom of [1, 1.25]) {
  test(`the browser webview lands on its pane at zoom ${zoom}`, async ({ page }) => {
    page.on('dialog', (d) => d.dismiss().catch(() => {}));
    await page.addInitScript(() => localStorage.setItem('xnaut-sidebar-visible', '0'));
    await page.goto('/?stub=1');
    await page.waitForSelector('#btn-help');
    await page.waitForTimeout(1500);
    await page.evaluate((z) => {
      window.__xnautStub.browser_pane_create = { label: 'stub-browser', url: 'https://duckduckgo.com' };
      document.documentElement.style.zoom = z === 1 ? '' : String(z);
      window.xnautUiZoom = z;
    }, zoom);
    await page.getByRole('button', { name: 'Open new browser tab', exact: true }).click();
    await page.waitForFunction(() => (window.__xnautInvokes || []).some((i) => i.cmd === 'browser_pane_create'));
    const got = await page.evaluate(() => {
      const call = (window.__xnautInvokes || []).find((i) => i.cmd === 'browser_pane_create');
      const req = call.args.req || call.args;
      const pane = document.querySelector('.browser-pane').getBoundingClientRect();
      return { x: req.x, width: req.width, paneLeft: pane.left, paneWidth: pane.width };
    });
    // The pane's rect is what the engine reports, already in viewport points.
    expect(Math.abs(got.x - (got.paneLeft + 6))).toBeLessThan(1.5);
    expect(Math.abs(got.width - (got.paneWidth - 12))).toBeLessThan(1.5);
  });
}
