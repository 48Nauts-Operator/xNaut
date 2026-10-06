import { test, expect } from '@playwright/test';

// Exercise the production pane in isolation: only the native IPC boundary is
// recorded. A missing frame callback reproduces the hidden WKWebView stall.
test.beforeEach(async ({ page, baseURL }) => {
  await page.setContent('<div id="host" style="width:720px;height:440px;display:flex"></div>');
  await page.evaluate(() => {
    window.nativeCalls = [];
    window.__TAURI__ = { core: { invoke: async (cmd, args) => {
      window.nativeCalls.push({ cmd, args });
      return { label: args?.req?.label };
    } } };
  });
  await page.addScriptTag({ url: `${baseURL}/js/browser-pane.js` });
});

for (const hidden of [true, false]) {
  test(`native creation settles without frame callbacks, hidden=${hidden}`, async ({ page }) => {
    const result = await page.evaluate(async (hidden) => {
      Object.defineProperty(document, 'hidden', { configurable: true, get: () => hidden });
      window.requestAnimationFrame = () => 7;
      window.cancelAnimationFrame = () => {};
      const entry = await window.xnautCreateBrowserPane('test', document.querySelector('#host'), 'https://example.com');
      return { kind: entry.kind, calls: window.nativeCalls, chips: document.querySelectorAll('.browser-page-chip').length };
    }, hidden);
    expect(result.kind).toBe('browser');
    expect(result.chips).toBe(1);
    const creates = result.calls.filter(c => c.cmd === 'browser_pane_create');
    expect(creates).toHaveLength(1);
    expect(creates[0].args.req.width).toBeGreaterThan(600);
    expect(creates[0].args.req.height).toBeGreaterThan(300);
  });
}

test('unmount during paused frames creates no native child', async ({ page }) => {
  const result = await page.evaluate(async () => {
    Object.defineProperty(document, 'hidden', { configurable: true, get: () => false });
    window.requestAnimationFrame = () => 7;
    window.cancelAnimationFrame = () => {};
    const pending = window.xnautCreateBrowserPane('test', document.querySelector('#host'));
    document.querySelector('#host').remove();
    return { entry: await pending, calls: window.nativeCalls };
  });
  expect(result.entry).toBeNull();
  expect(result.calls).toEqual([]);
});

test('zero geometry refuses native creation rather than creating a one-pixel child', async ({ page }) => {
  const result = await page.evaluate(async () => {
    Object.defineProperty(document, 'hidden', { configurable: true, get: () => true });
    document.querySelector('#host').style.display = 'none';
    try { await window.xnautCreateBrowserPane('test', document.querySelector('#host')); }
    catch (e) { return { error: String(e), calls: window.nativeCalls }; }
    return { error: null, calls: window.nativeCalls };
  });
  expect(result.error).toContain('no renderable bounds');
  expect(result.calls).toEqual([]);
});

test('a tab removed during native creation destroys its unregistered child', async ({ page }) => {
  const result = await page.evaluate(async () => {
    Object.defineProperty(document, 'hidden', { configurable: true, get: () => true });
    window.__TAURI__.core.invoke = async (cmd, args) => {
      window.nativeCalls.push({ cmd, args });
      if (cmd === 'browser_pane_create') document.querySelector('#host').remove();
      return { label: args?.req?.label };
    };
    return { entry: await window.xnautCreateBrowserPane('test', document.querySelector('#host')), calls: window.nativeCalls };
  });
  expect(result.entry).toBeNull();
  expect(result.calls.map(c => c.cmd)).toEqual(['browser_pane_create', 'browser_pane_destroy']);
  expect(result.calls[1].args.label).toBe(result.calls[0].args.req.label);
});
