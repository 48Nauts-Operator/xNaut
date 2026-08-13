// XNAUT-139: a work log left running when the app closed must be offered back.
//
// The session always survived on disk; only `AppState.active_worklog` died with
// the process, and `worklog_status` read nothing else. So the monitor stopped
// recording, the file stayed marked `"active": true` forever, and the hours went
// missing from the PM Space dashboard. Two such files were sitting in
// ~/.xnaut/worklogs when this was reported.
//
// Resuming is a question, not a default: time passed between the close and the
// relaunch that nobody worked, and where several logs were left open, silently
// adopting the newest would close whichever one was real.
import { test, expect } from '@playwright/test';

const ORPHAN = {
  id: 'test-orphan-1',
  client: 'Acme',
  project: 'Invoicing',
  started: new Date(Date.now() - 95 * 60 * 1000).toISOString(),
  ended: null,
  entries: [{ command: 'cargo test', directory: '/tmp' }],
  merkle_root: null,
  active: true,
};

// The stub answers unknown commands with null, and the startup path asks for
// this one within a few hundred ms of load. Patching `invoke` from an init
// script, as soon as the bridge exists, is the only hook early enough.
//
// The fake must drop a session once it has been resumed or closed, the way the
// real one does: `worklog_orphans` returns nothing while a log is running, and a
// finalized session no longer qualifies. A fixture that keeps handing back the
// same orphan makes the bar reappear the instant it is dismissed, which is the
// app behaving correctly against a backend that cannot exist.
function withOrphans(orphans) {
  return (list) => {
    window.__worklogCalls = [];
    let pending = list.slice();
    const install = setInterval(() => {
      const core = window.__TAURI__ && window.__TAURI__.core;
      if (!core) return;
      clearInterval(install);
      const real = core.invoke;
      core.invoke = (cmd, args) => {
        if (cmd.startsWith('worklog_')) window.__worklogCalls.push({ cmd, args });
        if (cmd === 'worklog_orphans') return Promise.resolve(pending);
        if (cmd === 'worklog_resume' || cmd === 'worklog_discard') {
          const taken = pending.find((s) => s.id === args.id) || null;
          // Resuming starts a log, so nothing further is on offer at all.
          pending = cmd === 'worklog_resume' ? [] : pending.filter((s) => s.id !== args.id);
          return Promise.resolve(taken);
        }
        return real(cmd, args);
      };
    }, 5);
  };
}

async function load(page, orphans) {
  await page.addInitScript(withOrphans(orphans), orphans);
  await page.goto('/?stub=1');
  await page.waitForSelector('#btn-help');
}

test('a work log left running is offered back, naming what it was', async ({ page }) => {
  await load(page, [ORPHAN]);

  const bar = page.locator('#worklog-resume-bar');
  await expect(bar).toBeVisible();
  await expect(bar).toContainText('Acme / Invoicing');
  await expect(bar).toContainText('1h 35m ago');
  await expect(bar).toContainText('1 command');
  await expect(bar).toContainText('Continue?');
});

test('nothing is offered when no work log was left running', async ({ page }) => {
  await load(page, []);
  await page.waitForTimeout(1500);

  await expect(page.locator('#worklog-resume-bar')).toHaveCount(0);
});

test('the bar does not cover the top bar', async ({ page }) => {
  await load(page, [ORPHAN]);
  await expect(page.locator('#worklog-resume-bar')).toBeVisible();

  // The update banner shipped position:fixed with no layout offset and made
  // every top-bar control unclickable. This one sits in the flow; prove it.
  const covered = await page.evaluate(() => {
    const out = [];
    for (const el of document.querySelector('.top-bar').querySelectorAll('button')) {
      const r = el.getBoundingClientRect();
      if (!r.width || !r.height) continue;
      const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
      if (hit && !el.contains(hit) && !hit.contains(el)) out.push(el.getAttribute('aria-label'));
    }
    return out;
  });

  expect(covered, `top-bar controls covered: ${covered.join(', ')}`).toEqual([]);
});

test('Continue resumes that session and starts recording again', async ({ page }) => {
  await load(page, [ORPHAN]);
  await page.locator('#worklog-resume-bar').getByRole('button', { name: 'Continue' }).click();

  const resumed = await page.evaluate(() =>
    (window.__worklogCalls || []).find((c) => c.cmd === 'worklog_resume'));
  expect(resumed, 'Continue did not resume anything').toBeTruthy();
  expect(resumed.args.id, 'resumed the wrong session').toBe(ORPHAN.id);

  await expect(page.locator('#worklog-resume-bar')).toHaveCount(0);
  // The red pill is the only sign the monitor is recording again.
  await expect(page.locator('#worklog-indicator')).toBeVisible();
});

test('Close it finalizes the session instead of resuming it', async ({ page }) => {
  await load(page, [ORPHAN]);
  await page.locator('#worklog-resume-bar').getByRole('button', { name: 'Close it' }).click();

  const calls = await page.evaluate(() => window.__worklogCalls || []);
  expect(calls.find((c) => c.cmd === 'worklog_discard')?.args.id).toBe(ORPHAN.id);
  expect(calls.find((c) => c.cmd === 'worklog_resume'), 'closing it must not resume it').toBeFalsy();

  await expect(page.locator('#worklog-resume-bar')).toHaveCount(0);
  await expect(page.locator('#worklog-indicator')).toHaveCount(0);
});

test('several orphans are offered one at a time, newest first', async ({ page }) => {
  const older = { ...ORPHAN, id: 'test-orphan-2', project: 'Older', entries: [] };
  await load(page, [ORPHAN, older]);

  const bar = page.locator('#worklog-resume-bar');
  await expect(bar).toContainText('Invoicing');
  await expect(bar, 'the caller is not told others are waiting').toContainText('+1 more');
});
