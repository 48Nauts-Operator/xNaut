// Smoke check for the Decisions right-pane view.
//
// Two things here can be silently wrong and pass every other check:
//   - the worktree rule: the host hands the view a PATH, and a run inside
//     <project>/.worktrees/<branch> must still ask for <project>, or it reads an
//     empty log and looks like nothing was ever decided;
//   - the "may shorten, may never resolve" rule: open items and missing
//     rationales must reach the DOM, not be tidied into a count.
//
// Run: node scripts/decisions-view-smoke.cjs

function el() {
  const node = { textContent: '', className: '', _html: '', children: new Map() };
  Object.defineProperty(node, 'innerHTML', {
    get() { return node._html; },
    set(v) { node._html = String(v); },
  });
  node.querySelector = (sel) => {
    if (!node.children.has(sel)) node.children.set(sel, el());
    return node.children.get(sel);
  };
  return node;
}

let asked = null;
let summarizeCalls = 0;
const brief = {
  summary: { text: 'Rate limiting settled on a token bucket; burst size still open.', at: '2026-08-12T20:00:00Z', n: 2, error: '' },
  headline: [{ ts: '', role: 'Architect', boundary: 'task.done', what: '', why: 'token bucket', alternatives: 'fixed window', key: 'rate-limit', open: false }],
  open: [{ ts: '', role: 'Security', boundary: 'council.verdict', what: '', why: 'burst size still disputed', alternatives: '', key: 'burst', open: true }],
  unexplained: 1,
  detail: [
    { ts: '', role: 'hook', boundary: 'task.done', what: 'task.done at Edit', why: '', alternatives: '', key: 's1', open: false },
    { ts: '', role: 'Security', boundary: 'council.verdict', what: '', why: 'burst size still disputed', alternatives: '', key: 'burst', open: true },
  ],
};

global.window = global;
global.document = {
  getElementById: () => null,
  head: { appendChild: () => {} },
  createElement: () => ({}),
};
// The two commands must be told apart: decision_log_summarize returns nothing,
// and a stub that hands it a brief would hide the refresh -> summarize loop.
global.__TAURI__ = { core: { invoke: async (cmd, args) => {
  if (cmd === 'decision_log_summarize') { summarizeCalls += 1; return null; }
  asked = args;
  return brief;
} } };

require('../src/js/right-pane-decisions.js');

const view = global.xnautViews.decisions;
const container = el();
view.mount(container, '/Users/x/DevHub_Studio/factory/02-Development/xnaut/.worktrees/nautflow-incident-loop');

setTimeout(() => {
  view.destroy();
  const head = container.querySelector('.dl-project');
  const note = container.querySelector('.dl-note');
  const body = container.querySelector('.dl-body');

  if (!asked || asked.project !== 'xnaut') {
    throw new Error(`worktree path must collapse to the project, asked for: ${asked && asked.project}`);
  }
  if (head.textContent !== 'xnaut') throw new Error('header does not name the project');
  if (!/1 without a reason/.test(note.textContent)) throw new Error('unexplained boundaries were not surfaced');
  if (!body.innerHTML.includes('burst size still disputed')) throw new Error('an open item did not reach the DOM');
  if (!body.innerHTML.includes('no rationale recorded')) throw new Error('a missing rationale was hidden');
  if (!body.innerHTML.includes('Open (1)')) throw new Error('the open section is missing');
  if (!body.innerHTML.includes('token bucket; burst size still open')) throw new Error('the summary block did not render');
  if (body.innerHTML.indexOf('dl-sum') > body.innerHTML.indexOf('Open (1)')) {
    throw new Error('the summary must sit above the mechanical sections, not replace them');
  }
  if (summarizeCalls > 1) throw new Error(`summarize ran ${summarizeCalls} times for one log length`);

  // A plain project root, no worktree.
  view.mount(container, '/Users/x/DevHub_Studio/factory/02-Development/xnaut');
  setTimeout(() => {
    view.destroy();
    if (asked.project !== 'xnaut') throw new Error('plain root must use its last component');
    console.log('decisions view smoke: ok');
  }, 20);
}, 20);
