#!/usr/bin/env node
// Proof of work, derived rather than remembered.
//
// André runs ten projects at once and has to show a client what was done. The
// evidence already exists: the PM control repo knows what was asked, git knows
// what changed, and tags know what shipped. Nothing joins them, so the report
// gets written from memory, which is the one source that cannot be audited.
//
// This joins them. Ticket ID in a commit message is the key; `git tag
// --contains` is the release proof. Both are free and already true.
//
//   node scripts/proof-of-work.mjs --since 2026-08-12
//   node scripts/proof-of-work.mjs --since "30 days ago" --project XNAUT --md
//
// Written 2026-08-19. Exit code 1 if a ticket is in review or done with no
// commit behind it, because that is the claim worth catching.

import { execFileSync } from 'node:child_process';
import { readdirSync, readFileSync, existsSync } from 'node:fs';
import { join } from 'node:path';
import { homedir } from 'node:os';

const arg = (name, fallback) => {
  const i = process.argv.indexOf(`--${name}`);
  return i === -1 ? fallback : process.argv[i + 1];
};
const has = (name) => process.argv.includes(`--${name}`);

const SINCE = arg('since', '7 days ago');
const ONLY = arg('project', null);
const MD = has('md');
const CONTROL = join(homedir(), '.xnaut-control');
const ROOT = arg('root', join(homedir(), 'DevHub_Studio/factory'));

const git = (repo, args) => {
  try {
    return execFileSync('git', ['-C', repo, ...args], {
      encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'], maxBuffer: 32 << 20,
    }).trim();
  } catch {
    return '';
  }
};

// Every git repo under the factory, worktrees included: a fix often lives in a
// worktree and the tag that shipped it lives there too.
const found = git(ROOT, ['rev-parse'])
  ? [ROOT]
  : execFileSync('find', [ROOT, '-maxdepth', '4', '-name', '.git', '-not', '-path', '*/node_modules/*'],
      { encoding: 'utf8', maxBuffer: 32 << 20 })
      .split('\n').filter(Boolean).map((p) => p.replace(/\/\.git$/, ''));
// A worktree sits deeper than the find above reaches, and most feature work
// lives in one. Missing them made the ticket scan report commits as absent
// while the commit listing below printed them by name.
const siblings = git(process.cwd(), ['worktree', 'list', '--porcelain'])
  .split('\n').filter((l) => l.startsWith('worktree ')).map((l) => l.slice(9));
const repos = [...new Set([...found, ...siblings])];

const cmpVersion = (a, b) => {
  const parts = (v) => v.replace(/^v/, '').split('.').map(Number);
  const [x, y] = [parts(a), parts(b)];
  for (let i = 0; i < 3; i += 1) if ((x[i] || 0) !== (y[i] || 0)) return (x[i] || 0) - (y[i] || 0);
  return 0;
};

// --full adds the commit-level evidence: everything that changed in the window,
// not only what a ticket happens to name. 77% of commits carry no ticket id, and
// a client report that drops them is wrong rather than merely incomplete.
const FULL = has('full');
// All worktrees of the repo this script lives in. They share history, so a sha
// is deduped; they do not share HEAD, so a branch-local commit is only visible
// from its own worktree.
const family = siblings;

const tickets = [];
const projectsDir = join(CONTROL, 'projects');
if (existsSync(projectsDir)) {
  for (const key of readdirSync(projectsDir)) {
    if (ONLY && key !== ONLY) continue;
    const dir = join(projectsDir, key, 'tickets');
    if (!existsSync(dir)) continue;
    for (const f of readdirSync(dir)) {
      if (!f.endsWith('.json')) continue;
      try { tickets.push(JSON.parse(readFileSync(join(dir, f), 'utf8'))); } catch {}
    }
  }
}

// One pass per repo. --grep over all ticket ids at once would be cheaper, but
// ten repos of a few thousand commits is already sub-second and this stays
// readable at 3am.
const since = ['--since', SINCE];
const evidence = new Map(); // ticket id -> [{repo, sha, subject, date, tag}]
for (const repo of repos) {
  const log = git(repo, ['log', ...since, '--format=%H%x1f%h%x1f%ad%x1f%s', '--date=short']);
  if (!log) continue;
  for (const line of log.split('\n')) {
    const [full, sha, date, subject] = line.split('\x1f');
    if (!subject) continue;
    for (const m of subject.matchAll(/\b([A-Z][A-Z0-9]{2,15}-\d+)\b/g)) {
      const id = m[1];
      if (ONLY && !id.startsWith(`${ONLY}-`)) continue;
      // A version tag is the release proof; scratch tags like
      // `pre-agent-merge-…` are not, and sort first alphabetically.
      const tags = git(repo, ['tag', '--contains', full]).split('\n').filter(Boolean);
      const tag = tags.filter((t) => /^v\d/.test(t)).sort(cmpVersion)[0] || null;
      if (!evidence.has(id)) evidence.set(id, []);
      // Worktrees of one repo share history, so the same sha shows up once per
      // worktree. The commit happened once; report it once.
      const rows = evidence.get(id);
      const seen = rows.find((r) => r.sha === sha);
      if (seen) { if (tag && !seen.tag) seen.tag = tag; continue; }
      rows.push({ repo: repo.split('/').pop(), sha, subject, date, tag });
    }
  }
}

const byId = new Map(tickets.map((t) => [t.id, t]));
const touched = [...evidence.keys()].sort();
// A ticket that claims to be finished in this window but has no commit is the
// thing a client would catch, so it is reported rather than omitted.
// A ticket can be finished long before it is moved. Before calling one
// unproven, look for its id anywhere in history, not just inside the window.
const everCommitted = (id) =>
  repos.some((r) => git(r, ['log', '--all', '-1', '--format=%h', `--grep=\\b${id}\\b`, '-E']));
const unproven = tickets.filter(
  (t) => ['review', 'done'].includes(t.status) && (t.updated_at || '') >= isoSince()
    && !evidence.has(t.id) && !everCommitted(t.id),
);

function isoSince() {
  const out = execFileSync('date', ['-u', '-v', ...relative(), '+%Y-%m-%d'], { encoding: 'utf8' }).trim();
  return out;
}
function relative() {
  const m = /^(\d+)\s+(day|week|month)/.exec(SINCE);
  if (!m) return ['-7d'];
  const unit = { day: 'd', week: 'w', month: 'm' }[m[2]];
  return [`-${m[1]}${unit}`];
}

const line = MD ? (s) => s : (s) => s;
const out = [];
out.push(MD ? `# Proof of work — since ${SINCE}` : `Proof of work — since ${SINCE}`);
out.push('');

if (FULL) {
  // One record per sha across the whole worktree family.
  const commits = new Map();
  for (const wt of family) {
    const log = git(wt, ['log', ...since, '--format=%H%x1f%h%x1f%ad%x1f%an%x1f%s', '--date=short']);
    if (!log) continue;
    for (const l of log.split('\n')) {
      const [full, sha, date, author, subject] = l.split('\x1f');
      if (!subject || commits.has(sha)) continue;
      // --numstat on the commit itself; merges are skipped because their diff
      // double-counts work already attributed to the branch commits.
      const stat = git(wt, ['show', '--numstat', '--format=', '--no-merges', full]);
      let add = 0; let del = 0; const files = [];
      for (const row of stat.split('\n').filter(Boolean)) {
        const [a, d, f] = row.split('\t');
        if (!f) continue;
        add += Number(a) || 0; del += Number(d) || 0; files.push(f);
      }
      const tags = git(wt, ['tag', '--contains', full]).split('\n').filter(Boolean)
        .filter((t) => /^v\d/.test(t)).sort(cmpVersion);
      commits.set(sha, { sha, date, author, subject, add, del, files, tag: tags[0] || null });
    }
  }

  const all = [...commits.values()].sort((a, b) => (a.date < b.date ? 1 : -1));
  const isTest = (f) => /(^|\/)tests?\//.test(f) || /\.(test|spec)\./.test(f) || /_test\.\w+$/.test(f);
  const testCommits = all.filter((c) => c.files.some(isTest) || /^test[(:]/.test(c.subject));
  const files = new Set(all.flatMap((c) => c.files));
  const releases = [...new Set(all.map((c) => c.tag).filter(Boolean))].sort(cmpVersion);
  const withTicket = all.filter((c) => /\b[A-Z][A-Z0-9]{2,15}-\d+\b/.test(c.subject));

  const h = (t) => out.push(MD ? `## ${t}` : `${t}`, '');
  h('Summary');
  out.push(`  commits         ${all.length}`);
  out.push(`  files touched   ${files.size}`);
  out.push(`  lines           +${all.reduce((n, c) => n + c.add, 0)} / -${all.reduce((n, c) => n + c.del, 0)}`);
  out.push(`  test commits    ${testCommits.length}`);
  out.push(`  ticket-linked   ${withTicket.length} of ${all.length}`);
  out.push(`  released as     ${releases.join(', ') || 'nothing tagged in window'}`);
  out.push('');

  // Every ticket that moved in the window, closed ones included. The join below
  // only lists tickets a commit happened to name, so a ticket closed without a
  // matching commit message was invisible in the report.
  const STATUSES = ['done', 'review', 'blocked', 'in_progress', 'ready', 'inbox'];
  const moved = tickets.filter((t) => (t.updated_at || '') >= isoSince());
  if (moved.length) {
    h(`Tickets moved (${moved.length})`);
    for (const status of STATUSES) {
      const rows = moved.filter((t) => t.status === status).sort((a, b) => a.id.localeCompare(b.id));
      if (!rows.length) continue;
      out.push(MD ? `### ${status} (${rows.length})` : `  ${status} (${rows.length})`);
      for (const t of rows) {
        const ev = evidence.get(t.id) || [];
        const tag = [...new Set(ev.map((r) => r.tag).filter(Boolean))].sort(cmpVersion).pop();
        const proof = ev.length ? `${ev.length} commit${ev.length > 1 ? 's' : ''}${tag ? `, ${tag}` : ''}` : 'no commit in window';
        out.push(`    ${t.id}  ${t.title}`);
        out.push(`              ${proof}`);
      }
      out.push('');
    }
  }

  // Conventional-commit type is the only grouping the history actually carries.
  const TYPES = ['feat', 'fix', 'test', 'perf', 'refactor', 'docs', 'chore'];
  const typeOf = (c) => {
    const m = /^(\w+)(\(.+?\))?!?:/.exec(c.subject);
    return m && TYPES.includes(m[1]) ? m[1] : 'other';
  };
  for (const type of [...TYPES, 'other']) {
    const rows = all.filter((c) => typeOf(c) === type);
    if (!rows.length) continue;
    h(`${type} (${rows.length})`);
    for (const c of rows) {
      const id = (/\b([A-Z][A-Z0-9]{2,15}-\d+)\b/.exec(c.subject) || [])[1];
      const ship = c.tag ? ` [${c.tag}]` : '';
      out.push(`  ${c.date}  ${c.sha}  ${c.subject}`);
      out.push(`              +${c.add}/-${c.del} in ${c.files.length} files${id ? `  ${id}` : ''}${ship}`);
    }
    out.push('');
  }
}

if (!touched.length) out.push('_No commits referencing a ticket in this window._');

for (const id of touched) {
  const t = byId.get(id);
  const rows = evidence.get(id);
  const shipped = [...new Set(rows.map((r) => r.tag).filter(Boolean))];
  const title = t ? t.title : '(no ticket in the control repo)';
  const status = t ? t.status : 'UNTRACKED';
  out.push(MD ? `### ${id} — ${title}` : `${id}  ${title}`);
  out.push(`  status: ${status}   commits: ${rows.length}   shipped: ${shipped.join(', ') || 'not yet released'}`);
  for (const r of rows) out.push(`    ${r.date}  ${r.repo}@${r.sha}  ${r.subject}`);
  if (t && Array.isArray(t.documentation) && t.documentation.length) {
    out.push(`    doc: ${t.documentation.join(', ')}`);
  }
  if (!t) out.push('    WARNING: commits reference a ticket that does not exist');
  out.push('');
}

if (unproven.length) {
  out.push(MD ? '## Claimed finished, no commit found' : 'Claimed finished, no commit found:');
  for (const t of unproven) out.push(`  ${t.id}  [${t.status}]  ${t.title}`);
  out.push('');
}

console.log(out.join('\n'));
process.exit(unproven.length ? 1 : 0);
