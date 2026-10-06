import { test, expect } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  page.on('dialog', d => d.dismiss().catch(() => {}));
  await page.goto('/?stub=1');
  await page.waitForFunction(() => typeof window.toggleSettingsPanel === 'function');
});

for (const voice of [false, true]) {
  test(`hidden Settings ${voice ? 'voice entry' : 'toggle'} is opaque without animation frames and closes`, async ({ page }) => {
    const state = await page.evaluate(voice => {
      Object.defineProperty(document, 'hidden', { configurable: true, get: () => true });
      window.requestAnimationFrame = () => 0;
      const focus = document.activeElement;
      if (voice) window.xnautOpenVoiceSettings(); else window.toggleSettingsPanel();
      const panel = document.getElementById('settings-panel');
      const s = getComputedStyle(panel), r = panel.getBoundingClientRect();
      return { opacity: s.opacity, animation: s.animationName, width: r.width, height: r.height,
        section: document.querySelector('.settings-nav-item.active')?.dataset.section,
        focusUnchanged: document.activeElement === focus };
    }, voice);
    expect(state.opacity).toBe('1');
    expect(state.animation).toBe('none');
    expect(state.width).toBeGreaterThan(600);
    expect(state.height).toBeGreaterThan(400);
    expect(state.section).toBe(voice ? 'voice' : 'ai');
    expect(state.focusUnchanged).toBe(true);
    await page.locator('#btn-close-settings-panel').click();
    await expect(page.locator('#settings-panel')).toBeHidden();
    expect(await page.evaluate(() => document.elementFromPoint(8, 8)?.closest('#settings-panel') !== null)).toBe(false);
  });
}

test('hiding during Settings fade settles its opacity and foreground reopen retains animation', async ({ page }) => {
  const state = await page.evaluate(() => {
    Object.defineProperty(document, 'hidden', { configurable: true, get: () => false });
    window.toggleSettingsPanel();
    const panel = document.getElementById('settings-panel');
    const animation = panel.getAnimations()[0];
    animation.pause(); animation.currentTime = 0;
    const before = getComputedStyle(panel).opacity;
    Object.defineProperty(document, 'hidden', { configurable: true, get: () => true });
    document.dispatchEvent(new Event('visibilitychange'));
    const after = getComputedStyle(panel).opacity;
    window.toggleSettingsPanel();
    Object.defineProperty(document, 'hidden', { configurable: true, get: () => false });
    document.dispatchEvent(new Event('visibilitychange'));
    window.toggleSettingsPanel();
    return { before, after, animation: getComputedStyle(panel).animationName };
  });
  expect(state.before).toBe('0');
  expect(state.after).toBe('1');
  expect(state.animation).toBe('fadeIn');
  await expect.poll(() => page.locator('#settings-panel').evaluate(e => getComputedStyle(e).opacity)).toBe('1');
});
