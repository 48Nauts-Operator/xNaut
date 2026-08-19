// Smoke for the decision ledger reader in Guardrails settings (XNAUT-202).
//
// ledger_recent was registered, ACL-allowed and called by nobody: every refusal
// an agent hit was written to disk and shown nowhere. Two things have to hold
// for that to be fixed rather than merely claimed:
//   - the settings surface actually asks the backend for the entries;
//   - what comes back reaches the DOM, kind and agent and reason intact.
//
// The wording is checked too. Only refused/asked/conflict are ever recorded, so
// a page that does not say so invites reading an empty list as "nothing ran".
//
// Run: node scripts/ledger-view-smoke.cjs

function el() {
  const node = { textContent: '', className: '', value: '', disabled: false, style: {}, _html: '', children: new Map() };
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

const asked = [];
const ENTRIES = [
  { at: new Date(Date.now() - 90 * 1000).toISOString(), kind: 'refused', agent: 'rudi', ticket: '', detail: 'Bash: Tags trigger a release. Prepare it and ask.', elapsed_secs: null },
  { at: new Date(Date.now() - 3 * 3600 * 1000).toISOString(), kind: 'asked', agent: 'claude', ticket: '', detail: 'Write: the websites folder is published', elapsed_secs: null },
  { at: new Date(Date.now() - 2 * 86400 * 1000).toISOString(), kind: 'conflict', agent: 'nautbot', ticket: '', detail: 'src/js/app.js with @rudi', elapsed_secs: null },
];

global.window = global;
global.document = { getElementById: () => null, head: { appendChild: () => {} }, createElement: () => ({}) };
global.__TAURI__ = { core: { invoke: async (cmd, args) => {
  asked.push([cmd, args]);
  if (cmd === 'veto_read') return { path: '/tmp/veto.toml', exists: true, text: '# rules', rules: 1 };
  if (cmd === 'veto_backups') return [];
  if (cmd === 'ledger_recent') return ENTRIES;
  return null;
} } };

require('../src/js/veto-settings.js');

const host = el();
window.xnautRenderVetoSettings(host).then(() => {
  const ledger = asked.find(([cmd]) => cmd === 'ledger_recent');
  if (!ledger) throw new Error('the guardrails page never asked for the ledger');
  if (!ledger[1] || !ledger[1].limit) throw new Error('ledger_recent was called without a limit');

  const list = host.querySelector('[data-ledger]').innerHTML;
  if (!list.includes('refused')) throw new Error('a refusal did not reach the DOM');
  if (!list.includes('Tags trigger a release')) throw new Error('the reason the agent was given was dropped');
  if (!list.includes('@rudi')) throw new Error('the agent that was stopped is not named');
  if (!list.includes('conflict')) throw new Error('a two-agent conflict did not reach the DOM');
  if (!/1m ago|2m ago/.test(list)) throw new Error(`a timestamp was not made readable: ${list}`);

  // The claim the page must not make: that an empty list means nothing ran.
  const page = host.innerHTML;
  if (!/allowed\s+call\s+records\s+nothing/i.test(page)) {
    throw new Error('the page does not say that an allowed call is never recorded');
  }
  console.log('ledger view smoke: ok');
}).catch((error) => { console.error(String(error && error.stack || error)); process.exit(1); });
