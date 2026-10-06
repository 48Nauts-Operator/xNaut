import { test, expect } from '@playwright/test';

// Drive the actual app caller as well as the browser pane: native creation can
// resolve after switchTab has detached it or closeTab has removed its owner.
for (const scenario of ['switch-away', 'close', 'switch-away-and-back']) {
  const close = scenario === 'close';
  const back = scenario === 'switch-away-and-back';
  test(`pending native browser completion remains owned across ${scenario}`, async ({ page }) => {
    const errors = [];
    page.on('pageerror', e => errors.push(String(e)));
    page.on('dialog', d => d.dismiss().catch(() => {}));
    await page.goto('/?stub=1');
    await page.waitForFunction(() => typeof window.xnautAttachBrowserTab === 'function' && typeof window.xnautSwitchTab === 'function');
    // Create a stable nonterminal destination using the actual tab path.
    await page.evaluate(async () => {
      window.keptTab = await window.xnautAttachMarkdownTab({});
    });
    await expect(page.locator('.md-edit')).toBeVisible();
    await page.evaluate(async () => {
      window.lifecycleCalls = [];
      window.activations = [];
      const original = window.__TAURI__.core.invoke;
      window.__TAURI__.core.invoke = async (cmd, args) => {
        if (cmd.startsWith('browser_pane_')) window.lifecycleCalls.push({ cmd, args });
        if (cmd === 'browser_pane_create') {
          return new Promise(resolve => { window.finishNativeCreate = () => resolve({ label: args.req.label }); });
        }
        return original(cmd, args);
      };
      const switched = window.xnautOnTabSwitched;
      window.xnautOnTabSwitched = (id) => { window.activations.push(id); switched?.(id); };
      window.pendingBrowser = await window.xnautAttachBrowserTab('https://example.com');
    });
    await page.waitForFunction(() => typeof window.finishNativeCreate === 'function');
    await page.evaluate(async ({ close, back }) => {
      window.firstNativeCreate = window.finishNativeCreate;
      if (close) {
        document.querySelector(`.tab-close[data-tab-id="${window.pendingBrowser}"]`).click();
      }
      await window.xnautSwitchTab(window.keptTab);
      if (back) window.xnautSwitchTab(window.pendingBrowser);
    }, { close, back });
    if (back) await page.waitForFunction(() => window.lifecycleCalls.filter(c => c.cmd === 'browser_pane_create').length === 2);
    await page.evaluate(() => window.firstNativeCreate());
    await page.waitForFunction(() => window.lifecycleCalls.some(c => c.cmd === 'browser_pane_destroy'));
    // Let the real awaited caller finish, not only the native cleanup callback.
    await page.waitForTimeout(100);
    const state = await page.evaluate(() => {
      const tab = eval('tabs').find(t => t.id === window.pendingBrowser);
      return { exists: !!tab, terminals: tab?.terminals.length ?? null,
        active: eval('activeTabId'), kept: window.keptTab, pending: window.pendingBrowser,
        activations: window.activations, calls: window.lifecycleCalls };
    });
    expect(state.exists).toBe(!close);
    expect(state.terminals).toBe(close ? null : 0);
    expect(state.active).toBe(back ? state.pending : state.kept);
    expect(state.activations.at(-1)).toBe(state.kept);
    expect(state.activations).not.toContain(state.pending);
    expect(state.calls.filter(c => c.cmd === 'browser_pane_create')).toHaveLength(back ? 2 : 1);
    expect(state.calls.filter(c => c.cmd === 'browser_pane_destroy')).toHaveLength(1);
    expect(errors).toEqual([]);
    if (!close) {
      // A later visit creates a fresh valid entry; no null poisons its cache.
      if (!back) await page.evaluate(() => { window.xnautSwitchTab(window.pendingBrowser); });
      await page.waitForFunction(() => window.lifecycleCalls.filter(c => c.cmd === 'browser_pane_create').length === 2);
      await page.evaluate(() => window.finishNativeCreate());
      await page.waitForFunction(() => eval('tabs').find(t => t.id === window.pendingBrowser)?.terminals.length === 1);
      await expect(page.locator('.browser-page-chip')).toBeVisible();
      expect(await page.evaluate(() => window.activations.at(-1))).toBe(state.pending);
      expect(errors).toEqual([]);
    }
  });
}
