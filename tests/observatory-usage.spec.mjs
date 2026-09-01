// XNAUT-257 — the Observatory's usage cards.
//
// The bug was not that a number was wrong. It was that every failure looked
// identical to an honest zero: a fetch that threw, a plan with no per-model
// limits, and a machine with no Codex history all rendered the same dash. The
// tester could not tell a broken app from an idle one, and neither could we.
//
// So these assertions are about WHY, not about layout. Each one fails if an
// empty card goes back to saying nothing about its own emptiness.
import { test, expect } from '@playwright/test';

// The verified live shape of GET /api/oauth/usage, trimmed to what parse_usage
// returns (src-tauri/src/usage.rs). Values are the real ones observed on
// 2026-09-01 so the expectations below are checkable against a real account.
const USAGE = {
  five_hour_pct: 12, seven_day_pct: 3,
  five_hour_resets_at: '2026-09-01T22:59:59Z',
  seven_day_resets_at: '2026-09-02T05:59:59Z',
  per_model: [{ name: 'Fable', percent: 3, resets_at: '2026-09-02T05:59:59Z' }],
  severity: 'normal',
  spend: { used: 9.04, limit: 150, currency: 'USD', percent: 6, enabled: true },
};

async function openObservatory(page, stub) {
  await page.addInitScript(() => localStorage.setItem('xnaut-sidebar-visible', '1'));
  await page.goto('/?stub=1');
  await page.waitForSelector('#btn-help');
  await page.evaluate((s) => { Object.assign(window.__xnautStub, s); }, stub);
  await page.evaluate(() => window.xnautAttachObservatoryTab());
  await expect(page.locator('.obs')).toBeVisible();
  await expect(page.locator('.obs-strip .obs-card').first()).toBeVisible();
}

test('a failed usage fetch names the failure instead of showing a bare dash', async ({ page }) => {
  await openObservatory(page, {
    max_usage: { __reject: 'usage endpoint returned HTTP 401' },
    codex_usage: { __reject: 'no Codex rate-limit data in session logs' },
  });

  const strip = page.locator('.obs-strip');
  // The reason the tester needed and did not get. Without it, "—" could mean
  // 0% used, and on a fresh MAX account that is a plausible reading.
  await expect(strip).toContainText('HTTP 401');
  await expect(strip).toContainText('no Codex rate-limit data');
  // And it must be the reason, not the old silent placeholder text.
  await expect(strip).not.toContainText('no per-model data');
});

test('an honest empty says which kind of empty it is', async ({ page }) => {
  // The fetch SUCCEEDED; the plan simply carries no per-model limit and no
  // spend block. That is a different sentence from "the request failed", and
  // conflating the two is the whole bug.
  await openObservatory(page, {
    max_usage: { ...USAGE, per_model: [], spend: null },
    codex_usage: null,
  });

  const strip = page.locator('.obs-strip');
  await expect(strip).toContainText('no per-model limits on this plan');
  await expect(strip).toContainText('plan reports no spend block');
  // Data that did arrive still renders, so this is not a blanket error state.
  await expect(strip).toContainText('12%');
});

test('real extra-usage spend is shown, captioned as extra usage rather than cost', async ({ page }) => {
  await openObservatory(page, { max_usage: USAGE, codex_usage: null });

  const card = page.locator('.obs-card', { hasText: 'Extra usage' });
  await expect(card).toContainText('$9.04');   // 904 minor units from the API
  await expect(card).toContainText('of $150.00');
  // It must never be labelled the cost of the work: on a MAX plan the marginal
  // cost of work inside the plan limits is zero, and this figure is only the
  // credits billed beyond those limits.
  await expect(card).not.toContainText('cost');
});

test('the sidebar usage strip reads the same command as everything else', async ({ page }) => {
  // It used to read ~/.flowai/usage.json, which nothing in this repo writes, so
  // it could only ever say "usage: n/a". Reported as a blank surface alongside
  // the Observatory cards, and a different subsystem from all of them.
  await page.addInitScript(() => localStorage.setItem('xnaut-sidebar-visible', '1'));
  await page.goto('/?stub=1');
  await page.waitForSelector('#btn-help');
  await page.evaluate((u) => { window.__xnautStub.max_usage = u; }, USAGE);
  await page.locator('[aria-label="Refresh plan usage"]').click();

  await expect(page.locator('.sbar-usage-rows')).toContainText('12% 5h');
  await expect(page.locator('.sbar-usage-rows')).toContainText('3% wk');
  await expect(page.locator('.sbar-usage-rows')).not.toContainText('n/a');
  // Nothing may go looking for the file nobody writes.
  const paths = await page.evaluate(() => window.__xnautInvokes
    .filter((i) => i.cmd === 'read_file').map((i) => i.args?.path || ''));
  expect(paths.filter((p) => p.includes('.flowai'))).toEqual([]);
});

test('Refresh refetches the usage cards, and Kill no longer throws', async ({ page }) => {
  await openObservatory(page, { max_usage: USAGE, codex_usage: null });

  await page.evaluate(() => { window.__xnautInvokes.length = 0; });
  await page.locator('[data-refresh]').click();
  // The whole point of the control: the plan numbers are refetched, not just
  // the row list. Before the fix nothing in this panel could refetch them.
  await expect.poll(() => page.evaluate(
    () => window.__xnautInvokes.filter((i) => i.cmd === 'max_usage').length,
  )).toBeGreaterThan(0);

  // refresh() was called by Stop-all and every Kill button but never defined,
  // so both threw ReferenceError. Any such throw lands in __xnautErrors.
  await page.locator('[data-stopall]').click();
  await expect.poll(() => page.evaluate(() => window.__xnautErrors || [])).toEqual([]);
});
