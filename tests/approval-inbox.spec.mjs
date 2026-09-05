// XNAUT-154. "Approvals clear from anywhere" means the phone too: the bridge
// has served /api/inbox since XNAUT-32 and nothing on mobile.html called it.
import { test, expect } from '@playwright/test';

test('an approval posted to the phone clears from the phone', async ({ page }) => {
  const item = {
    id: 'in-1', at: new Date().toISOString(), project: 'SMOKE', from: 'builder',
    kind: 'approve', title: 'Push agent/builder/smoke-1?', body: 'Suites are green.',
    level: 'info', options: [], context: {}, links: [], status: 'open',
  };
  let decided = null;
  let answered = false;

  await page.route('**/cdn.jsdelivr.net/**', (r) =>
    r.fulfill({ contentType: 'text/javascript', body: 'window.Terminal = function () {};' }));
  await page.route('**/api/**', (route) => {
    const url = new URL(route.request().url());
    if (url.pathname === '/api/sessions') return route.fulfill({ json: [] });
    if (url.pathname === '/api/inbox') return route.fulfill({ json: answered ? [] : [item] });
    if (url.pathname.endsWith('/decide')) {
      decided = url.searchParams.get('decision');
      answered = true;
      return route.fulfill({ json: { ...item, status: decided } });
    }
    return route.fulfill({ status: 404, body: 'not stubbed' });
  });

  await page.goto('/mobile.html#test-token');

  const row = page.locator('#inbox-list .msg');
  await expect(row, 'the waiting approval never reached the phone').toHaveCount(1);
  await expect(row).toContainText('Push agent/builder/smoke-1?');
  await expect(page.locator('#inbox-count')).toHaveText('1 WAITING');

  await row.getByRole('button', { name: 'Approve' }).click();
  await expect(row, 'the answered item stayed in the list').toHaveCount(0);
  await expect(page.locator('#inbox-wrap')).not.toHaveClass(/\bon\b/);
  expect(decided, 'Approve did not reach the bridge').toBe('approved');
});
