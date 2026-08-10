#!/usr/bin/env node
// Turn a red GUI smoke run into a Forgejo issue, and keep it to ONE issue.
//
//   node scripts/report-issue.mjs ~/xnaut-testing/runs/tron/20260810-192859
//   node scripts/report-issue.mjs <run-dir> --post     actually file it
//
// Prints by default. Filing is the side-effecting half and needs --post, because
// a test harness that can open issues unattended will eventually open forty.
//
// This runs HERE, not on tron. tron has no route to Forgejo -- no tailscale, and
// curl to cosmos:3000 times out -- so the run comes home over rsync and the issue
// is filed from the machine that already has the token. That is also the better
// split: the test machine never needs a credential.
//
// ONE issue per (version, case), enforced by a marker comment in the body. A
// failure that repeats across cycles is the same incident and gets a comment,
// not a duplicate. Re-filing it every cycle is how a board becomes unreadable,
// and it also destroys the signal that actually matters: how many cycles a
// failure has survived.
//
// ponytail: no client library, no cache. One fetch to search, one to write.

import { readFile } from 'node:fs/promises';
import { basename, join } from 'node:path';
import { homedir } from 'node:os';

const FORGE = process.env.FORGEJO_URL || 'http://cosmos.tail138398.ts.net:3000';
const REPO = process.env.FORGEJO_REPO || '48Nauts/xnaut';

const args = process.argv.slice(2);
const post = args.includes('--post');
const runDir = args.find((a) => !a.startsWith('--'));
if (!runDir) {
  console.error('usage: report-issue.mjs <run-dir> [--post]');
  process.exit(2);
}

const run = JSON.parse(await readFile(join(runDir, 'run.json'), 'utf8'));
const failed = (run.cases || []).filter((c) => c.status !== 'passed');

if (!failed.length) {
  console.log(`run ${run.id} (${run.app_version}) is all green — nothing to file.`);
  process.exit(0);
}

// The token is read only to be put in a header. It is never printed, never
// interpolated into a message, and never written to the run record.
let token = '';
try {
  token = (await readFile(join(homedir(), '.config/forgejo/token'), 'utf8')).trim();
} catch {
  if (post) {
    console.error('report-issue: no ~/.config/forgejo/token — cannot post.');
    process.exit(1);
  }
}

const api = async (path, init) => {
  const res = await fetch(`${FORGE}/api/v1/${path}`, {
    ...init,
    headers: {
      'Content-Type': 'application/json',
      ...(token ? { Authorization: `token ${token}` } : {}),
      ...(init?.headers || {}),
    },
  });
  if (!res.ok) throw new Error(`${init?.method || 'GET'} ${path} -> ${res.status} ${await res.text()}`);
  return res.json();
};

const shots = (c) => (c.shots || []).map((s) => `\`${s}\``).join(', ') || '_none_';

for (const c of failed) {
  // The marker is the identity of the incident. Keyed on version+case rather
  // than on the note, because the note carries a changing count ("9 pressed but
  // unverifiable: ...") and keying on it would open a fresh issue every time the
  // count moved by one.
  const marker = `<!-- gui-smoke:${run.app_version}:${c.id} -->`;
  const title = `GUI smoke: ${c.id} ${c.status} on ${run.app_version}`;

  // null is "this section does not apply", '' is a deliberate blank line. Both
  // were '' at first, and dropping the empties to lose the former took the
  // latter with it -- which collapsed the blank line before the table, and a
  // markdown table without one is not a table, it is six lines of pipes.
  const body = [
    marker,
    `**${c.title || c.id}** came back \`${c.status}\` on \`${run.app_version}\`.`,
    c.note ? '' : null,
    c.note ? `> ${c.note}` : null,
    '',
    `| | |`,
    `|---|---|`,
    `| run | \`${run.id}\` |`,
    `| host | \`${run.host}\` |`,
    `| finished | ${run.finished} |`,
    `| evidence | ${run.evidence || 'unknown'} |`,
    `| shots | ${shots(c)} |`,
    '',
    run.task ? `**Task under test**\n\n> ${run.task}\n` : null,
    `Run record: \`${runDir}\` (screenshots and video alongside).`,
    '',
    '_Filed by `scripts/report-issue.mjs` from a GUI smoke run. Comments below are later cycles hitting the same failure._',
  ].filter((l) => l !== null).join('\n');

  if (!post) {
    console.log(`\n${'='.repeat(70)}\n${title}\n${'='.repeat(70)}\n${body}`);
    continue;
  }

  // Forgejo's search is substring over title+body, so the marker finds prior
  // filings and nothing else.
  const found = await api(
    `repos/${REPO}/issues?state=all&q=${encodeURIComponent(marker)}&type=issues`,
  );
  const existing = found.find((i) => (i.body || '').includes(marker));

  if (existing) {
    await api(`repos/${REPO}/issues/${existing.number}/comments`, {
      method: 'POST',
      body: JSON.stringify({
        body: `Still \`${c.status}\` on run \`${run.id}\` (${run.finished}).${c.note ? `\n\n> ${c.note}` : ''}`,
      }),
    });
    // A reopened issue is the honest state when a fixed thing breaks again.
    if (existing.state === 'closed') {
      await api(`repos/${REPO}/issues/${existing.number}`, {
        method: 'PATCH',
        body: JSON.stringify({ state: 'open' }),
      });
      console.log(`reopened #${existing.number} and commented — ${title}`);
    } else {
      console.log(`commented on #${existing.number} — ${title}`);
    }
  } else {
    const issue = await api(`repos/${REPO}/issues`, {
      method: 'POST',
      body: JSON.stringify({ title, body, labels: [] }),
    });
    console.log(`opened #${issue.number} — ${title}`);
    console.log(`  ${FORGE}/${REPO}/issues/${issue.number}`);
  }
}

if (!post) console.log(`\n${failed.length} issue(s) would be filed. Re-run with --post to file them.`);
