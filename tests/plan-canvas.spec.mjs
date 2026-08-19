import { test, expect } from '@playwright/test';

// Plan Canvas (XNAUT-192). A plan a human can point at, annotate and answer.
//
// Driven through the real pane against the Tauri stub, because every part of
// this is a seam: which lines a click anchors to, which command the note is
// written with, and which word the verdict sends back. A unit test of the
// splitter would prove the anchor arithmetic and none of the wiring.

const PLAN = [
  '# Migration plan',            // line 1
  '',                            // 2
  'Step one: drop the table.',   // 3
  '',                            // 4
  '```sql',                      // 5
  'DROP TABLE users;',           // 6
  '```',                         // 7
  '',                            // 8
  'Step two: rebuild it.',       // 9
].join('\n');

// A notes.json that actually remembers what was written to it. The stock stub
// answers every invoke from a fixed table, which would make notes_add a no-op
// and hide the whole round trip.
async function openPlanPane(page, { pending = null } = {}) {
  await page.addInitScript(() => localStorage.setItem('xnaut-sidebar-visible', '1'));
  await page.goto('/?stub=1');
  // The panel factories are plain <script>s; waiting on a clock instead of on
  // the function is how a cold first run reports "no plan pane" (2026-08-19).
  await page.waitForFunction(() => typeof window.xnautAttachPlanTab === 'function');
  await page.waitForTimeout(400);
  await page.evaluate(({ plan, pending }) => {
    const base = window.__TAURI__.core.invoke;
    const doc = { version: 1, files: [] };
    window.__xnautStub.read_file = plan;
    window.__xnautStub.notes_read = doc;
    window.__xnautStub.inbox_list = pending
      ? [{ id: pending, status: 'open', kind: 'approve', context: { plan_project: '/tmp/smoke', plan_file: 'PLAN.md' } }]
      : [];
    window.__TAURI__.core.invoke = (cmd, args) => {
      if (cmd === 'notes_add') {
        window.__xnautInvokes.push({ cmd, args });
        let file = doc.files.filter((f) => f.path === args.filePath)[0];
        if (!file) { file = { path: args.filePath, annotations: [] }; doc.files.push(file); }
        file.annotations.push(Object.assign({ id: 'note-' + (file.annotations.length + 1) }, args.note));
        return Promise.resolve(doc);
      }
      if (cmd === 'notes_remove') {
        window.__xnautInvokes.push({ cmd, args });
        doc.files.forEach((f) => { f.annotations = f.annotations.filter((a) => a.id !== args.noteId); });
        return Promise.resolve(doc);
      }
      return base(cmd, args);
    };
    window.xnautAttachPlanTab({ projectContext: { path: '/tmp/smoke' } });
  }, { plan: PLAN, pending });
  const view = page.locator('.plan-doc-view');
  await expect(view.locator('.plan-block')).toHaveCount(4);
  return view;
}

const noteOn = async (view, index, text) => {
  const block = view.locator('.plan-block').nth(index);
  await block.click();
  await block.locator('.plan-composer-input').fill(text);
  await block.getByRole('button', { name: 'Add note' }).click();
};

test('a click anchors a note to the plan lines that block came from', async ({ page }) => {
  const view = await openPlanPane(page);

  // Four blocks: the heading, step one, the fenced SQL, step two. The fence is
  // ONE block including both delimiters, so a note on it means the whole
  // statement rather than a stray backtick line.
  const bounds = await view.locator('.plan-block').evaluateAll(
    (els) => els.map((el) => [Number(el.dataset.start), Number(el.dataset.end)]),
  );
  expect(bounds).toEqual([[1, 1], [3, 3], [5, 7], [9, 9]]);

  await noteOn(view, 2, 'this drops production data');

  const added = await page.evaluate(() => window.__xnautInvokes.filter((i) => i.cmd === 'notes_add'));
  expect(added).toHaveLength(1);
  // notes.rs anchors, unchanged: an inclusive 1-indexed range on the new side.
  expect(added[0].args.note.newRange).toEqual([5, 7]);
  expect(added[0].args.note.summary).toBe('this drops production data');
  expect(added[0].args.filePath).toBe('PLAN.md');

  // And it comes back onto the block it was left on, numbered.
  const noted = view.locator('.plan-block').nth(2);
  await expect(noted.locator('.plan-note-n')).toHaveText('1');
  await expect(noted.locator('.plan-note-text')).toHaveText('this drops production data');
});

test('notes are numbered down the plan, not in the order they were clicked', async ({ page }) => {
  const view = await openPlanPane(page);
  await noteOn(view, 3, 'rebuild from the dump');   // line 9, clicked first
  await noteOn(view, 1, 'why drop it at all');      // line 3, clicked second

  // Read top to bottom, note 1 has to be the one nearer the top of the plan.
  const order = await view.locator('.plan-note').evaluateAll((els) => els.map((el) => [
    el.querySelector('.plan-note-n').textContent,
    el.querySelector('.plan-note-text').textContent,
  ]));
  expect(order).toEqual([['1', 'why drop it at all'], ['2', 'rebuild from the dump']]);
});

test('Approve and Request changes answer the waiting agent', async ({ page }) => {
  // An agent is already blocked when the tab opens, so the buttons cannot
  // depend on having caught the plan-review event.
  let view = await openPlanPane(page, { pending: 'in-plan-1' });
  const bar = page.locator('.plan-review-bar');
  await expect(bar).toBeVisible();
  await expect(bar.getByRole('button', { name: 'Approve plan' })).toBeVisible();

  await noteOn(view, 1, 'step one needs a backup first');
  await bar.getByRole('button', { name: 'Request changes' }).click();
  await expect(bar.locator('.plan-review-msg')).toHaveText('Sent back with 1 note.');
  let decided = await page.evaluate(() => window.__xnautInvokes.filter((i) => i.cmd === 'inbox_decide'));
  expect(decided).toHaveLength(1);
  expect(decided[0].args).toEqual({ id: 'in-plan-1', decision: 'denied' });
  // Answered once, and only once: the buttons go away with the question.
  await expect(bar.getByRole('button', { name: 'Approve plan' })).toBeHidden();

  view = await openPlanPane(page, { pending: 'in-plan-2' });
  await page.locator('.plan-review-bar').getByRole('button', { name: 'Approve plan' }).click();
  decided = await page.evaluate(() => window.__xnautInvokes.filter((i) => i.cmd === 'inbox_decide'));
  expect(decided[0].args).toEqual({ id: 'in-plan-2', decision: 'approved' });
  await expect(page.locator('.plan-review-msg')).toHaveText('Approved. The agent is unblocked.');
});

test('a plan handed over by an agent lands in the pane already showing it', async ({ page }) => {
  const view = await openPlanPane(page);
  await expect(page.locator('.plan-review-bar')).toBeHidden();

  await page.evaluate(() => window.__xnautEmit('plan-review', {
    id: 'in-plan-3',
    project: '/tmp/smoke',
    planPath: '/tmp/smoke/PLAN.md',
    title: 'Review the plan: PLAN.md',
  }));

  await expect(page.locator('.plan-review-bar')).toBeVisible();
  await expect(page.locator('.plan-review-msg')).toHaveText('An agent is waiting on this plan.');
  // Adopted in place, not stacked: still one plan pane on screen.
  await expect(view).toHaveCount(1);
});

test('a note whose lines the plan lost is still shown, not silently dropped', async ({ page }) => {
  const view = await openPlanPane(page);
  await noteOn(view, 3, 'rebuild from the dump');
  // The plan shrinks under the note: line 9 no longer exists.
  await page.locator('.plan-doc-toggle button[data-mode="edit"]').click();
  await page.locator('.plan-doc-edit').fill('# Migration plan');
  await page.locator('.plan-doc-toggle button[data-mode="review"]').click();

  const drift = page.locator('.plan-drift');
  await expect(drift).toBeVisible();
  await expect(drift.locator('.plan-note-text')).toHaveText('rebuild from the dump');
});
