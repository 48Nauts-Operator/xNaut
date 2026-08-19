// Smoke for the Codex spend estimate on the usage footer (XNAUT-202).
//
// codex_spend was registered and never called, so the USD estimate existed only
// in Rust. Three things have to hold:
//   - the footer asks for it;
//   - the figure reaches the strip, labelled as an estimate at API list prices
//     rather than as money that left an account (codex_spend.rs is explicit that
//     any UI showing it must say which of the two it means);
//   - an unpriced model shows NOTHING, never $0.00. A model with no known price
//     reading as free is the one wrong answer here.
//
// Run: node scripts/codex-spend-footer-smoke.cjs

function el() {
  const node = { id: '', textContent: '', className: '', style: {}, _html: '', children: new Map() };
  Object.defineProperty(node, 'innerHTML', {
    get() { return node._html; },
    set(v) { node._html = String(v); },
  });
  node.querySelector = (sel) => {
    if (!node.children.has(sel)) node.children.set(sel, el());
    return node.children.get(sel);
  };
  node.appendChild = () => {};
  node.classList = { add: () => {}, remove: () => {} };
  return node;
}

const app = el();
let footer = null;
const asked = [];
let spend = [{ session_id: 'a', model: 'gpt-5-codex', started: '', input_tokens: 2e6, cached_input_tokens: 1e6, output_tokens: 1e6, reasoning_output_tokens: 0, total_tokens: 3e6, cost_usd: 11.375, path: '/tmp/a.jsonl' }];

global.window = global;
global.document = {
  readyState: 'complete',
  getElementById: (id) => (id === 'app' ? app : (id === 'uf-styles' ? { id } : null)),
  head: { appendChild: () => {} },
  createElement: () => { footer = el(); return footer; },
  addEventListener: () => {},
};
global.__TAURI__ = { core: { invoke: async (cmd) => {
  asked.push(cmd);
  if (cmd === 'max_accounts') return [];
  if (cmd === 'max_usage') return { five_hour_pct: 12, seven_day_pct: 40, per_model: [] };
  if (cmd === 'codex_usage') return { plan_type: 'Plus', primary: { used_percent: 33, window_label: '5h' } };
  if (cmd === 'codex_spend') return spend;
  return null;
} } };
// The footer polls; one pass is all this needs and a live timer keeps node up.
global.setInterval = () => 0;

require('../src/js/usage-footer.js');

setTimeout(() => {
  if (!asked.includes('codex_spend')) throw new Error('the footer never asked what the last codex run cost');
  const strip = footer.innerHTML;
  if (!strip.includes('~$11')) throw new Error(`the estimate is not on the strip: ${strip}`);
  if (!strip.includes('about $11.38')) throw new Error('the exact figure is not in the tooltip');
  if (!/list prices/i.test(strip)) throw new Error('the figure does not say it is at API list prices');
  if (!/not money billed/i.test(strip)) throw new Error('the figure reads as a bill, which it is not');

  // An unpriced model: tokens are still known, the price is not.
  spend = [{ ...spend[0], model: 'some-future-model', cost_usd: null }];
  const again = el();
  global.document.createElement = () => { footer = again; return again; };
  const button = footer.querySelector('.uf-refresh');
  button.onclick();
  setTimeout(() => {
    if (/\$/.test(footer.innerHTML)) {
      throw new Error(`an unpriced model must show no figure at all, got: ${footer.innerHTML}`);
    }
    console.log('codex spend footer smoke: ok');
  }, 30);
}, 40);
