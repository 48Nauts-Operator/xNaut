#!/usr/bin/env node
// Build the testing dashboard: one self-contained HTML page over a directory of runs.
//
//   node scripts/testing-report.mjs                 # ~/xnaut-testing/runs -> runs/index.html
//   node scripts/testing-report.mjs <runs-dir>
//
// A run is a directory `<runs>/<host>/<id>/` holding run.json (the record),
// report.md (the agent's write-up), NN-*.png (evidence) and run.mp4. Nothing
// else is required and nothing else is read: anything able to write that shape
// shows up here, whether it ran locally or was delegated to another machine.
//
// The data is INLINED into the page rather than fetched, because a file:// page
// cannot fetch its own JSON (CORS). Images and video stay as relative paths, so
// the page is small and the 2 MB screenshots load only when scrolled to.
//
// Statuses are passed / failed / UNTESTED, and untested is the point. The tron
// run on 2026-08-10 refused to mark the Observatory scenarios passed because
// nothing was running to list. Folding that into passed reports a green run that
// tested nothing; folding it into failed cries wolf.

import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(process.argv[2] || path.join(os.homedir(), 'xnaut-testing', 'runs'));

const isDir = (p) => { try { return fs.statSync(p).isDirectory(); } catch { return false; } };
const read = (p) => { try { return fs.readFileSync(p, 'utf8'); } catch { return null; } };

function loadRuns(root) {
  const runs = [];
  for (const host of fs.readdirSync(root).sort()) {
    const hostDir = path.join(root, host);
    if (!isDir(hostDir) || host.startsWith('.')) continue;
    for (const id of fs.readdirSync(hostDir).sort()) {
      const dir = path.join(hostDir, id);
      if (!isDir(dir)) continue;
      const raw = read(path.join(dir, 'run.json'));
      if (!raw) { console.warn(`  skipped ${host}/${id}: no run.json`); continue; }
      let run;
      try { run = JSON.parse(raw); } catch (e) { console.warn(`  skipped ${host}/${id}: ${e.message}`); continue; }
      const files = fs.readdirSync(dir);
      run.host ||= host;
      run.id ||= id;
      run.rel = `${host}/${id}`;
      run.report = read(path.join(dir, 'report.md'));
      run.shots = files.filter((f) => /\.png$/i.test(f)).sort();
      run.video = files.find((f) => /\.(mp4|mov)$/i.test(f)) || null;
      run.cases ||= [];
      run.bugs ||= [];
      runs.push(run);
    }
  }
  // Newest first: the run you want is almost always the last one.
  runs.sort((a, b) => String(b.started || b.id).localeCompare(String(a.started || a.id)));
  return runs;
}

const PAGE = (data, md) => `<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>xNAUT testing</title>
<style>
:root { --bg:#0f1115; --card:#171717; --border:#262626; --fg:#fafafa; --muted:#a1a1a1;
        --pass:#7ec98f; --fail:#e98b83; --untested:#6b6f78; --accent:#f5b840; --link:#5bd1c9; }
* { box-sizing:border-box; }
body { margin:0; background:var(--bg); color:var(--fg); font:14px/1.55 -apple-system,"SF Pro Text",Segoe UI,Roboto,sans-serif; }
.wrap { max-width:1120px; margin:0 auto; padding:32px 28px 80px; display:flex; flex-direction:column; gap:22px; }
h1 { font-size:22px; margin:0; font-weight:650; letter-spacing:-.01em; }
h2 { font-size:15px; margin:0 0 12px; font-weight:600; }
.sub { color:var(--muted); font-size:13px; }
.card { background:var(--card); border:1px solid var(--border); border-radius:12px; padding:16px 18px; }
.row { display:flex; gap:16px; flex-wrap:wrap; }
.stats { display:flex; gap:10px; flex-wrap:wrap; }
.stat { background:var(--card); border:1px solid var(--border); border-radius:10px; padding:10px 16px; min-width:104px; }
.stat b { display:block; font-size:22px; font-weight:650; line-height:1.2; }
.stat span { color:var(--muted); font-size:12px; }
table { width:100%; border-collapse:collapse; font-size:13px; }
th { text-align:left; color:var(--muted); font-weight:500; padding:0 10px 8px 0; border-bottom:1px solid var(--border); }
td { padding:9px 10px 9px 0; border-bottom:1px solid var(--border); vertical-align:top; }
tr:last-child td { border-bottom:0; }
.pill { display:inline-block; font-size:11px; font-weight:600; padding:2px 8px; border-radius:99px; text-transform:uppercase; letter-spacing:.03em; }
.p-passed { background:rgba(126,201,143,.16); color:var(--pass); }
.p-failed { background:rgba(233,139,131,.16); color:var(--fail); }
.p-untested { background:rgba(107,111,120,.2); color:#9aa0aa; }
.tabs { display:flex; gap:8px; flex-wrap:wrap; }
.tab { background:var(--card); border:1px solid var(--border); color:var(--fg); border-radius:9px;
       padding:9px 14px; cursor:pointer; font:inherit; text-align:left; line-height:1.35; }
.tab:hover { border-color:#3a3a3a; }
.tab[aria-selected=true] { border-color:var(--accent); box-shadow:inset 0 0 0 1px var(--accent); }
.tab small { display:block; color:var(--muted); font-size:11px; }
.bar { display:flex; height:6px; border-radius:99px; overflow:hidden; margin-top:7px; background:var(--border); }
pre.task { white-space:pre-wrap; background:#0d0f13; border:1px solid var(--border); border-left:3px solid var(--accent);
           border-radius:8px; padding:12px 14px; margin:0; font:12.5px/1.6 "SF Mono",Menlo,monospace; color:#cfd3da; }
.shots { display:grid; grid-template-columns:repeat(auto-fill,minmax(210px,1fr)); gap:12px; }
.shots a { display:block; text-decoration:none; color:var(--muted); font-size:11.5px; }
.shots img { width:100%; border:1px solid var(--border); border-radius:8px; display:block; margin-bottom:5px; background:#000; }
video { width:100%; border:1px solid var(--border); border-radius:8px; background:#000; }
a { color:var(--link); }
.legend { display:flex; gap:14px; font-size:12px; color:var(--muted); align-items:center; }
.legend i { width:9px; height:9px; border-radius:3px; display:inline-block; margin-right:5px; }
.note { color:var(--muted); font-size:12.5px; margin-top:3px; }
.empty { color:var(--muted); font-style:italic; }
</style>
<div class="wrap">
  <header>
    <h1>xNAUT testing</h1>
    <p class="sub" id="hdr"></p>
  </header>
  <div class="stats" id="stats"></div>
  <section class="card"><h2>Bugs found by testing</h2><div id="bugs"></div></section>
  <section class="card"><h2>Runs over time</h2><div id="charts"></div></section>
  <section><h2>Runs</h2><div class="tabs" id="tabs"></div></section>
  <div id="detail"></div>
</div>
<script>${md}</script>
<script id="data" type="application/json">${JSON.stringify(data).replace(/</g, '\\u003c')}</script>
<script>
const RUNS = JSON.parse(document.getElementById('data').textContent);
const $ = (id) => document.getElementById(id);
const esc = (s) => String(s == null ? '' : s).replace(/[&<>"]/g, (c) => ({ '&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;' }[c]));
const ORDER = ['passed', 'failed', 'untested'];
const COLOR = { passed:'#7ec98f', failed:'#e98b83', untested:'#6b6f78' };
const tally = (r) => ORDER.map((s) => r.cases.filter((c) => c.status === s).length);
const when = (r) => String(r.started || r.id).replace('T', ' ').replace(/(\\.\\d+)?Z?$/, '');

function segments(counts) {
  const total = counts.reduce((a, b) => a + b, 0) || 1;
  return ORDER.map((s, i) => counts[i] ? \`<span style="width:\${counts[i] / total * 100}%;background:\${COLOR[s]}"></span>\` : '').join('');
}

// Aggregate across every run: the same defect surfacing in five runs is one row,
// with the span it survived. A bug list per run would just be five copies.
function bugRows() {
  const byKey = new Map();
  for (const r of RUNS) for (const b of r.bugs) {
    const key = b.ticket || b.title;
    const seen = byKey.get(key) || { ...b, runs: [] };
    seen.runs.push(r);
    byKey.set(key, seen);
  }
  return [...byKey.values()];
}

function renderTop() {
  const cases = RUNS.flatMap((r) => r.cases);
  const counts = ORDER.map((s) => cases.filter((c) => c.status === s).length);
  const bugs = bugRows();
  $('hdr').textContent = \`\${RUNS.length} run\${RUNS.length === 1 ? '' : 's'} across \${new Set(RUNS.map((r) => r.host)).size} machine(s). Generated \${new Date().toISOString().slice(0, 16).replace('T', ' ')}Z.\`;
  $('stats').innerHTML = [
    ['Runs', RUNS.length, ''],
    ['Passed', counts[0], COLOR.passed],
    ['Failed', counts[1], COLOR.failed],
    ['Untested', counts[2], COLOR.untested],
    ['Open bugs', bugs.filter((b) => b.status !== 'fixed').length, bugs.length ? COLOR.failed : ''],
  ].map(([k, v, c]) => \`<div class="stat"><b style="\${c ? 'color:' + c : ''}">\${v}</b><span>\${k}</span></div>\`).join('');

  $('bugs').innerHTML = !bugs.length
    ? '<p class="empty">No bugs recorded yet.</p>'
    : \`<table><thead><tr><th>Ticket</th><th>What</th><th>Seen in</th><th>First</th><th>Last</th></tr></thead><tbody>\${
        bugs.map((b) => {
          const ids = b.runs.map((r) => when(r)).sort();
          return \`<tr><td>\${b.ticket ? esc(b.ticket) : '<span class="empty">unfiled</span>'}</td>
            <td>\${esc(b.title)}\${b.note ? \`<div class="note">\${esc(b.note)}</div>\` : ''}</td>
            <td>\${b.runs.length} run\${b.runs.length === 1 ? '' : 's'}</td>
            <td>\${esc(ids[0].slice(0, 10))}</td><td>\${esc(ids[ids.length - 1].slice(0, 10))}</td></tr>\`;
        }).join('')}</tbody></table>\`;

  // Oldest on the left, so a regression reads as the bar changing shape.
  const chron = [...RUNS].reverse();
  const max = Math.max(1, ...chron.map((r) => r.cases.length));
  const W = 46, GAP = 14, H = 116;
  $('charts').innerHTML = \`
    <svg viewBox="0 0 \${Math.max(320, chron.length * (W + GAP))} \${H + 34}" style="width:100%;max-height:190px">
      \${chron.map((r, i) => {
        const c = tally(r), x = i * (W + GAP);
        let y = H;
        const bars = ORDER.map((s, k) => {
          const h = c[k] / max * (H - 8);
          y -= h;
          return h ? \`<rect x="\${x}" y="\${y}" width="\${W}" height="\${h}" fill="\${COLOR[s]}"/>\` : '';
        }).join('');
        return bars + \`<text x="\${x + W / 2}" y="\${H + 14}" fill="#a1a1a1" font-size="10" text-anchor="middle">\${esc(r.app_version || r.id.slice(0, 8))}</text>
                       <text x="\${x + W / 2}" y="\${H + 27}" fill="#6b6f78" font-size="9" text-anchor="middle">\${esc(r.host)}</text>\`;
      }).join('')}
    </svg>
    <div class="legend">\${ORDER.map((s) => \`<span><i style="background:\${COLOR[s]}"></i>\${s}</span>\`).join('')}</div>\`;

  $('tabs').innerHTML = RUNS.map((r, i) => {
    const c = tally(r);
    return \`<button class="tab" role="tab" data-i="\${i}" aria-selected="\${i === 0}">
      <b>\${esc(r.app_version || r.suite || r.id)}</b> <small>\${esc(r.host)} · \${esc(when(r))}</small>
      <span class="bar">\${segments(c)}</span></button>\`;
  }).join('');
  $('tabs').onclick = (e) => {
    const b = e.target.closest('.tab');
    if (!b) return;
    [...$('tabs').children].forEach((t) => t.setAttribute('aria-selected', t === b));
    renderRun(RUNS[+b.dataset.i]);
  };
}

function renderRun(r) {
  const c = tally(r), total = r.cases.length || 1;
  // A donut is 3 lines of stroke-dasharray on one circle; a chart library is 90 KB.
  const R = 46, C = 2 * Math.PI * R;
  let off = 0;
  const ring = ORDER.map((s, i) => {
    const len = c[i] / total * C, seg = \`<circle cx="60" cy="60" r="\${R}" fill="none" stroke="\${COLOR[s]}" stroke-width="16"
      stroke-dasharray="\${len} \${C - len}" stroke-dashoffset="\${-off}" transform="rotate(-90 60 60)"/>\`;
    off += len;
    return c[i] ? seg : '';
  }).join('');

  $('detail').innerHTML = \`
    <section class="card">
      <h2>Task sent</h2>
      \${r.task ? \`<pre class="task">\${esc(r.task)}</pre>\` : '<p class="empty">No task recorded (run started locally).</p>'}
    </section>

    <section class="card" style="margin-top:22px">
      <h2>Result</h2>
      <div class="row" style="align-items:flex-start">
        <div style="flex:0 0 132px">
          <svg viewBox="0 0 120 120" width="132" height="132">\${ring}
            <text x="60" y="58" text-anchor="middle" fill="#fafafa" font-size="24" font-weight="600">\${r.cases.length}</text>
            <text x="60" y="74" text-anchor="middle" fill="#a1a1a1" font-size="10">cases</text></svg>
        </div>
        <div style="flex:1 1 340px;min-width:280px">
          <table><tbody>\${r.cases.map((k) => \`<tr>
            <td style="width:96px"><span class="pill p-\${esc(k.status)}">\${esc(k.status)}</span></td>
            <td>\${esc(k.title || k.id)}\${k.note ? \`<div class="note">\${esc(k.note)}</div>\` : ''}</td></tr>\`).join('')
            || '<tr><td class="empty">No cases recorded.</td></tr>'}</tbody></table>
        </div>
      </div>
    </section>

    \${r.report ? '<section class="card" style="margin-top:22px"><h2>Agent report</h2><div class="xnaut-md" id="md"></div></section>' : ''}

    \${r.video ? \`<section class="card" style="margin-top:22px"><h2>Recording</h2>
      <video controls preload="metadata" src="\${esc(r.rel)}/\${esc(r.video)}"></video></section>\` : ''}

    <section class="card" style="margin-top:22px">
      <h2>Screenshots (\${r.shots.length})</h2>
      <div class="shots">\${r.shots.map((s) => \`<a href="\${esc(r.rel)}/\${esc(s)}" target="_blank">
        <img loading="lazy" src="\${esc(r.rel)}/\${esc(s)}" alt="\${esc(s)}">\${esc(s)}</a>\`).join('')
        || '<p class="empty">No screenshots.</p>'}</div>
    </section>\`;

  if (r.report) window.xnautMarkdown.renderInto($('md'), r.report);
}

renderTop();
if (RUNS.length) renderRun(RUNS[0]);
</script>
`;

const runs = loadRuns(ROOT);
if (!runs.length) {
  console.error(`No runs under ${ROOT}. Expected <runs>/<host>/<id>/run.json`);
  process.exit(1);
}
const md = read(path.join(HERE, '..', 'src', 'js', 'markdown-render.js')) || 'window.xnautMarkdown={renderInto:(el,t)=>{el.textContent=t}};';
const out = path.join(ROOT, 'index.html');
fs.writeFileSync(out, PAGE(runs, md));
console.log(`${out}  (${runs.length} run(s), ${runs.flatMap((r) => r.cases).length} cases, ${runs.flatMap((r) => r.bugs).length} bug records)`);
