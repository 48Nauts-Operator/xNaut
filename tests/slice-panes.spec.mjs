// XNAUT-106 — see the codebase an agent is working in.
//
// André, mid-build: "We have no screen to see the actual codebase it is working
// in." The Files, Search, Git and Slice diff views were all rooted at the
// PROJECT while the work happened in a nautloom/<slug> worktree beside it, so
// during a build you could watch an agent type and not see one file it touched.
//
// Four things are asserted here, and each fails if its production line is
// reverted:
//   1. selecting a slice tab roots the panes at THAT slice's worktree;
//   2. the provenance band names which worktree is on screen;
//   3. a slice that is still running is marked read-only, a finished one is not;
//   4. the read-only mark is enforced — a save into a live worktree is refused.
import { test, expect } from '@playwright/test';

const OK = {
  enabled: true, configured: true, valid: true, repo_path: '/tmp/smoke-control',
  remote_url: '', git_repository: true, project_count: 1, ticket_count: 0,
  error: '', warning: '', branch: 'main', last_commit: '', dirty: false, ahead: 0, behind: 0,
};

// Two slices in two worktrees, one still running. This is the shape
// publishBuildToSwarm writes, which is the list every surface here reads.
const QUEUE = [
  { id: 'engine', title: 'Engine chains', project: 'SMOKE', status: 'running',
    wt: '/tmp/smoke/.worktrees/engine', branch: 'nautloom/engine', sid: 's1', started: 1 },
  { id: 'ui', title: 'Dashboard UI', project: 'SMOKE', status: 'done',
    wt: '/tmp/smoke/.worktrees/ui', branch: 'nautloom/ui', sid: 's2', started: 2 },
];

async function openPage(page) {
  await page.addInitScript(() => {
    localStorage.setItem('xnaut-sidebar-visible', '1');
    localStorage.setItem('xnaut-right-pane-visible', '1');
  });
  await page.goto('/?stub=1');
  await page.waitForFunction(() => typeof window.xnautRightPaneSetRoot === 'function');
  await page.waitForSelector('[data-rpane-view="files"]');
}

// Drive the real Build stage, not a hand-made tab strip: the bug was that the
// build stage never CALLED the re-rooting plumbing, so a test that called it
// directly would have passed against the broken code.
async function openBuildStage(page, queue) {
  await page.evaluate(({ status, q }) => {
    window.__xnautStub.pm_module_status = status;
    window.xnautBuild = window.xnautBuild || {};
    window.xnautBuild.project = 'SMOKE';
    window.xnautBuild.queue = q;
    window.xnautBuild.active = q.some((w) => w.status === 'running');
    document.querySelectorAll('#pm-test-host').forEach((n) => n.remove());
    const host = document.createElement('div');
    host.id = 'pm-test-host';
    document.body.appendChild(host);
    window.xnautCreateProjectManagementPanel('pm-test', host, {});
  }, { status: OK, q: queue });

  const pane = page.locator('#pm-test-host .pmw');
  await pane.locator('[data-project]:not([data-project=""])').first().click();
  await pane.locator('[data-project-section="nautflow"]').click();
  await pane.locator('[data-flow-stage="build"]').click();
  return pane;
}

test.beforeEach(async ({ page }) => {
  page.on('dialog', (d) => d.dismiss().catch(() => {}));
});

test('selecting a build slice roots the panes at that slice worktree', async ({ page }) => {
  await openPage(page);
  const pane = await openBuildStage(page, QUEUE);

  // The panel publishes the queue under its own project key; the tabs are the
  // slices. Two of them, in order.
  const tabs = pane.locator('.pmw-build-tab');
  await expect(tabs).toHaveCount(2);

  await tabs.nth(1).click();
  await expect.poll(() => page.evaluate(() => {
    const o = window.xnautRightPaneOrigin();
    return o && o.path;
  })).toBe('/tmp/smoke/.worktrees/ui');

  await tabs.nth(0).click();
  await expect.poll(() => page.evaluate(() => window.xnautRightPaneOrigin().path))
    .toBe('/tmp/smoke/.worktrees/engine');

  // And the Files view really re-rooted — the pane's own label, not the call.
  await page.locator('[data-rpane-view="files"]').click();
  await expect(page.locator('.rpf-root')).toHaveText('/tmp/smoke/.worktrees/engine');
});

test('the provenance band names the worktree, and a project root clears it', async ({ page }) => {
  await openPage(page);
  const pane = await openBuildStage(page, QUEUE);
  await pane.locator('.pmw-build-tab').nth(1).click();

  const band = page.locator('.rpane-origin');
  await expect(band).toBeVisible();
  await expect(band.locator('.rpane-origin-badge')).toHaveText('build slice');
  await expect(band.locator('.rpane-origin-name')).toHaveText('Dashboard UI · nautloom/ui');
  await expect(band.locator('.rpane-origin-path')).toHaveText('/tmp/smoke/.worktrees/ui');

  // A plain project switch passes no origin: the band goes away rather than
  // keeping a stale slice name over a tree that is no longer that slice's.
  await page.evaluate(() => window.xnautRightPaneSetRoot('/tmp/smoke'));
  await expect(band).toBeHidden();
  expect(await page.evaluate(() => window.xnautRightPaneOrigin())).toBeNull();
});

test('a running slice is marked read-only and a finished one is not', async ({ page }) => {
  await openPage(page);
  const pane = await openBuildStage(page, QUEUE);
  const band = page.locator('.rpane-origin');

  await pane.locator('.pmw-build-tab').nth(0).click(); // status: running
  await expect(band).toHaveAttribute('data-live', '1');
  await expect(band.locator('.rpane-origin-ro')).toHaveText('READ-ONLY · agent writing');

  await pane.locator('.pmw-build-tab').nth(1).click(); // status: done
  await expect(band).not.toHaveAttribute('data-live', '1');
  await expect(band.locator('.rpane-origin-ro')).toHaveCount(0);
});

test('a path is attributed to the running slice that owns it, by longest prefix', async ({ page }) => {
  await openPage(page);
  const answers = await page.evaluate((q) => {
    window.xnautBuild = { project: 'SMOKE', active: true, queue: q.concat([
      // A sibling whose worktree path STARTS WITH another's. Only the '/'
      // boundary test separates them; a bare startsWith would hand
      // engine-docs' README to engine.
      { id: 'engine-docs', title: 'Engine docs', project: 'SMOKE', status: 'running',
        wt: '/tmp/smoke/.worktrees/engine-docs', branch: 'nautloom/engine-docs' },
      // A worktree checked out INSIDE another one. Legal git, and the only
      // case where longest-prefix rather than first-match decides the answer:
      // both entries genuinely contain this file, and the inner one owns it.
      { id: 'vendor', title: 'Vendored engine', project: 'SMOKE', status: 'running',
        wt: '/tmp/smoke/.worktrees/engine/vendor', branch: 'nautloom/vendor' },
    ]) };
    const id = (p) => { const s = window.xnautLiveSliceFor(p); return s ? s.id : null; };
    return {
      inside: id('/tmp/smoke/.worktrees/engine/src/chain.js'),
      sibling: id('/tmp/smoke/.worktrees/engine-docs/README.md'),
      nested: id('/tmp/smoke/.worktrees/engine/vendor/lib/x.js'),
      finished: id('/tmp/smoke/.worktrees/ui/src/app.js'),
      outside: id('/tmp/smoke/src/app.js'),
      // A directory NOBODY in the queue owns, whose path happens to start with
      // one that is owned. Nothing longer can outrank the wrong answer here,
      // so this is the case the '/' boundary alone has to get right.
      strayPrefix: id('/tmp/smoke/.worktrees/engine-experiments/notes.md'),
      finishedAnyStatus: (window.xnautSliceFor('/tmp/smoke/.worktrees/ui/src/app.js') || {}).id,
    };
  }, QUEUE);

  expect(answers.inside).toBe('engine');
  expect(answers.sibling, 'a sibling worktree whose path starts with another\'s was misattributed').toBe('engine-docs');
  expect(answers.nested, 'a worktree inside another must win — it is the one actually holding the file').toBe('vendor');
  expect(answers.finished, 'a finished slice no longer owns its worktree').toBeNull();
  expect(answers.outside, 'a file outside every worktree belongs to no slice').toBeNull();
  expect(answers.strayPrefix, 'an unowned directory was swallowed by a worktree whose name it starts with').toBeNull();
  expect(answers.finishedAnyStatus, 'provenance still knows the finished slice').toBe('ui');
});

test('a save into a worktree an agent is writing is refused, not written', async ({ page }) => {
  await openPage(page);
  const result = await page.evaluate((q) => {
    window.xnautBuild = { project: 'SMOKE', active: true, queue: q };
    const toasts = [];
    window.xnautToast = (m) => toasts.push(String(m));
    const before = window.__xnautInvokes.filter((i) => i.cmd === 'write_file').length;
    const blockedLive = window.xnautSliceWriteBlocked('/tmp/smoke/.worktrees/engine/src/chain.js');
    const blockedDone = window.xnautSliceWriteBlocked('/tmp/smoke/.worktrees/ui/src/app.js');
    const after = window.__xnautInvokes.filter((i) => i.cmd === 'write_file').length;
    return { blockedLive, blockedDone, wrote: after - before, toasts };
  }, QUEUE);

  expect(result.blockedLive, 'a live slice worktree accepted a write').toBe(true);
  expect(result.blockedDone, 'a finished slice is not read-only').toBe(false);
  expect(result.wrote, 'the guard wrote the file anyway').toBe(0);
  expect(result.toasts.join(' ')).toContain('Engine chains');

  // The editor save is wired to it, so the refusal is real and not just an
  // exported helper nobody calls.
  const editorWrote = await page.evaluate(async () => {
    const ta = document.createElement('textarea');
    ta.id = 'editor-textarea';
    ta.value = 'human edit';
    document.body.appendChild(ta);
    window.xnautToast = () => {};
    const before = window.__xnautInvokes.filter((i) => i.cmd === 'write_file').length;
    await window.xnautOpenInEditor('/tmp/smoke/.worktrees/engine/src/chain.js');
    const spawned = window.__xnautInvokes.filter((i) => i.cmd === 'create_command_session').length;
    const after = window.__xnautInvokes.filter((i) => i.cmd === 'write_file').length;
    return { wrote: after - before, spawned };
  });
  expect(editorWrote.spawned, '$EDITOR was spawned inside the agent\'s worktree').toBe(0);
  expect(editorWrote.wrote).toBe(0);
});

test('the Slice diff view shows what the rooted worktree changed since it forked', async ({ page }) => {
  await openPage(page);
  const pane = await openBuildStage(page, QUEUE);
  await pane.locator('.pmw-build-tab').nth(0).click();

  await page.locator('[data-rpane-view="buildfiles"]').click();
  const host = page.locator('.bf-host');
  await expect(host).toBeVisible();

  // It asked about the worktree the panes are rooted at, not the project.
  await expect.poll(() => page.evaluate(() => {
    const calls = window.__xnautInvokes.filter((i) => i.cmd === 'slice_changes');
    return calls.length ? calls[calls.length - 1].args.worktree : null;
  })).toBe('/tmp/smoke/.worktrees/engine');

  await expect(host.locator('.bf-meta')).toContainText('base abc1234');
  await expect(host.locator('.bf-meta')).toContainText('feature/dashboard');
  const rows = host.locator('.bf-file');
  await expect(rows).toHaveCount(2);
  await expect(rows.nth(0)).toContainText('src/engine/chain.js');
  await expect(rows.nth(0)).toContainText('+84');
  await expect(rows.nth(1)).toContainText('New');

  // A file opens its diff, from the merge base.
  await rows.nth(0).click();
  await expect(host.locator('.bf-diff .bf-line.add').first()).toContainText('steps.reduce');

  // Now switch slice while this view is ALREADY mounted and on screen. Mounting
  // reads the root once; following it afterwards is what setRoot is for, and a
  // view that ignored it would keep showing the previous agent's diff under the
  // new agent's name — the exact confusion the provenance band exists to stop.
  await pane.locator('.pmw-build-tab').nth(1).click();
  await expect.poll(() => page.evaluate(() => {
    const calls = window.__xnautInvokes.filter((i) => i.cmd === 'slice_changes');
    return calls.length ? calls[calls.length - 1].args.worktree : null;
  })).toBe('/tmp/smoke/.worktrees/ui');
  // The expanded diff belongs to the slice that is gone: it must not survive.
  await expect(host.locator('.bf-diff')).toHaveCount(0);
});
