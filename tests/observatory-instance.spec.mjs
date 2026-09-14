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

  const card = page.locator('.obs-card', { hasText: 'This instance' });
  // The ROLE leads, because it is the fact that decides whether this machine
  // may start work at all — the question an owner staring at an idle board is
  // really asking.
  await expect(card).toContainText('fleet');
  await expect(card).toContainText('dispatches and verifies');
  await expect(card).toContainText('tron');
  // The key is shortened for reading; the build is not, because two versions
  // that differ in the patch digit is exactly the drift being looked for.
  await expect(card).toContainText('1.10.1');
  await expect(card).toContainText('inst-tro');
});

test('a workstation says on the deck that it never dispatches', async ({ page }) => {
  await openObservatory(page, {
    instance_stamp: { ...ME, role: 'workstation', machine: 'studio' },
    ledger_recent: [],
  });

  const card = page.locator('.obs-card', { hasText: 'This instance' });
  await expect(card).toContainText('workstation');
  // Said in words on the machine it applies to. A board that is not moving on
  // the owner's desk is correct behaviour here, and the deck has to say so or
  // it reads as a stall.
  await expect(card).toContainText('never dispatches');
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
