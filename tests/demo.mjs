// Headed demo of the Help overlay, driven on DISPLAY=:0 so the screen recorder
// captures it. Exercises the feature slowly, the way a human would.
import { chromium } from '@playwright/test';

const URL = process.env.DEMO_URL || 'http://127.0.0.1:4173/index.html';
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const browser = await chromium.launch({
  headless: false,
  args: [
    '--window-position=0,0',
    '--window-size=1440,900',
    '--disable-session-crashed-bubble',
    '--disable-infobars',
  ],
});
const context = await browser.newContext({ viewport: null });
const page = await context.newPage();
page.on('dialog', (d) => d.dismiss().catch(() => {}));

// Give the user a realistic, customized keybinding so the demo shows real data.
await page.addInitScript(() => {
  localStorage.setItem('xnaut-keybindings', JSON.stringify({
    newTab: { key: 't', ctrl: true, label: 'New Tab' },
  }));
});

console.log('demo: loading app…');
await page.goto(URL, { waitUntil: 'domcontentloaded' });
await page.waitForSelector('#btn-help');
await sleep(3500);

// 1. Point out the new Help icon (last on the right of the top bar).
const help = page.locator('#btn-help');
await help.hover();
await sleep(3500);

// 2. Click it — overlay slides out from the right.
console.log('demo: opening help overlay…');
await help.click();
await page.waitForFunction(() => document.querySelector('#help-overlay')?.classList.contains('open'));
await sleep(3500);

// 3. Slowly move through the shortcut groups so they can be read on camera.
const list = page.locator('#help-overlay-list');
for (const frac of [0.25, 0.55, 0.85, 1]) {
  await list.evaluate((el, f) => { el.scrollTop = (el.scrollHeight - el.clientHeight) * f; }, frac);
  await sleep(3500);
}
await list.evaluate((el) => { el.scrollTop = 0; });
await sleep(2000);

// 4. Close with the × button.
console.log('demo: closing via button…');
await page.locator('#btn-help-close').click();
await page.waitForFunction(() => !document.querySelector('#help-overlay')?.classList.contains('open'));
await sleep(3500);

// 5. Reopen using the "?" keyboard shortcut — a "reminder" the user can pop anytime.
console.log('demo: reopening via "?" key…');
await page.keyboard.press('Shift+Slash');
await page.waitForFunction(() => document.querySelector('#help-overlay')?.classList.contains('open'));
await sleep(3500);

// 6. Scroll once more, then close by clicking the dimmed backdrop.
await list.evaluate((el) => { el.scrollTop = el.scrollHeight; });
await sleep(3500);
await list.evaluate((el) => { el.scrollTop = 0; });
await sleep(2000);

console.log('demo: closing via backdrop click…');
await page.locator('#help-overlay-backdrop').click({ position: { x: 60, y: 300 } });
await page.waitForFunction(() => !document.querySelector('#help-overlay')?.classList.contains('open'));
await sleep(3000);

// 6b. Reopen and close with Escape to show that path too.
console.log('demo: reopening then closing via Escape…');
await page.keyboard.press('Shift+Slash');
await page.waitForFunction(() => document.querySelector('#help-overlay')?.classList.contains('open'));
await sleep(3500);
await page.keyboard.press('Escape');
await page.waitForFunction(() => !document.querySelector('#help-overlay')?.classList.contains('open'));
await sleep(3000);

// 7. Open one last time to end on the feature.
await help.click();
await sleep(3000);

// Capture a still for the report.
await page.waitForFunction(() => document.querySelector('#help-overlay')?.classList.contains('open')).catch(() => {});
await page.screenshot({ path: '/artifacts/help-overlay.png' }).catch(() => {});
await sleep(3500);

console.log('demo: done');
await browser.close();
