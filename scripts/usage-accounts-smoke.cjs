// Usage footer, several MAX accounts (XNAUT-24 phase 3).
//
// Same lifting trick as flow-tracks-smoke.cjs: refresh and claudeBlock are
// pulled out of the real source rather than copied, so this cannot keep passing
// after they change. The backend is a stub, so no Keychain is touched.
const { readFileSync } = require('node:fs');
const src = readFileSync(`${__dirname}/../src/js/usage-footer.js`, 'utf8');

const lift = (name, re) => {
  const m = src.match(re);
  if (!m) throw new Error(`${name} not found in usage-footer.js — did it get renamed?`);
  return m[0];
};
const body = [
  // Whole line, not up to the first `;` — esc's own replacement string has one.
  lift('esc', /const esc = .*/),
  lift('fillColor', /const fillColor = [\s\S]*?;/),
  lift('metric', /function metric\(pct, label\) \{[\s\S]*?\n {2}\}/),
  lift('claudeBlock', /function claudeBlock\(u, label\) \{[\s\S]*?\n {2}\}/),
  lift('refresh', /async function refresh\(footer, btn\) \{[\s\S]*?\n {2}\}/),
].join('\n');

const ok = (cond, label) => { if (!cond) throw new Error(label); };
let calls, seen;
// String concatenation, not a template literal: the lifted source is full of
// backticks and ${…} of its own.
// eslint-disable-next-line no-eval
const ctx = eval('((invoke, render) => {\n' + body + '\nreturn { refresh, claudeBlock }; })')(
  (cmd, args) => {
    calls.push([cmd, args]);
    const r = seen[cmd];
    return typeof r === 'function' ? r(args) : Promise.resolve(r);
  },
  (footer, claudes, codex, codexErr) => { calls.rendered = { claudes, codex, codexErr }; },
);

const USAGE = { five_hour_pct: 4, seven_day_pct: 31, per_model: [] };
const run = async (stubs) => {
  calls = [];
  seen = stubs;
  await ctx.refresh({}, null);
  return calls;
};

(async () => {
  // One account is the normal case: one request, no account argument, and no
  // label — the footer must look exactly as it did before this existed.
  let c = await run({ max_accounts: ['cand0rian'], max_usage: USAGE, codex_usage: Promise.reject('none') });
  ok(c.filter(([n]) => n === 'max_usage').length === 1, 'one account means one usage request');
  ok(c.find(([n]) => n === 'max_usage')[1].account === null, 'a single account is fetched as the default');
  ok(c.rendered.claudes.length === 1 && c.rendered.claudes[0].label === '', 'a lone account is not labelled');

  // Several accounts: one request each, each block carrying its own name.
  c = await run({ max_accounts: ['personal', 'work'], max_usage: USAGE, codex_usage: Promise.reject('none') });
  const asked = c.filter(([n]) => n === 'max_usage').map(([, a]) => a.account);
  ok(JSON.stringify(asked) === '["personal","work"]', `each account is fetched, got ${asked}`);
  ok(c.rendered.claudes.map((x) => x.label).join() === 'personal,work', 'blocks keep their account labels');

  // One account failing must not take the others down with it — the whole point
  // of the strip is seeing which account is out of headroom.
  c = await run({
    max_accounts: ['personal', 'work'],
    max_usage: ({ account }) => (account === 'work' ? Promise.reject('expired') : Promise.resolve(USAGE)),
    codex_usage: Promise.reject('none'),
  });
  ok(c.rendered.claudes.length === 2, 'a failed account still gets a block');
  ok(c.rendered.claudes[0].usage && c.rendered.claudes[1].usage === null, 'only the failed account renders empty');

  // Enumeration is best-effort: an old build without the command, or a Keychain
  // that will not dump, still has to render the default account.
  c = await run({ max_accounts: Promise.reject('not allowed by ACL'), max_usage: USAGE, codex_usage: Promise.reject('none') });
  ok(c.rendered.claudes.length === 1 && c.rendered.claudes[0].usage, 'no account list falls back to the default');
  c = await run({ max_accounts: [], max_usage: USAGE, codex_usage: Promise.reject('none') });
  ok(c.rendered.claudes.length === 1, 'an empty account list falls back to the default');

  // Codex is still fetched exactly once no matter how many Claude accounts there
  // are, and its rejection still reaches the renderer.
  c = await run({ max_accounts: ['a', 'b', 'c'], max_usage: USAGE, codex_usage: Promise.reject('none') });
  ok(c.filter(([n]) => n === 'codex_usage').length === 1, 'codex is polled once per refresh');
  ok(c.rendered.codexErr === 'none', 'a codex failure still reaches the footer');

  // The label goes into HTML, and Keychain account names are not ours to trust.
  const evil = ctx.claudeBlock(USAGE, '<img src=x onerror=alert(1)>');
  ok(!evil.includes('<img'), 'an account name cannot inject markup');
  ok(!ctx.claudeBlock(USAGE, '').includes('uf-acct'), 'no label means no label element');

  console.log('usage accounts: 12 checks passed');
})();
