// The Memory view (XNAUT-333) is a reader over the index memory.rs writes, and
// every way it can be wrong is quiet: a renamed field renders a blank column, a
// client-side filter looks like a search until the day it disagrees with the one
// the agents run, and a recall block paraphrased instead of quoted is worse than
// no recall block at all. So the assertions here name values.
//
// The search test is the load-bearing one. `find()` is mirrored below from
// memory.rs:239 (words) and memory.rs:355 (find), and the stub answers
// `memory_find_cmd` with it. If the panel ever filters its own list instead of
// asking the backend, the rendered rows stop matching the mirror and the test
// fails.
import { test, expect } from '@playwright/test';

const ago = (days) => Date.now() - days * 86400000;

const INDEX = [
  {
    note: 'xnaut/Memory/2026-09-10-nautgate-route.md',
    kind: 'fix', project: 'XNAUT', ticket: 'XNAUT-301', at: ago(1),
    files: ['src-tauri/src/agents.rs'],
    title: 'A configured NautGate that is not running is not a route',
    keywords: ['xnaut-301', 'src-tauri/src/agents.rs', 'nautgate', 'route', 'configured', 'running'],
  },
  {
    note: 'xnaut/Memory/2026-09-09-emit-flood.md',
    kind: 'incident', project: 'XNAUT', ticket: 'XNAUT-54', at: ago(2),
    files: ['src-tauri/src/pty.rs'],
    title: 'An emit flood livelocked the window',
    keywords: ['xnaut-54', 'src-tauri/src/pty.rs', 'emit', 'flood', 'livelock', 'window'],
  },
  {
    note: 'bucky/Memory/2026-09-08-roster-global.md',
    kind: 'learning', project: 'BUCKY', ticket: 'BUCKY-7', at: ago(3),
    files: ['src/js/agent-roster.js'],
    title: 'The roster global had never been assigned',
    keywords: ['bucky-7', 'src/js/agent-roster.js', 'roster', 'global', 'assigned'],
  },
];

const NOTES = {
  'xnaut/Memory/2026-09-10-nautgate-route.md': {
    path: 'xnaut/Memory/2026-09-10-nautgate-route.md', at: ago(1), kind: 'fix', project: 'XNAUT',
    ticket: 'XNAUT-301', run_id: 'run-301-a', files: ['src-tauri/src/agents.rs'],
    text: 'A configured NautGate that is not running is not a route',
    cause: 'The route list was built from settings, so a gate that was configured but down still counted as reachable.',
    fix: 'Probe the gate before listing it, and drop it from the lineup when the probe fails.',
    source: 'handback:XNAUT-301-3',
  },
  'bucky/Memory/2026-09-08-roster-global.md': {
    path: 'bucky/Memory/2026-09-08-roster-global.md', at: ago(3), kind: 'learning', project: 'BUCKY',
    ticket: 'BUCKY-7', run_id: '', files: ['src/js/agent-roster.js'],
    text: 'The roster global had never been assigned',
    cause: 'window.xnautActiveProjectKey was read but never written, so the scope fell back to global with no error.',
    fix: 'Assign the global, and grep for the assignment before calling any window.* again.',
    source: 'jury:BUCKY-7-1',
  },
};

const STATS = {
  notes: INDEX.length,
  by_kind: [['fix', 1], ['incident', 1], ['learning', 1]],
  by_project: [['XNAUT', 2], ['BUCKY', 1]],
  last_sync: new Date(Date.now() - 3600000).toISOString(),
  root: '/tmp/vault/work',
  remote: true,
};

// The recall block memory.rs builds for a dispatch. Held here as one string so
// the test can assert the panel shows it character for character.
const RECALL = `## What xNAUT remembers about this area

- [fix XNAUT-301] A configured NautGate that is not running is not a route Closed by: Probe the gate before listing it. (xnaut/Memory/2026-09-10-nautgate-route.md)
- [incident XNAUT-54] An emit flood livelocked the window Cause: every PTY chunk became an emit. (xnaut/Memory/2026-09-09-emit-flood.md)
`;

// ── memory.rs's search, mirrored ────────────────────────────────────────────
// memory.rs:239. Split on anything that is not alphanumeric or / . _ -, trim
// leading and trailing dots and dashes, lowercase, keep words longer than two.
const words = (s) => String(s == null ? '' : s)
  .split(/[^0-9A-Za-z/._-]+/)
  .map((w) => w.replace(/^[.-]+|[.-]+$/g, '').toLowerCase())
  .filter((w) => w.length > 2);

// memory.rs:355. Every word of the query must appear in the entry's keywords,
// title, files or ticket; a file path matches on its last segment too.
function find(index, query, project, limit) {
  const q = words(query);
  if (!q.length) return [];
  return index
    .filter((e) => !project || String(e.project).toLowerCase() === String(project).toLowerCase())
    .filter((e) => {
      const hay = [...e.keywords, ...words(e.title), ...e.files.map((f) => f.toLowerCase()), String(e.ticket).toLowerCase()];
      return q.every((w) => hay.some((h) => h === w || h.endsWith(`/${w}`)));
    })
    .slice(0, limit);
}

async function openMemory(page, over = {}) {
  page.on('pageerror', (error) => { throw error; });
  await page.addInitScript(() => localStorage.setItem('xnaut-sidebar-visible', '1'));
  await page.goto('/?stub=1');
  await page.waitForSelector('#btn-help');

  // memory_find_cmd has to answer the query, not a fixed value, so the stub is
  // wrapped rather than assigned. The panel resolves window.__TAURI__ at call
  // time, so replacing invoke here reaches it.
  await page.evaluate(({ index, notes, stats, recall, findSrc, wordsSrc }) => {
    Object.assign(window.__xnautStub, {
      memory_index_list: index,
      memory_stats: stats,
      memory_recall_for_ticket: recall,
    });
    const mirror = new Function(`const words = ${wordsSrc};\nconst find = ${findSrc};\nreturn find;`)();
    window.__memMirror = (q, project) => mirror(window.__xnautStub.memory_index_list, q, project, 200);
    const real = window.__TAURI__.core.invoke;
    window.__TAURI__.core.invoke = (cmd, args) => {
      if (cmd === 'memory_find_cmd') {
        window.__xnautInvokes.push({ cmd, args });
        return Promise.resolve(window.__memMirror(args.query, args.project));
      }
      if (cmd === 'memory_note_read') {
        window.__xnautInvokes.push({ cmd, args });
        const note = notes[args.note];
        return note ? Promise.resolve(note) : Promise.reject(`${args.note} is not a memory note`);
      }
      return real(cmd, args);
    };
  }, {
    index: over.index || INDEX,
    notes: NOTES,
    stats: over.stats || STATS,
    recall: over.recall === undefined ? RECALL : over.recall,
    findSrc: find.toString(),
    wordsSrc: words.toString(),
  });

  // Startup opens its own tabs for a couple of seconds. Opening the panel
  // before that settles gets it switched away from mid-test, which reads as a
  // search that returned nothing. Same wait the Delivery spec takes.
  await page.waitForTimeout(2500);
  await page.evaluate(() => window.xnautOpenMemoryPanel());
  await expect(page.locator('.mem')).toBeVisible();
}

test('the list shows the notes in the index, newest first', async ({ page }) => {
  await openMemory(page);

  await expect(page.locator('.mem-row')).toHaveCount(3);
  expect(await page.locator('.mem-row .mem-title').allTextContents()).toEqual(INDEX.map((e) => e.title));
  // The kind chip, the project, the ticket and the files it names.
  const first = page.locator('.mem-row').first();
  await expect(first.locator('.mem-kind')).toHaveText('fix');
  await expect(first).toContainText('XNAUT-301');
  await expect(first.locator('.mem-files')).toContainText('src-tauri/src/agents.rs');

  // The counts and the last sync, above the list.
  await expect(page.locator('.mem-top')).toContainText('3');
  await expect(page.locator('.mem-top')).toContainText('incident');
  await expect(page.locator('.mem-top')).toContainText('BUCKY');
  await expect(page.locator('.mem-sync')).toContainText('Last sync');

  expect(await page.evaluate(() => window.__xnautErrors || [])).toEqual([]);
});

test('typing a query narrows the list to exactly what memory_search returns', async ({ page }) => {
  await openMemory(page);

  await page.locator('.mem-q').fill('nautgate');
  await expect(page.locator('.mem-row')).toHaveCount(1);
  await expect(page.locator('.mem-row .mem-title')).toHaveText(find(INDEX, 'nautgate', null, 200)[0].title);

  // A file path finds its note by its last segment, the way find() does.
  await page.locator('.mem-q').fill('pty.rs');
  await expect.poll(() => page.locator('.mem-row .mem-title').allTextContents())
    .toEqual(find(INDEX, 'pty.rs', null, 200).map((e) => e.title));

  // Every word must match, so a second word that matches nothing empties it.
  await page.locator('.mem-q').fill('nautgate roster');
  expect(find(INDEX, 'nautgate roster', null, 200)).toEqual([]);
  await expect(page.locator('.mem-row')).toHaveCount(0);
  await expect(page.locator('.mem-list')).toContainText('An agent searching the same words would also come back empty');

  // The panel asked the backend rather than filtering its own list.
  const queries = await page.evaluate(() => window.__xnautInvokes
    .filter((i) => i.cmd === 'memory_find_cmd').map((i) => i.args.query));
  expect(queries).toContain('nautgate');
  expect(queries).toContain('pty.rs');

  // The project filter is passed through to the same search.
  await page.locator('.mem-q').fill('roster');
  await page.locator('.mem-proj').selectOption('XNAUT');
  await expect(page.locator('.mem-row')).toHaveCount(0);
  expect(find(INDEX, 'roster', 'XNAUT', 200)).toEqual([]);

  expect(await page.evaluate(() => window.__xnautErrors || [])).toEqual([]);
});

test('clicking a note renders its cause and its fix', async ({ page }) => {
  await openMemory(page);
  await page.locator('.mem-row').first().click();

  const note = NOTES['xnaut/Memory/2026-09-10-nautgate-route.md'];
  const detail = page.locator('.mem-detail');
  await expect(detail.locator('.mem-h')).toHaveText(note.text);
  await expect(detail).toContainText(note.cause);
  await expect(detail).toContainText(note.fix);
  await expect(detail).toContainText('src-tauri/src/agents.rs');
  await expect(detail).toContainText(note.source);
  await expect(detail).toContainText(note.run_id);
  // Its ticket and its run are reachable.
  await expect(detail.locator('[data-open-ticket]')).toHaveText('Open XNAUT-301');
  await expect(detail.locator('[data-open-run]')).toBeVisible();

  // A note with no run offers no run link, rather than a link to nothing.
  await page.locator('.mem-row').nth(2).click();
  await expect(page.locator('.mem-detail')).toContainText('window.xnautActiveProjectKey was read but never written');
  await expect(page.locator('.mem-detail [data-open-run]')).toHaveCount(0);

  expect(await page.evaluate(() => window.__xnautErrors || [])).toEqual([]);
});

test("a ticket's recall block is shown verbatim", async ({ page }) => {
  await openMemory(page);

  await page.locator('.mem-tabs button[data-view="recall"]').click();
  await page.locator('.mem-recall-t').fill('XNAUT-301');
  await page.locator('[data-recall]').click();

  // Character for character, including the heading and the note paths, because
  // this is what the agent read before it started. textContent rather than
  // toHaveText: toHaveText normalises whitespace, which is exactly the part of
  // "verbatim" that matters in a block the model reads as markdown.
  await expect(page.locator('[data-recall-block]')).toBeVisible();
  expect(await page.locator('[data-recall-block]').textContent()).toBe(RECALL);
  const sent = await page.evaluate(() => window.__xnautInvokes
    .filter((i) => i.cmd === 'memory_recall_for_ticket').pop());
  expect(sent.args.ticket).toBe('XNAUT-301');

  expect(await page.evaluate(() => window.__xnautErrors || [])).toEqual([]);
});

test('a ticket xNAUT remembers nothing about says so, and shows no block', async ({ page }) => {
  await openMemory(page, { recall: '' });

  await page.locator('.mem-tabs button[data-view="recall"]').click();
  await page.locator('.mem-recall-t').fill('XNAUT-999');
  await page.locator('[data-recall]').click();

  await expect(page.locator('.mem-detail')).toContainText('remembers nothing about XNAUT-999');
  await expect(page.locator('.mem-detail')).toContainText('carried no recall block');
  await expect(page.locator('[data-recall-block]')).toHaveCount(0);

  expect(await page.evaluate(() => window.__xnautErrors || [])).toEqual([]);
});

test('an empty vault renders the empty state, not a blank pane', async ({ page }) => {
  await openMemory(page, {
    index: [],
    stats: { notes: 0, by_kind: [], by_project: [], last_sync: '', root: '/tmp/vault/work', remote: false },
  });

  await expect(page.locator('.mem-row')).toHaveCount(0);
  await expect(page.locator('.mem-list')).toContainText('xNAUT has not remembered anything yet');
  await expect(page.locator('.mem-detail')).toContainText('There is nothing to read yet');
  await expect(page.locator('.mem-sync')).toContainText('no remote');

  expect(await page.evaluate(() => window.__xnautErrors || [])).toEqual([]);
});

test('the sidebar Memory entry opens the panel', async ({ page }) => {
  page.on('pageerror', (error) => { throw error; });
  await page.addInitScript(() => localStorage.setItem('xnaut-sidebar-visible', '1'));
  await page.goto('/?stub=1');
  await page.waitForSelector('.sbar-nav-row');
  await page.waitForTimeout(2500);

  // The global the sidebar row and the Delivery panel's Memory tab both call.
  expect(await page.evaluate(() => typeof window.xnautOpenMemoryPanel)).toBe('function');

  await page.evaluate((index) => { window.__xnautStub.memory_index_list = index; }, INDEX);
  await page.locator('.sbar-nav-row', { hasText: 'Memory' }).click();
  await expect(page.locator('.mem')).toBeVisible();
  await expect(page.locator('.mem-row')).toHaveCount(3);

  expect(await page.evaluate(() => window.__xnautErrors || [])).toEqual([]);
});
