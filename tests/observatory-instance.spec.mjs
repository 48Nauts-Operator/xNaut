// XNAUT-370 — the Observatory says which instance it is, and every ledger line
// says which instance wrote it.
//
// The failure this covers is not a wrong number, it is a missing subject. On
// 2026-09-13 the Studio and tron ran different builds of the same app against
// the same board; every ledger line either of them wrote read identically, so
// "which machine did this, on which version" had no answer anywhere in the
// product. Two bugs came out of that before anyone noticed the drift.
//
// So these assertions are about ATTRIBUTION. Each fails if a line goes back to
// being unattributable, or if a machine's own role stops being visible on the
// page that decides whether it starts work.
//
// XNAUT-391 moved the identity out of the stat strip and into a badge beside
// the title. The attribution rules did not change — what the machine is, and
// which lines are its own, are still the questions — so the assertions below
// were re-pointed rather than rewritten, and the new ones guard the things a
// badge can get wrong that a card could not: the accent spent on a role that
// does not dispatch, and a hostname long enough to push the header apart.
import { test, expect } from '@playwright/test';

const ME = { id: 'inst-tron-0001', role: 'fleet', version: '1.10.1', machine: 'tron' };

// Two instances and two builds in one log: the 2026-09-13 shape exactly.
const LINES = [
  { at: '2026-09-14T09:02:00Z', kind: 'sweep_dispatch', agent: 'nautbot', ticket: 'XNAUT-370',
    detail: 'launched claude on agent/claude/xnaut-370',
    instance: 'inst-tron-0001', role: 'fleet', version: '1.10.1' },
  { at: '2026-09-14T09:01:00Z', kind: 'sweep_refused', agent: 'nautbot', ticket: 'XNAUT-371',
    detail: 'this instance is a workstation on xNAUT 1.10.0; it does not dispatch',
    instance: 'inst-studio-0002', role: 'workstation', version: '1.10.0' },
  { at: '2026-09-13T18:00:00Z', kind: 'dispatched', agent: 'codex', ticket: 'XNAUT-358',
    detail: 'written before signatures existed', instance: '', role: '', version: '' },
];

async function openObservatory(page, stub) {
  await page.addInitScript(() => localStorage.setItem('xnaut-sidebar-visible', '1'));
  await page.goto('/?stub=1');
  await page.waitForSelector('#btn-help');
  await page.evaluate((s) => { Object.assign(window.__xnautStub, s); }, stub);
  await page.evaluate(() => window.xnautAttachObservatoryTab());
  await expect(page.locator('.obs')).toBeVisible();
  await expect(page.locator('.obs-strip .obs-card').first()).toBeVisible();
}

test('the deck names this instance, its role and the build it is running', async ({ page }) => {
  await openObservatory(page, { instance_stamp: ME, ledger_recent: [] });

  // XNAUT-391: a badge in the header, not a card in the strip. The identity of
  // the machine is the page's subject, so it sits with the title; the strip is
  // for things that move.
  const badge = page.locator('.obs-inst');
  await expect(badge).toBeVisible();
  await expect(page.locator('.obs-strip .obs-card', { hasText: 'This instance' })).toHaveCount(0);
  // The ROLE leads, because it is the fact that decides whether this machine
  // may start work at all — the question an owner staring at an idle board is
  // really asking. Host and build follow it, and the build is unshortened,
  // because two versions differing in the patch digit is the drift being
  // looked for.
  await expect(badge).toContainText('fleet');
  await expect(badge).toContainText('tron');
  await expect(badge).toContainText('1.10.1');
  // The badge sits beside the title, inside the header.
  await expect(page.locator('.obs-head .obs-title .obs-inst')).toHaveCount(1);
});

test('the badge carries the role meaning and the full key on its tooltip', async ({ page }) => {
  await openObservatory(page, { instance_stamp: ME, ledger_recent: [] });

  const badge = page.locator('.obs-inst');
  // What a role MEANS is read once and then known, so it is a tooltip rather
  // than a line of prose on a deck that has to stay scannable. The full key
  // goes the same way: needed only when matching against settings.json.
  await expect(badge).toHaveAttribute('title', /dispatches and verifies/);
  await expect(badge).toHaveAttribute('title', /inst-tron-0001/);
  await expect(badge).toHaveAttribute('title', /instance\.role in settings\.json/);
});

test('a workstation says on the deck that it never dispatches', async ({ page }) => {
  await openObservatory(page, {
    instance_stamp: { ...ME, role: 'workstation', machine: 'studio' },
    ledger_recent: [],
  });

  const badge = page.locator('.obs-inst');
  await expect(badge).toContainText('workstation');
  // Said on the machine it applies to. A board that is not moving on the
  // owner's desk is correct behaviour here, and the deck has to say so or it
  // reads as a stall.
  await expect(badge).toHaveAttribute('title', /never dispatches/);
});

test('only a fleet instance wears the yellow frame', async ({ page }) => {
  // The accent means "this machine dispatches". Spending it on the other two
  // roles would make it decoration, and then it stops answering anything.
  await openObservatory(page, { instance_stamp: ME, ledger_recent: [] });
  await expect(page.locator('.obs-inst')).toHaveClass(/fleet/);

  for (const role of ['workstation', 'sandbox']) {
    await page.evaluate((s) => { Object.assign(window.__xnautStub, s); },
      { instance_stamp: { ...ME, role } });
    await page.locator('.obs [data-refresh]').click();
    await expect(page.locator('.obs-inst')).toContainText(role);
    await expect(page.locator('.obs-inst')).not.toHaveClass(/fleet/);
  }
});

test('a hostname long enough to break the header is clipped instead', async ({ page }) => {
  // The failure this stops: a host like a full FQDN pushing Refresh and Stop
  // all off the right edge, or the badge growing a second line and shoving the
  // strip down. It clips, and the whole name stays on the tooltip.
  const long = 'tron-build-node-with-an-extremely-long-hostname.candoo.internal.example.com';
  await openObservatory(page, { instance_stamp: { ...ME, machine: long }, ledger_recent: [] });

  const badge = page.locator('.obs-inst');
  const head = page.locator('.obs-head');
  const actions = page.locator('.obs-actions');
  const [bb, hb, ab] = await Promise.all([
    badge.boundingBox(), head.boundingBox(), actions.boundingBox(),
  ]);
  // Inside the header's box on both axes: no horizontal overflow, one line.
  expect(bb.x + bb.width).toBeLessThanOrEqual(hb.x + hb.width + 1);
  expect(bb.height).toBeLessThanOrEqual(30);
  // And it has not eaten the buttons it shares the row with.
  expect(bb.x + bb.width).toBeLessThanOrEqual(ab.x + 1);
  expect(await actions.locator('[data-refresh]').isVisible()).toBe(true);
  // Clipped, not shrunk to nothing: the host element is narrower than its text.
  const clipped = await page.locator('.obs-inst-host')
    .evaluate((el) => el.scrollWidth > el.clientWidth);
  expect(clipped).toBe(true);
  await expect(badge).toHaveAttribute('title', new RegExp(long.replace(/\./g, '\\.')));
});

test('an instance that has not answered says so rather than guessing a role', async ({ page }) => {
  await openObservatory(page, {
    instance_stamp: { __reject: 'instance_stamp not allowed by ACL' },
    ledger_recent: [],
  });

  const badge = page.locator('.obs-inst');
  // The XNAUT-257 rule, kept through the move: an unanswered read and an
  // answered one must not look alike, and neither may be dressed as a role.
  await expect(badge).toHaveClass(/pending/);
  await expect(badge).toContainText('unavailable');
  await expect(badge).toHaveAttribute('title', /not allowed by ACL/);
  await expect(badge).not.toContainText('fleet');
});

test('every ledger line carries the instance and the build that wrote it', async ({ page }) => {
  await openObservatory(page, { instance_stamp: ME, ledger_recent: LINES });

  const band = page.locator('.obs-table', { hasText: 'Ledger' });
  await expect(band.locator('.obs-led')).toHaveCount(3);
  // Both instances and both builds are on screen — the whole point.
  await expect(band).toContainText('inst-tro');
  await expect(band).toContainText('inst-stu');
  await expect(band).toContainText('1.10.1');
  await expect(band).toContainText('1.10.0');
  // And the refusal itself is readable, which no pane on this page showed
  // before: a sweep that declines to dispatch used to be silent here.
  await expect(band).toContainText('sweep_refused');
  await expect(band).toContainText('it does not dispatch');
});

test('a line from another build is marked as drift, and this machine\'s own is not', async ({ page }) => {
  await openObservatory(page, { instance_stamp: ME, ledger_recent: LINES });

  const band = page.locator('.obs-table', { hasText: 'Ledger' });
  // Mine: the machine reading the page wrote this one.
  const mine = band.locator('.obs-led', { hasText: 'XNAUT-370' }).locator('.obs-sig');
  await expect(mine).toHaveClass(/mine/);
  await expect(mine).toHaveAttribute('title', /this machine/);

  // Theirs: a different build, which is the fact worth seeing without hunting.
  const theirs = band.locator('.obs-led', { hasText: 'XNAUT-371' }).locator('.obs-sig');
  await expect(theirs).toHaveClass(/drift/);
  await expect(theirs).toHaveAttribute('title', /different build from this one/);

  // Two instances wrote these lines and the header says so, so the drift is
  // countable rather than something you have to notice.
  await expect(band).toContainText('2 instances');
});

test('a line written before signatures existed says so instead of borrowing this machine', async ({ page }) => {
  await openObservatory(page, { instance_stamp: ME, ledger_recent: LINES });

  const band = page.locator('.obs-table', { hasText: 'Ledger' });
  const old = band.locator('.obs-led', { hasText: 'XNAUT-358' }).locator('.obs-sig');
  // The attribution rule the backend keeps: empty means UNKNOWN, never "this
  // machine". Rendering an unsigned line as mine would be the exact wrong
  // attribution the ledger module refuses to make anywhere else.
  await expect(old).toHaveText('unsigned');
  await expect(old).not.toHaveClass(/mine/);
});

test('a ledger that cannot be read says why, instead of looking empty', async ({ page }) => {
  await openObservatory(page, {
    instance_stamp: ME,
    ledger_recent: { __reject: 'ledger_recent not allowed by ACL' },
  });

  const band = page.locator('.obs-table', { hasText: 'Ledger' });
  // XNAUT-257's lesson applied to a new band: a broken read and an honest
  // empty are different facts, and a tester has to be able to tell them apart.
  await expect(band).toContainText('Could not read the ledger');
  await expect(band).toContainText('not allowed by ACL');
  await expect(band).not.toContainText('The ledger is empty');
});
